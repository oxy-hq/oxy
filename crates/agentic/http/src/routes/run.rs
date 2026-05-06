//! Run lifecycle handlers: create, stream, answer, cancel, update thinking mode.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, Path},
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event as SseEvent, KeepAlive, Sse},
    },
};
use serde::Deserialize;
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use agentic_pipeline::PipelineBuilder;
use agentic_pipeline::platform::{BuilderBridges, PlatformContext};
use agentic_pipeline::{AutoAcceptInputProvider, LlmClient, OpenAiProvider};

use crate::{
    db, sse,
    state::{AgenticState, RunStatus},
};

use super::{AnswerRequest, CreateRunRequest, CreateRunResponse, RunIdPath, ThinkingMode};

/// Cap on the number of tables an onboarding request may supply — guards
/// against pathological LLM prompts and request-size blowups.
const MAX_ONBOARDING_TABLES: usize = 50;

pub async fn create_run(
    Extension(state): Extension<Arc<AgenticState>>,
    Extension(platform): Extension<Arc<dyn PlatformContext>>,
    Extension(bridges): Extension<BuilderBridges>,
    Json(body): Json<CreateRunRequest>,
) -> Response {
    tracing::info!(
        agent_id = %body.agent_id,
        domain = ?body.domain,
        thread_id = ?body.thread_id,
        "create_run: received request"
    );

    if let Some(ctx) = &body.onboarding_context
        && ctx.tables.len() > MAX_ONBOARDING_TABLES
    {
        return (
            StatusCode::BAD_REQUEST,
            format!("onboarding_context.tables exceeds limit of {MAX_ONBOARDING_TABLES} entries"),
        )
            .into_response();
    }

    let db = state.db.clone();

    let thread_id_uuid = body
        .thread_id
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok());

    // If onboarding context is provided, build the prompt server-side.
    // Otherwise use the raw question from the request.
    let question = if let Some(ctx) = &body.onboarding_context {
        ctx.build_prompt()
    } else {
        body.question.clone()
    };

    let mut builder = PipelineBuilder::new(platform.clone())
        .with_builder_bridges(bridges.clone())
        .question(&question)
        .thinking_mode(body.thinking_mode)
        .schema_cache(Arc::clone(&state.schema_cache));

    if let Some(tid) = thread_id_uuid {
        builder = builder.thread(tid);
    }
    if let Some(runner) = state.builder_test_runner.clone() {
        builder = builder.test_runner(runner);
    }
    if let Some(runner) = state.builder_app_runner.clone() {
        builder = builder.app_runner(runner);
    }

    // Onboarding auto-accepts all file_change tool calls — no HITL.
    if body.auto_accept {
        builder = builder.human_input(Arc::new(AutoAcceptInputProvider));
    }

    // During onboarding the chosen model may not be in config.yml yet (the
    // builder agent is about to write it). When onboarding_context carries a
    // model_config, build the LlmClient directly and override the pipeline's
    // default resolution.
    if let Some(mc) = body
        .onboarding_context
        .as_ref()
        .and_then(|ctx| ctx.model_config.as_ref())
    {
        let api_key = platform
            .resolve_secret(&mc.key_var)
            .await
            .unwrap_or_default();
        let client = if mc.vendor == "openai" {
            LlmClient::with_provider(OpenAiProvider::new(&api_key, &mc.model_ref))
        } else {
            LlmClient::with_model(api_key, mc.model_ref.clone())
        };
        builder = builder.with_builder_llm_client(client);
    }

    builder = if body.domain.as_deref() == Some("builder") {
        builder.builder(body.model.clone())
    } else {
        builder.analytics(&body.agent_id)
    };

    let started = match builder.start(&db).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(
                agent_id = %body.agent_id,
                domain = ?body.domain,
                thread_id = ?body.thread_id,
                error = %e,
                "create_run: pipeline start failed"
            );
            return (StatusCode::BAD_REQUEST, format!("pipeline error: {e}")).into_response();
        }
    };

    let run_id = started.run_id.clone();

    let (answer_tx, answer_rx) = mpsc::channel::<String>(1);
    let (cancel_tx, cancel_rx) = watch::channel(false);
    state.register(&run_id, answer_tx, cancel_tx);

    let runtime_state = state.runtime.clone();
    let schema_cache = Some(state.schema_cache.clone());
    let builder_test_runner = state.builder_test_runner.clone();
    let builder_app_runner = state.builder_app_runner.clone();
    tokio::spawn(async move {
        agentic_pipeline::drive_with_coordinator(
            started,
            db,
            runtime_state,
            answer_rx,
            cancel_rx,
            platform,
            Some(bridges),
            schema_cache,
            builder_test_runner,
            builder_app_runner,
        )
        .await;
    });

    Json(CreateRunResponse {
        run_id,
        thread_id: body.thread_id,
    })
    .into_response()
}

// ── GET /runs/:id/events (SSE) ────────────────────────────────────────────────

pub async fn stream_events(
    Path(RunIdPath { id: run_id }): Path<RunIdPath>,
    headers: HeaderMap,
    Extension(state): Extension<Arc<AgenticState>>,
) -> Response {
    let last_seq = headers
        .get("Last-Event-ID")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(-1);

    let notifier = state.notifiers.get(&run_id).map(|n| Arc::clone(&*n));
    let run_id = run_id.clone();

    let db = state.db.clone();

    let source_type = db::get_run(&db, &run_id)
        .await
        .ok()
        .flatten()
        .and_then(|r| r.source_type)
        .unwrap_or_else(|| "analytics".to_string());

    let registry = Arc::clone(&state.event_registry);

    let stream = async_stream::stream! {
        let mut last_sent_seq = last_seq;
        let mut processor = registry.stream_processor(&source_type);

        loop {
            let rows = match db::get_events_after(&db, &run_id, last_sent_seq).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!(run_id, "SSE db error: {e}");
                    break;
                }
            };

            let mut terminal = false;
            for row in rows {
                last_sent_seq = row.seq;

                // Pass recovery_resumed events directly (not domain events).
                if row.event_type == "recovery_resumed" {
                    let event = SseEvent::default()
                        .id(row.seq.to_string())
                        .event("recovery_resumed")
                        .data(row.payload.to_string());
                    yield Ok::<_, std::convert::Infallible>(event);
                    continue;
                }

                for (ui_event_type, mut ui_payload) in processor.process(&row.event_type, &row.payload) {
                    // Inject attempt number into every SSE payload.
                    if let serde_json::Value::Object(ref mut map) = ui_payload {
                        map.insert("attempt".into(), row.attempt.into());
                    }
                    let event = SseEvent::default()
                        .id(row.seq.to_string())
                        .event(&ui_event_type)
                        .data(ui_payload.to_string());
                    yield Ok::<_, std::convert::Infallible>(event);

                    if sse::is_terminal(&ui_event_type) {
                        terminal = true;
                    }
                }
            }
            if terminal { return; }

            let still_active = state.notifiers.contains_key(&run_id);
            if !still_active {
                if let Ok(final_rows) = db::get_events_after(&db, &run_id, last_sent_seq).await {
                    for row in final_rows {
                        if row.event_type == "recovery_resumed" {
                            let event = SseEvent::default()
                                .id(row.seq.to_string())
                                .event("recovery_resumed")
                                .data(row.payload.to_string());
                            yield Ok(event);
                            continue;
                        }
                        for (ui_event_type, mut ui_payload) in processor.process(&row.event_type, &row.payload) {
                            if let serde_json::Value::Object(ref mut map) = ui_payload {
                                map.insert("attempt".into(), row.attempt.into());
                            }
                            let event = SseEvent::default()
                                .id(row.seq.to_string())
                                .event(&ui_event_type)
                                .data(ui_payload.to_string());
                            yield Ok(event);
                        }
                    }
                }
                break;
            }

            match &notifier {
                Some(n) => {
                    tokio::select! {
                        _ = n.notified() => {},
                        _ = state.shutdown_token.cancelled() => break,
                    }
                }
                None => break,
            }
        }
    };

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

// ── POST /runs/:id/answer ─────────────────────────────────────────────────────

pub async fn answer_run(
    Path(RunIdPath { id: run_id }): Path<RunIdPath>,
    Extension(state): Extension<Arc<AgenticState>>,
    Extension(platform): Extension<Arc<dyn PlatformContext>>,
    Extension(bridges): Extension<BuilderBridges>,
    Json(body): Json<AnswerRequest>,
) -> Response {
    // Hot path: the coordinator is still alive in memory — deliver the
    // answer through the answer channel. The coordinator receives it via
    // answer_rxs and handles resume via TaskSpec::Resume (spawning a fresh
    // pipeline). We skip the status check because the coordinator processes
    // messages sequentially — it handles TaskOutcome::Suspended before
    // checking answer_rxs, so the answer is safe to buffer.
    if let Some(tx) = state.answer_txs.get(&run_id) {
        let tx = tx.clone();
        if tx.send(body.answer.clone()).await.is_ok() {
            return Json(serde_json::json!({ "ok": true, "resumed": true })).into_response();
        }
        // Coordinator dropped the answer channel — fall through to cold
        // resume so the pipeline can be rebuilt from persisted suspension data.
        tracing::warn!(
            run_id = %run_id,
            "hot-path answer channel closed, falling through to cold resume"
        );
    }

    // Cold resume: coordinator is dead (e.g. after server restart).
    // Rebuild the pipeline and drive a new coordinator.
    let db = state.db.clone();

    // Retry a few times: there is a small window where the frontend sees the
    // awaiting_input SSE event but the coordinator hasn't yet persisted the
    // suspension status to DB (e.g. if the server just restarted and the DB
    // write is in flight).
    let mut run = None;
    for attempt in 0..3 {
        match db::get_run(&db, &run_id).await {
            Ok(Some(r)) if r.task_status.as_deref() == Some("awaiting_input") => {
                run = Some(r);
                break;
            }
            Ok(Some(r)) if attempt < 2 => {
                tracing::debug!(
                    run_id = %run_id,
                    task_status = ?r.task_status,
                    attempt,
                    "answer_run: task_status not yet awaiting_input, retrying"
                );
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            Ok(Some(r)) => {
                return (
                    StatusCode::CONFLICT,
                    format!(
                        "run is not suspended (task_status: {})",
                        r.task_status.as_deref().unwrap_or("none")
                    ),
                )
                    .into_response();
            }
            Ok(None) => return (StatusCode::NOT_FOUND, "run not found").into_response(),
            Err(e) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}"))
                    .into_response();
            }
        }
    }
    let run = run.unwrap();

    let source_type = run.source_type.as_deref().unwrap_or("analytics");

    let resume_data = match db::get_suspension(&db, &run_id).await {
        Ok(Some(data)) => data,
        Ok(None) => {
            return (StatusCode::GONE, "no suspension data found for this run").into_response();
        }
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response();
        }
    };

    // Extract agent_id and model from run metadata.
    let agent_id = run
        .metadata
        .as_ref()
        .and_then(|m| m.get("agent_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    let model = run
        .metadata
        .as_ref()
        .and_then(|m| m.get("model"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Rebuild the pipeline and drive it.
    let mut builder = PipelineBuilder::new(platform.clone())
        .with_builder_bridges(bridges.clone())
        .question(&run.question)
        .schema_cache(Arc::clone(&state.schema_cache));

    if let Some(tid) = run.thread_id {
        builder = builder.thread(tid);
    }
    if let Some(runner) = state.builder_test_runner.clone() {
        builder = builder.test_runner(runner);
    }
    if let Some(runner) = state.builder_app_runner.clone() {
        builder = builder.app_runner(runner);
    }

    // Persist an input_resolved event so the SSE stream (and page reloads)
    // can see that the suspension was resolved. The trace_id matches the
    // corresponding awaiting_input event for frontend correlation.
    let answer_for_event = body.answer.clone();
    {
        let max_seq = db::get_max_seq(&db, &run_id).await.unwrap_or(-1);
        let payload =
            serde_json::json!({ "answer": answer_for_event, "trace_id": &resume_data.trace_id });
        if let Err(e) = db::insert_event(
            &db,
            &run_id,
            max_seq + 1,
            "input_resolved",
            &payload,
            run.attempt,
        )
        .await
        {
            tracing::error!(run_id = %run_id, error = %e, "failed to persist input_resolved for cold resume");
        }
    }

    let started = match builder
        .resume(
            &db,
            &run_id,
            source_type,
            &agent_id,
            model,
            resume_data,
            body.answer,
        )
        .await
    {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to resume pipeline: {e}"),
            )
                .into_response();
        }
    };

    // Register in-memory state and drive the resumed pipeline via coordinator.
    let (answer_tx, answer_rx) = mpsc::channel::<String>(1);
    let (cancel_tx, cancel_rx) = watch::channel(false);
    state.register(&run_id, answer_tx, cancel_tx);

    let runtime_state = state.runtime.clone();
    let schema_cache = Some(state.schema_cache.clone());
    let builder_test_runner = state.builder_test_runner.clone();
    let builder_app_runner = state.builder_app_runner.clone();
    tokio::spawn(async move {
        agentic_pipeline::drive_with_coordinator(
            started,
            db,
            runtime_state,
            answer_rx,
            cancel_rx,
            platform,
            Some(bridges),
            schema_cache,
            builder_test_runner,
            builder_app_runner,
        )
        .await;
    });

    Json(serde_json::json!({ "ok": true, "resumed": true })).into_response()
}

// ── POST /runs/:id/cancel ─────────────────────────────────────────────────────

pub async fn cancel_run(
    Path(RunIdPath { id: run_id }): Path<RunIdPath>,
    Extension(state): Extension<Arc<AgenticState>>,
) -> Response {
    if state.cancel(&run_id) {
        Json(serde_json::json!({ "ok": true })).into_response()
    } else {
        db::update_run_failed(&state.db, &run_id, "cancelled by user")
            .await
            .ok();
        state.statuses.insert(
            run_id.clone(),
            RunStatus::Failed("cancelled by user".into()),
        );
        Json(serde_json::json!({ "ok": true })).into_response()
    }
}

// ── PATCH /runs/:id/thinking_mode ────────────────────────────────────────────

#[derive(Deserialize)]
pub struct UpdateThinkingModeRequest {
    pub thinking_mode: Option<ThinkingMode>,
}

pub async fn update_thinking_mode(
    Path(RunIdPath { id: run_id }): Path<RunIdPath>,
    Extension(state): Extension<Arc<AgenticState>>,
    Json(body): Json<UpdateThinkingModeRequest>,
) -> Response {
    let db = state.db.clone();
    let thinking_mode = body.thinking_mode.unwrap_or(ThinkingMode::Auto);
    match db::update_run_thinking_mode(&db, &run_id, thinking_mode.to_db()).await {
        Ok(_) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response(),
    }
}

// ── GET /threads/:thread_id/runs ──────────────────────────────────────────────
