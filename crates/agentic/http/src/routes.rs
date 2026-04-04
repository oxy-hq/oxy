//! Route handlers:
//!   POST   /runs           — create a run, start pipeline in background
//!   GET    /runs/:id/events — SSE stream (live + postgres catch-up)
//!   POST   /runs/:id/answer — deliver user answer to a suspended run

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
use oxy_shared::OxyError;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use agentic_analytics::{
    AnalyticsEvent, AnalyticsIntent, QuestionType, analytics_step_summary, analytics_tool_summary,
    build_analytics_handlers,
    config::{AgentConfig, BuildContext},
};
use agentic_core::{
    UiTransformState,
    events::{CoreEvent, Event, EventStream},
    orchestrator::{Orchestrator, OrchestratorError},
};

use crate::{
    db, sse,
    state::{AgenticState, RunStatus},
};

use oxy::adapters::project::manager::ProjectManager;
use oxy::database::client::establish_connection;
use sea_orm::DatabaseConnection;

// ── Request / response types ──────────────────────────────────────────────────

/// Thinking mode preset for a run.
///
/// `Auto` uses the agent's default config.  `ExtendedThinking` applies the
/// `llm.extended_thinking` overrides from the agent YAML at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingMode {
    Auto,
    ExtendedThinking,
}

impl ThinkingMode {
    /// Convert to the `Option<String>` stored in the database column.
    /// `Auto` maps to `None` (the default), `ExtendedThinking` to `Some("extended_thinking")`.
    fn to_db(self) -> Option<String> {
        match self {
            Self::Auto => None,
            Self::ExtendedThinking => Some("extended_thinking".to_string()),
        }
    }

    fn is_extended(self) -> bool {
        matches!(self, Self::ExtendedThinking)
    }
}

#[derive(Deserialize)]
pub struct CreateRunRequest {
    /// Which agent config to load (`{agent_id}.agentic.yml` in project root).
    pub agent_id: String,
    pub question: String,
    /// Optional thread FK — links this run to an existing thread.
    pub thread_id: Option<String>,
    /// Thinking mode preset: `"auto"` (default) or `"extended_thinking"`.
    ///
    /// When `"extended_thinking"`, the `llm.extended_thinking` config from the agent YAML is
    /// applied as runtime overrides for model and thinking config.
    #[serde(default = "default_thinking_mode")]
    pub thinking_mode: ThinkingMode,
}

fn default_thinking_mode() -> ThinkingMode {
    ThinkingMode::Auto
}

#[derive(Serialize)]
pub struct CreateRunResponse {
    pub run_id: String,
    pub thread_id: Option<String>,
}

#[derive(Deserialize)]
pub struct AnswerRequest {
    pub answer: String,
}

#[derive(Deserialize)]
pub struct RunIdPath {
    id: String,
}

#[derive(Deserialize)]
pub struct ThreadIdPath {
    thread_id: String,
}

/// Lightweight summary returned by GET /analytics/threads/:thread_id/run
#[derive(Serialize)]
pub struct RunSummary {
    pub run_id: String,
    pub status: String,
    pub agent_id: String,
    pub question: String,
    pub answer: Option<String>,
    pub error_message: Option<String>,
    /// Thinking mode used for this run (`"auto"` or `"extended_thinking"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_mode: Option<String>,
    /// UI events replayed through UiTransformState for frontend rendering.
    /// Present only on the list endpoint; `None` on the single-run endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui_events: Option<Vec<sse::UiEvent>>,
}

// ── POST /runs ────────────────────────────────────────────────────────────────

pub async fn create_run(
    Extension(state): Extension<Arc<AgenticState>>,
    Extension(project_manager): Extension<ProjectManager>,
    Json(body): Json<CreateRunRequest>,
) -> Response {
    let run_id = Uuid::new_v4().to_string();

    let db = match establish_connection().await {
        Ok(db) => db,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response();
        }
    };

    // Load agent config lazily per request.
    let config_path = project_manager
        .config_manager
        .project_path()
        .join(&body.agent_id);
    let config = match AgentConfig::from_file(&config_path) {
        Ok(c) => c,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("agent config error: {e}")).into_response();
        }
    };

    // Persist the run record before spawning so SSE subscribers that connect
    // immediately can find it in postgres.
    let thread_id_uuid = body
        .thread_id
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok());
    if let Err(e) = db::insert_run(
        &db,
        &run_id,
        &body.agent_id,
        &body.question,
        thread_id_uuid,
        body.thinking_mode.to_db(),
    )
    .await
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response();
    }

    // Load prior completed turns for this thread so the pipeline can resolve
    // cross-turn references (e.g. "same metric", "break it down differently").
    let thread_turns = if let Some(tid) = thread_id_uuid {
        db::get_thread_history(&db, tid, 10)
            .await
            .unwrap_or_default()
    } else {
        vec![]
    };

    // Extract the most recent spec_hint for cross-turn query continuity.
    let prior_spec_hint: Option<agentic_analytics::SpecHint> = thread_turns
        .iter()
        .rev()
        .find_map(|t| t.spec_hint.as_ref())
        .and_then(|v| serde_json::from_value(v.clone()).ok());

    let history: Vec<agentic_analytics::ConversationTurn> = thread_turns
        .into_iter()
        .map(|t| agentic_analytics::ConversationTurn {
            question: t.question,
            answer: t.answer,
        })
        .collect();

    // Channel for delivering user answers to the orchestrator task on suspension.
    let (answer_tx, answer_rx) = mpsc::channel::<String>(1);
    // Cancellation signal: send `true` to stop the pipeline task.
    let (cancel_tx, cancel_rx) = watch::channel(false);
    state.register(&run_id, answer_tx, cancel_tx);

    let state2 = Arc::clone(&state);
    let run_id2 = run_id.clone();
    let base_dir = project_manager.config_manager.project_path().to_path_buf();
    let question = body.question.clone();
    let thinking_mode = body.thinking_mode;
    {
        tokio::spawn(async move {
            let _ = run_pipeline(
                state2,
                config,
                base_dir,
                run_id2,
                question,
                history,
                prior_spec_hint,
                answer_rx,
                cancel_rx,
                db,
                project_manager,
                thinking_mode,
            )
            .await;
        });
    }

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
    // `Last-Event-ID` lets the client resume after a disconnect or page refresh.
    let last_seq = headers
        .get("Last-Event-ID")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(-1);

    // Grab the notifier before any DB query so we don't miss a wake-up that
    // fires between the query and the await below.
    let notifier = state.notifiers.get(&run_id).map(|n| Arc::clone(&*n));
    let run_id = run_id.clone();

    let db = match establish_connection().await {
        Ok(db) => db,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response();
        }
    };

    let stream = async_stream::stream! {
        let mut last_sent_seq = last_seq;
        // Per-connection transformer: fresh state for each reconnect so
        // that replaying from seq=0 rebuilds current_label / fan-out state.
        let mut ui_state: UiTransformState<AnalyticsEvent> =
            UiTransformState::new()
                .with_summary_fn(analytics_step_summary)
                .with_tool_summary_fn(analytics_tool_summary);
        let mut ui_serializer = sse::UiBlockSerializer::new();

        loop {
            // Query for any new events since last_sent_seq.
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

                // Deserialize raw DB row → Event<AnalyticsEvent>, then
                // transform → UiBlock(s), then serialize → SSE event.
                if let Some(raw_event) = sse::deserialize(&row.event_type, &row.payload) {
                    for block in ui_state.process(raw_event) {
                        let (ui_event_type, ui_payload) = ui_serializer.serialize_block(&block);
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
                // else: unrecognised raw event type — skip silently
            }
            if terminal { return; }

            // If the run is no longer active (done/failed), do a final query
            // to catch any events written after we read status, then close.
            let still_active = state.notifiers.contains_key(&run_id);
            if !still_active {
                // One final sweep.
                if let Ok(final_rows) = db::get_events_after(&db, &run_id, last_sent_seq).await {
                    for row in final_rows {
                        if let Some(raw_event) = sse::deserialize(&row.event_type, &row.payload) {
                            for block in ui_state.process(raw_event) {
                                let (ui_event_type, ui_payload) = ui_serializer.serialize_block(&block);
                                let event = SseEvent::default()
                                    .id(row.seq.to_string())
                                    .event(&ui_event_type)
                                    .data(ui_payload.to_string());
                                yield Ok(event);
                            }
                        }
                    }
                }
                break;
            }

            // Park until the orchestrator task writes new events.
            match &notifier {
                Some(n) => n.notified().await,
                None => break, // run finished before we connected — loop will exit above
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
    Json(body): Json<AnswerRequest>,
) -> Response {
    let status = match state.statuses.get(&run_id) {
        Some(s) => s.clone(),
        None => return (StatusCode::NOT_FOUND, "run not found").into_response(),
    };

    let RunStatus::Suspended { .. } = status else {
        return (StatusCode::CONFLICT, "run is not suspended").into_response();
    };

    let tx = match state.answer_txs.get(&run_id) {
        Some(t) => t.clone(),
        None => {
            return (StatusCode::GONE, "orchestrator task is no longer running").into_response();
        }
    };

    if tx.send(body.answer).await.is_err() {
        return (
            StatusCode::GONE,
            "orchestrator task dropped the answer channel",
        )
            .into_response();
    }

    Json(serde_json::json!({ "ok": true })).into_response()
}

// ── POST /runs/:id/cancel ─────────────────────────────────────────────────────

pub async fn cancel_run(
    Path(RunIdPath { id: run_id }): Path<RunIdPath>,
    Extension(state): Extension<Arc<AgenticState>>,
) -> Response {
    if state.cancel(&run_id) {
        Json(serde_json::json!({ "ok": true })).into_response()
    } else {
        // The in-memory task is gone (server panic, already finished, etc.)
        // but the DB row may still be in "running" status.  Mark it failed so
        // a page refresh doesn't show a perpetual loading state.
        if let Ok(conn) = establish_connection().await {
            db::update_run_failed(&conn, &run_id, "cancelled by user")
                .await
                .ok();
        }
        state.statuses.insert(
            run_id.clone(),
            RunStatus::Failed("cancelled by user".into()),
        );
        Json(serde_json::json!({ "ok": true })).into_response()
    }
}

// ── PATCH /runs/:id/thinking_mode ────────────────────────────────────────────
//
// Updates the persisted `thinking_mode` on a *completed or idle* run record.
// This does **not** affect an in-flight pipeline — `thinking_mode` is read only
// at pipeline startup.  The primary use-case is the UI toggle: when the user
// changes the thinking mode *between* follow-up questions, the frontend patches
// the latest run so the next question inherits the correct mode.

#[derive(Deserialize)]
pub struct UpdateThinkingModeRequest {
    pub thinking_mode: Option<ThinkingMode>,
}

pub async fn update_thinking_mode(
    Path(RunIdPath { id: run_id }): Path<RunIdPath>,
    Json(body): Json<UpdateThinkingModeRequest>,
) -> Response {
    let db = match establish_connection().await {
        Ok(db) => db,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response();
        }
    };
    let thinking_mode = body.thinking_mode.unwrap_or(ThinkingMode::Auto);
    match db::update_run_thinking_mode(&db, &run_id, thinking_mode.to_db()).await {
        Ok(_) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response(),
    }
}

// ── Background pipeline task ──────────────────────────────────────────────────

#[tracing::instrument(skip_all, err, fields(
    otel.name = "analytics.run",
    oxy.span_type = "analytics",
    run_id = %run_id,
    question = %question,
))]
async fn run_pipeline(
    state: Arc<AgenticState>,
    config: AgentConfig,
    base_dir: std::path::PathBuf,
    run_id: String,
    question: String,
    history: Vec<agentic_analytics::ConversationTurn>,
    prior_spec_hint: Option<agentic_analytics::SpecHint>,
    mut answer_rx: mpsc::Receiver<String>,
    mut cancel_rx: watch::Receiver<bool>,
    db: DatabaseConnection,
    project_manager: ProjectManager,
    thinking_mode: ThinkingMode,
) -> Result<(), OxyError> {
    tracing::info!(run_id = %run_id, "pipeline started");
    // Assemble project-level context from the oxy config.yml.
    let mut build_ctx = build_project_context_pub(&config, &project_manager, &base_dir).await;
    build_ctx.schema_cache = Some(Arc::clone(&state.schema_cache));

    // Apply extended thinking mode overrides when requested by the UI toggle.
    if thinking_mode.is_extended() {
        if let Some(extended_thinking) = &config.llm.extended_thinking {
            if let Some(thinking_cfg) = &extended_thinking.thinking {
                build_ctx.thinking_override = Some(thinking_cfg.to_thinking_config());
            }
            if let Some(model) = &extended_thinking.model {
                build_ctx.model_override = Some(model.clone());
            }
        }
    }

    // Build solver from config (lazy, per request).
    let (solver, procedure_files) =
        match config.build_solver_with_context(&base_dir, build_ctx).await {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("solver build failed: {e}");
                tracing::error!(run_id = %run_id, "{msg}");
                // Write an error SSE event so the client sees the failure.
                // Use sse::serialize with a proper CoreEvent::Error so that
                // sse::deserialize can reconstruct it (it requires trace_id).
                let error_event = Event::<AnalyticsEvent>::Core(CoreEvent::Error {
                    message: msg.clone(),
                    trace_id: run_id.clone(),
                });
                let (event_type, payload) = sse::serialize(&error_event);
                db::insert_event(&db, &run_id, 0, &event_type, &payload)
                    .await
                    .ok();
                state.notify(&run_id);
                state
                    .statuses
                    .insert(run_id.clone(), RunStatus::Failed(msg.clone()));
                db::update_run_failed(&db, &run_id, &msg).await.ok();
                state.deregister(&run_id);
                return Ok(());
            }
        };

    // Wall-clock start for duration tracking.  Instant is Copy so we can use
    // it in both the bridge task closure and the pipeline loop below.
    let pipeline_start = std::time::Instant::now();

    // Wire the event channel: mpsc → postgres + Notify.
    let (event_tx, mut event_rx) = mpsc::channel::<Event<AnalyticsEvent>>(256);
    let event_stream: EventStream<AnalyticsEvent> = event_tx;

    // Keep a clone for emitting cancel events after the orchestrator is dropped.
    let cancel_event_tx = event_stream.clone();
    // Keep a clone for emitting HumanInputResolved when an answer is received.
    let resume_event_tx = event_stream.clone();

    // The solver's state handlers ignore the orchestrator's `_events` arg and
    // call solver methods that use `solver.event_tx` directly.  Wire the same
    // channel into the solver so LlmToken / ThinkingToken events are emitted.
    let solver = solver.with_events(event_stream.clone());

    // Always attach a procedure runner so search_procedures works even when
    // context globs resolve to zero files.  When procedure_files is non-empty
    // the runner searches those paths directly; otherwise it falls back to a
    // project-wide scan via list_workflows().
    let solver = {
        let runner = agentic_workflow::OxyProcedureRunner::new(project_manager)
            .with_procedure_files(procedure_files)
            .with_events(event_stream.clone());
        solver.with_procedure_runner(std::sync::Arc::new(runner))
    };

    let db2 = db.clone();
    let state2 = Arc::clone(&state);
    let run_id2 = run_id.clone();
    let bridge_handle = tokio::spawn(async move {
        let mut seq: i64 = 0;
        // Buffer of (seq, event_type, payload_json) pending a batch write.
        let mut buf: Vec<(i64, String, String)> = Vec::new();

        // Flush the buffer as a single SQLite transaction; notify SSE handlers.
        macro_rules! flush {
            () => {
                if !buf.is_empty() {
                    db::batch_insert_events(&db2, &run_id2, &buf).await.ok();
                    state2.notify(&run_id2);
                    buf.clear();
                }
            };
        }

        // Periodic flush: every 20 ms so token streams feel smooth (50 Hz).
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(20));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        tick.tick().await; // discard the immediate first tick

        loop {
            tokio::select! {
                maybe = event_rx.recv() => {
                    let event = match maybe {
                        Some(e) => e,
                        None => { flush!(); break; }
                    };

                    let (event_type, mut payload) = sse::serialize(&event);

                    // Inject wall-time into terminal events so every SSE client
                    // sees the total duration without a separate request.
                    if sse::is_terminal(&event_type)
                        && let serde_json::Value::Object(ref mut map) = payload {
                            map.insert(
                                "duration_ms".into(),
                                (pipeline_start.elapsed().as_millis() as u64).into(),
                            );
                        }

                    match event_type.as_str() {
                        "llm_token" | "thinking_token" => {
                            tracing::trace!(run_id = %run_id2, seq, %event_type);
                        }
                        _ => {
                            tracing::debug!(
                                run_id = %run_id2, seq,
                                event = %event_type,
                                data = %payload,
                            );
                        }
                    }

                    let flush_now = sse::is_terminal(&event_type)
                        || matches!(event_type.as_str(), "awaiting_input" | "human_input_resolved");
                    buf.push((seq, event_type, payload.to_string()));
                    seq += 1;

                    if flush_now { flush!(); }
                }
                _ = tick.tick() => { flush!(); }
            }
        }

        tracing::debug!(run_id = %run_id2, "event stream closed");
        state2.notify(&run_id2);
    });

    let mut orchestrator = Orchestrator::new(solver)
        .with_handlers(build_analytics_handlers())
        .with_events(event_stream);

    let initial_intent = AnalyticsIntent {
        raw_question: question,
        summary: String::new(), // populated by Clarifying stage
        question_type: QuestionType::SingleValue, // refined by Clarifying stage
        metrics: vec![],
        dimensions: vec![],
        filters: vec![],
        history,
        spec_hint: prior_spec_hint.clone(),
        selected_procedure: None,
        semantic_query: Default::default(), // populated by Clarifying stage
        semantic_confidence: 0.0,
    };

    // Drive the pipeline; loop to handle multiple ask_user suspensions.
    // Each long-running await is wrapped in select! to respect cancellation.
    let mut cancelled = false;

    let initial_result = tokio::select! {
        r = orchestrator.run(initial_intent) => Some(r),
        _ = cancel_rx.wait_for(|v| *v) => { cancelled = true; None },
    };

    if let Some(mut result) = initial_result {
        loop {
            match result {
                Ok(answer) => {
                    tracing::info!(run_id = %run_id, "pipeline done");
                    let hint_json = answer
                        .spec_hint
                        .as_ref()
                        .and_then(|h| serde_json::to_value(h).ok());
                    db::update_run_done(&db, &run_id, &answer.text, hint_json)
                        .await
                        .ok();
                    state.statuses.insert(run_id.clone(), RunStatus::Done);
                    break;
                }

                Err(OrchestratorError::Suspended {
                    questions,
                    resume_data,
                    trace_id: suspended_trace_id,
                    ..
                }) => {
                    let combined_prompt = questions
                        .iter()
                        .map(|q| q.prompt.as_str())
                        .collect::<Vec<_>>()
                        .join("\n");
                    let first_suggestions = questions
                        .first()
                        .map(|q| q.suggestions.clone())
                        .unwrap_or_default();
                    tracing::info!(run_id = %run_id, prompt = %combined_prompt, "pipeline suspended — awaiting user input");
                    db::upsert_suspension(
                        &db,
                        &run_id,
                        &combined_prompt,
                        &first_suggestions,
                        &resume_data,
                    )
                    .await
                    .ok();
                    db::update_run_suspended(&db, &run_id).await.ok();
                    state
                        .statuses
                        .insert(run_id.clone(), RunStatus::Suspended { questions });

                    // Wait for the user's answer or a cancellation signal.
                    let user_answer = tokio::select! {
                        opt = answer_rx.recv() => opt,
                        _ = cancel_rx.wait_for(|v| *v) => None,
                    };

                    let Some(answer) = user_answer else {
                        if *cancel_rx.borrow() {
                            tracing::info!(run_id = %run_id, "pipeline cancelled while suspended");
                            cancelled = true;
                        } else {
                            tracing::warn!(run_id = %run_id, "answer channel closed — abandoning run");
                            db::update_run_failed(&db, &run_id, "abandoned").await.ok();
                            state
                                .statuses
                                .insert(run_id.clone(), RunStatus::Failed("abandoned".into()));
                        }
                        break;
                    };

                    tracing::info!(run_id = %run_id, "resuming after user answer");
                    // Emit HumanInputResolved so SSE replays see the transition
                    // and don't show the suspended prompt on page refresh.
                    let _ = resume_event_tx
                        .send(Event::Core(
                            agentic_core::events::CoreEvent::HumanInputResolved {
                                answer: answer.clone(),
                                trace_id: suspended_trace_id,
                            },
                        ))
                        .await;
                    db::update_run_running(&db, &run_id).await.ok();
                    state.statuses.insert(run_id.clone(), RunStatus::Running);

                    let resume_result = tokio::select! {
                        r = orchestrator.resume(resume_data, answer) => Some(r),
                        _ = cancel_rx.wait_for(|v| *v) => None,
                    };

                    match resume_result {
                        Some(r) => result = r,
                        None => {
                            cancelled = true;
                            break;
                        }
                    }
                }

                Err(OrchestratorError::Fatal(e)) => {
                    let msg = format!("fatal: {e:?}");
                    tracing::error!(run_id = %run_id, "{msg}");
                    db::update_run_failed(&db, &run_id, &msg).await.ok();
                    state
                        .statuses
                        .insert(run_id.clone(), RunStatus::Failed(msg));
                    break;
                }

                Err(OrchestratorError::MaxIterationsExceeded) => {
                    tracing::warn!(run_id = %run_id, "max iterations exceeded");
                    db::update_run_failed(&db, &run_id, "max iterations exceeded")
                        .await
                        .ok();
                    state.statuses.insert(
                        run_id.clone(),
                        RunStatus::Failed("max iterations exceeded".into()),
                    );
                    break;
                }

                Err(OrchestratorError::ResumeNotSupported) => {
                    tracing::error!(run_id = %run_id, "resume called on solver without HITL support");
                    db::update_run_failed(&db, &run_id, "resume not supported")
                        .await
                        .ok();
                    state.statuses.insert(
                        run_id.clone(),
                        RunStatus::Failed("resume not supported".into()),
                    );
                    break;
                }
            }
        }
    }

    if cancelled {
        tracing::info!(run_id = %run_id, "pipeline cancelled by user");
        // Emit an error event so SSE subscribers get a terminal signal.
        let _ = cancel_event_tx
            .send(Event::Core(agentic_core::events::CoreEvent::Error {
                message: "cancelled by user".into(),
                trace_id: "".into(),
            }))
            .await;
        db::update_run_failed(&db, &run_id, "cancelled by user")
            .await
            .ok();
        state.statuses.insert(
            run_id.clone(),
            RunStatus::Failed("cancelled by user".into()),
        );
    }

    // Drop all event senders so the bridge task's receiver closes and the
    // task can drain its queue and exit.  We must await the bridge before
    // deregistering so the SSE final-sweep sees all events in the DB.
    drop(orchestrator);
    drop(cancel_event_tx);
    drop(resume_event_tx);
    bridge_handle.await.ok();
    state.deregister(&run_id);
    Ok(())
}

// ── GET /threads/:thread_id/runs ──────────────────────────────────────────────

pub async fn list_runs_by_thread(Path(ThreadIdPath { thread_id }): Path<ThreadIdPath>) -> Response {
    let thread_uuid = match Uuid::parse_str(&thread_id) {
        Ok(u) => u,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid thread_id").into_response(),
    };

    let db = match establish_connection().await {
        Ok(db) => db,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response();
        }
    };

    match db::get_runs_by_thread(&db, thread_uuid).await {
        Ok(runs) => {
            let mut summaries: Vec<RunSummary> = Vec::with_capacity(runs.len());
            for r in runs {
                let raw_rows = db::get_all_events(&db, &r.id).await.unwrap_or_default();
                let mut ui_state: UiTransformState<AnalyticsEvent> = UiTransformState::new()
                    .with_summary_fn(analytics_step_summary)
                    .with_tool_summary_fn(analytics_tool_summary);
                let mut ui_serializer = sse::UiBlockSerializer::new();
                let mut ui_events: Vec<sse::UiEvent> = Vec::new();
                for row in raw_rows {
                    if let Some(event) = sse::deserialize(&row.event_type, &row.payload) {
                        for block in ui_state.process(event) {
                            ui_events.push(sse::UiEvent::from_block(
                                row.seq,
                                &block,
                                &mut ui_serializer,
                            ));
                        }
                    }
                }
                summaries.push(RunSummary {
                    run_id: r.id,
                    status: r.status,
                    agent_id: r.agent_id,
                    question: r.question,
                    answer: r.answer,
                    error_message: r.error_message,
                    thinking_mode: r.thinking_mode,
                    ui_events: Some(sse::squash_deltas(ui_events)),
                });
            }
            Json(summaries).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response(),
    }
}

// ── GET /threads/:thread_id/run ───────────────────────────────────────────────

pub async fn get_run_by_thread(Path(ThreadIdPath { thread_id }): Path<ThreadIdPath>) -> Response {
    let thread_uuid = match Uuid::parse_str(&thread_id) {
        Ok(u) => u,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid thread_id").into_response(),
    };

    let db = match establish_connection().await {
        Ok(db) => db,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response();
        }
    };

    match db::get_run_by_thread(&db, thread_uuid).await {
        Ok(Some(run)) => Json(RunSummary {
            run_id: run.id,
            status: run.status,
            agent_id: run.agent_id,
            question: run.question,
            answer: run.answer,
            error_message: run.error_message,
            thinking_mode: run.thinking_mode,
            ui_events: None,
        })
        .into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "no run for this thread").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response(),
    }
}

// ── Project context builder ────────────────────────────────────────────────────

/// Assemble a [`BuildContext`] from the project `config.yml` via `ProjectManager`.
///
/// - Resolves each name in `config.databases` to a live connector via the oxy
///   [`Connector`] infrastructure (supports all database types).
/// - Resolves the LLM model config (vendor, model_ref, api_key, base_url) from
///   `config.yml` when `config.llm.model` is absent in the agent YAML.
pub(crate) async fn build_project_context_pub(
    config: &AgentConfig,
    project_manager: &ProjectManager,
    base_dir: &std::path::Path,
) -> BuildContext {
    use std::sync::Arc;

    use agentic_analytics::config::LlmVendor;
    use agentic_connector::{
        BigQueryConnector, ClickHouseConnector, ConnectorError, DatabaseConnector,
        DuckDbConnection, DuckDbConnector, LoadStrategy, PostgresConnector, SnowflakeConnector,
    };
    use oxy::config::model::{DatabaseType, DuckDBOptions, Model, SnowflakeAuthType};

    let mut ctx = BuildContext::default();

    // Resolve the project model config.
    //
    // Priority:
    //   1. `llm.ref: <name>` — explicit named model from config.yml; always
    //      resolved and sets `has_explicit_ref = true` so the vendor from the
    //      ref takes precedence even when `llm.model` is also overridden.
    //   2. Project default model — used as a fallback when neither `llm.ref`
    //      nor `llm.model` is present in the agent YAML.
    let resolve_name: Option<(&str, bool)> = if let Some(ref_name) = config.llm.model_ref.as_deref()
    {
        Some((ref_name, true))
    } else if config.llm.model.is_none() {
        project_manager
            .config_manager
            .default_model()
            .map(|n| (n, false))
    } else {
        None
    };

    if let Some((name, is_explicit_ref)) = resolve_name {
        match project_manager.config_manager.resolve_model(name) {
            Ok(model) => {
                ctx.project_model = Some(model.model_name().to_string());
                ctx.has_explicit_ref = is_explicit_ref;

                if let Some(key_var) = model.key_var() {
                    ctx.project_api_key = std::env::var(key_var).ok();
                }

                match model {
                    Model::Anthropic { config: m } => {
                        ctx.project_vendor = Some(LlmVendor::Anthropic);
                        ctx.project_base_url = m.api_url.clone();
                    }
                    Model::OpenAI { config: m } => {
                        ctx.project_vendor = Some(LlmVendor::OpenAi);
                        ctx.project_base_url = m.api_url.clone();
                    }
                    Model::Ollama { config: m } => {
                        ctx.project_vendor = Some(LlmVendor::OpenAiCompat);
                        ctx.project_api_key = Some(m.api_key.clone());
                        ctx.project_base_url = Some(m.api_url.clone());
                    }
                    Model::Google { .. } => {
                        tracing::warn!(
                            model = name,
                            "Google/Gemini models are not yet supported in analytics agents; \
                             falling back to default"
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!(model = name, "could not resolve model from config.yml: {e}");
            }
        }
    }

    // Build the effective database list: explicit `databases:` entries plus any
    // names inferred from procedure and SQL context files.  This allows the
    // `databases:` key to be omitted from the agent YAML when the databases are
    // already referenced in the context files.
    let mut effective_databases: Vec<String> = config.databases.clone();
    if let Ok(resolved) = config.resolve_context(base_dir) {
        for db in resolved.referenced_databases {
            if !effective_databases.contains(&db) {
                effective_databases.push(db);
            }
        }
    }

    // Build a native connector for each name in the effective database list.
    for db_name in &effective_databases {
        let db = match project_manager.config_manager.resolve_database(db_name) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(db = %db_name, "databases: '{}' not found in config.yml: {}", db_name, e);
                continue;
            }
        };

        let connector: Arc<dyn DatabaseConnector> = match &db.database_type {
            // ── DuckDB ──────────────────────────────────────────────────────
            DatabaseType::DuckDB(duck) => match &duck.options {
                DuckDBOptions::Local { file_search_path } => {
                    let path = match project_manager
                        .config_manager
                        .resolve_file(file_search_path)
                        .await
                    {
                        Ok(p) => std::path::PathBuf::from(p),
                        Err(e) => {
                            tracing::warn!(db = %db_name, "DuckDB: cannot resolve path: {e}");
                            continue;
                        }
                    };
                    match DuckDbConnector::from_directory(&path, LoadStrategy::View) {
                        Ok(c) => Arc::new(c),
                        Err(e) => {
                            tracing::warn!(db = %db_name, "DuckDB: {e}");
                            continue;
                        }
                    }
                }
                DuckDBOptions::DuckLake(ducklake_config) => {
                    let stmts = match ducklake_config
                        .to_duckdb_attach_stmt(&project_manager.secrets_manager)
                        .await
                    {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::warn!(db = %db_name, "DuckLake attach: {e}");
                            continue;
                        }
                    };
                    let conn = match tokio::task::spawn_blocking(move || {
                        let conn = DuckDbConnection::open_in_memory()
                            .map_err(|e| ConnectorError::ConnectionError(e.to_string()))?;
                        for stmt in &stmts {
                            conn.execute_batch(stmt)
                                .map_err(|e| ConnectorError::QueryFailed {
                                    sql: stmt.clone(),
                                    message: e.to_string(),
                                })?;
                        }
                        Ok::<_, ConnectorError>(conn)
                    })
                    .await
                    {
                        Ok(Ok(c)) => c,
                        Ok(Err(e)) => {
                            tracing::warn!(db = %db_name, "DuckLake: {e}");
                            continue;
                        }
                        Err(e) => {
                            tracing::warn!(db = %db_name, "DuckLake task: {e}");
                            continue;
                        }
                    };
                    Arc::new(DuckDbConnector::new(conn))
                }
            },

            // ── Postgres ─────────────────────────────────────────────────────
            DatabaseType::Postgres(pg) => {
                let host = pg
                    .get_host(&project_manager.secrets_manager)
                    .await
                    .unwrap_or_else(|_| "localhost".into());
                let port: u16 = pg
                    .get_port(&project_manager.secrets_manager)
                    .await
                    .unwrap_or_else(|_| "5432".into())
                    .parse()
                    .unwrap_or(5432);
                let user = pg
                    .get_user(&project_manager.secrets_manager)
                    .await
                    .unwrap_or_default();
                let password = pg
                    .get_password(&project_manager.secrets_manager)
                    .await
                    .unwrap_or_default();
                let database = pg
                    .get_database(&project_manager.secrets_manager)
                    .await
                    .unwrap_or_default();
                match PostgresConnector::new(&host, port, &user, &password, &database).await {
                    Ok(c) => Arc::new(c),
                    Err(e) => {
                        tracing::warn!(db = %db_name, "Postgres: {e}");
                        continue;
                    }
                }
            }

            // ── Redshift (Postgres-compatible) ───────────────────────────────
            DatabaseType::Redshift(rds) => {
                let host = rds
                    .get_host(&project_manager.secrets_manager)
                    .await
                    .unwrap_or_else(|_| "localhost".into());
                let port: u16 = rds
                    .get_port(&project_manager.secrets_manager)
                    .await
                    .unwrap_or_else(|_| "5439".into())
                    .parse()
                    .unwrap_or(5439);
                let user = rds
                    .get_user(&project_manager.secrets_manager)
                    .await
                    .unwrap_or_default();
                let password = rds
                    .get_password(&project_manager.secrets_manager)
                    .await
                    .unwrap_or_default();
                let database = rds
                    .get_database(&project_manager.secrets_manager)
                    .await
                    .unwrap_or_default();
                match PostgresConnector::new(&host, port, &user, &password, &database).await {
                    Ok(c) => Arc::new(c),
                    Err(e) => {
                        tracing::warn!(db = %db_name, "Redshift: {e}");
                        continue;
                    }
                }
            }

            // ── ClickHouse ───────────────────────────────────────────────────
            DatabaseType::ClickHouse(ch) => {
                let host = ch
                    .get_host(&project_manager.secrets_manager)
                    .await
                    .unwrap_or_else(|_| "localhost".into());
                let user = ch
                    .get_user(&project_manager.secrets_manager)
                    .await
                    .unwrap_or_default();
                let password = ch
                    .get_password(&project_manager.secrets_manager)
                    .await
                    .unwrap_or_default();
                let database = ch
                    .get_database(&project_manager.secrets_manager)
                    .await
                    .unwrap_or_default();
                let url = format!("http://{}:8123", host);
                match ClickHouseConnector::new(url, user, password, database).await {
                    Ok(c) => Arc::new(c),
                    Err(e) => {
                        tracing::warn!(db = %db_name, "ClickHouse: {e}");
                        continue;
                    }
                }
            }

            // ── Snowflake ────────────────────────────────────────────────────
            DatabaseType::Snowflake(sf) => {
                let password = match sf.get_password(&project_manager.secrets_manager).await {
                    Ok(p) => p,
                    Err(e) => {
                        // Only password auth is supported; skip key-pair / browser auth
                        if matches!(
                            sf.auth_type,
                            SnowflakeAuthType::Password { .. }
                                | SnowflakeAuthType::PasswordVar { .. }
                        ) {
                            tracing::warn!(db = %db_name, "Snowflake: cannot resolve password: {e}");
                        } else {
                            tracing::warn!(db = %db_name, "Snowflake: only password auth supported in agentic connector");
                        }
                        continue;
                    }
                };
                match SnowflakeConnector::new(
                    sf.account.clone(),
                    sf.username.clone(),
                    password,
                    sf.role.clone(),
                    sf.warehouse.clone(),
                    Some(sf.database.clone()),
                    sf.schema.clone(),
                )
                .await
                {
                    Ok(c) => Arc::new(c),
                    Err(e) => {
                        tracing::warn!(db = %db_name, "Snowflake: {e}");
                        continue;
                    }
                }
            }

            // ── BigQuery ─────────────────────────────────────────────────────
            DatabaseType::Bigquery(bq) => {
                let key_path = match bq.get_key_path(&project_manager.secrets_manager).await {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!(db = %db_name, "BigQuery: {e}");
                        continue;
                    }
                };
                // Resolve relative paths through config manager
                let key_path = project_manager
                    .config_manager
                    .resolve_file(&key_path)
                    .await
                    .unwrap_or(key_path);
                // project_id is embedded in the service account key file
                let project_id = extract_project_id_from_key(&key_path).unwrap_or_default();
                match BigQueryConnector::new(&key_path, project_id, bq.dataset.clone()).await {
                    Ok(c) => Arc::new(c),
                    Err(e) => {
                        tracing::warn!(db = %db_name, "BigQuery: {e}");
                        continue;
                    }
                }
            }

            // ── Unsupported ──────────────────────────────────────────────────
            DatabaseType::Mysql(_) => {
                tracing::warn!(db = %db_name, "MySQL not yet supported in agentic connector");
                continue;
            }
            // ── MotherDuck ───────────────────────────────────────────────────
            DatabaseType::MotherDuck(md) => {
                let token = match md.get_token(&project_manager.secrets_manager).await {
                    Ok(t) => t,
                    Err(e) => {
                        tracing::warn!(db = %db_name, "MotherDuck token: {e}");
                        continue;
                    }
                };
                let conn_url = match &md.database {
                    Some(db) => format!("md:{}?motherduck_token={}", db, token),
                    None => format!("md:?motherduck_token={}", token),
                };
                let conn = match tokio::task::spawn_blocking(move || {
                    DuckDbConnection::open(&conn_url)
                        .map_err(|e| ConnectorError::ConnectionError(e.to_string()))
                })
                .await
                {
                    Ok(Ok(c)) => c,
                    Ok(Err(e)) => {
                        tracing::warn!(db = %db_name, "MotherDuck: {e}");
                        continue;
                    }
                    Err(e) => {
                        tracing::warn!(db = %db_name, "MotherDuck task: {e}");
                        continue;
                    }
                };
                Arc::new(DuckDbConnector::new(conn))
            }
            DatabaseType::DOMO(_) => {
                tracing::warn!(db = %db_name, "DOMO not yet supported in agentic connector");
                continue;
            }
        };

        if ctx.extra_default_connector.is_none() {
            ctx.extra_default_connector = Some(db_name.clone());
        }
        ctx.extra_connectors.insert(db_name.clone(), connector);
    }

    ctx
}

/// Read the `project_id` field from a GCP service-account JSON key file.
fn extract_project_id_from_key(key_path: &str) -> Option<String> {
    let contents = std::fs::read_to_string(key_path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&contents).ok()?;
    v.get("project_id")?.as_str().map(|s| s.to_string())
}

// ── Headless eval entry-point ─────────────────────────────────────────────────

/// Run an agentic analytics pipeline headlessly for evaluation purposes.
///
/// Loads the `.agentic.yml` config at `config_path`, builds the solver,
/// drives the orchestrator with `prompt`, and returns the answer text.
///
/// Designed to be called from the test evaluation infrastructure.
/// Returns an error if the pipeline suspends (asks a clarifying question),
/// exceeds max iterations, or encounters a fatal domain error.
pub async fn run_agentic_eval(
    project_manager: ProjectManager,
    config_path: &std::path::Path,
    prompt: String,
) -> Result<String, oxy_shared::errors::OxyError> {
    let base_dir = project_manager.config_manager.project_path().to_path_buf();

    let config = AgentConfig::from_file(config_path).map_err(|e| {
        oxy_shared::errors::OxyError::ConfigurationError(format!(
            "Failed to load agentic config at {}: {e}",
            config_path.display()
        ))
    })?;

    let build_ctx = build_project_context_pub(&config, &project_manager, &base_dir).await;

    let (solver, procedure_files) = config
        .build_solver_with_context(&base_dir, build_ctx)
        .await
        .map_err(|e| {
            oxy_shared::errors::OxyError::RuntimeError(format!(
                "Failed to build agentic solver: {e}"
            ))
        })?;

    // Drain events into /dev/null — eval doesn't need SSE streaming.
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<Event<AnalyticsEvent>>(256);
    let event_stream: EventStream<AnalyticsEvent> = event_tx;
    tokio::spawn(async move { while event_rx.recv().await.is_some() {} });

    let solver = solver.with_events(event_stream.clone());

    // Always wire a procedure runner so search_procedures works.
    let solver = {
        let runner = agentic_workflow::OxyProcedureRunner::new(project_manager)
            .with_procedure_files(procedure_files)
            .with_events(event_stream);
        solver.with_procedure_runner(std::sync::Arc::new(runner))
    };

    let mut orchestrator = Orchestrator::new(solver).with_handlers(build_analytics_handlers());

    let intent = AnalyticsIntent {
        raw_question: prompt,
        summary: String::new(),
        question_type: QuestionType::SingleValue,
        metrics: vec![],
        dimensions: vec![],
        filters: vec![],
        history: vec![],
        spec_hint: None,
        selected_procedure: None,
        semantic_query: Default::default(),
        semantic_confidence: 0.0,
    };

    orchestrator
        .run(intent)
        .await
        .map(|answer| answer.text)
        .map_err(|e| match e {
            OrchestratorError::Suspended { questions, .. } => {
                let prompts: Vec<_> = questions.iter().map(|q| q.prompt.as_str()).collect();
                oxy_shared::errors::OxyError::RuntimeError(format!(
                    "Agentic pipeline asked a clarifying question during eval (not supported): {}",
                    prompts.join("; ")
                ))
            }
            OrchestratorError::MaxIterationsExceeded => oxy_shared::errors::OxyError::RuntimeError(
                "Agentic pipeline exceeded max iterations".to_string(),
            ),
            OrchestratorError::ResumeNotSupported => oxy_shared::errors::OxyError::RuntimeError(
                "Agentic pipeline resume not supported".to_string(),
            ),
            OrchestratorError::Fatal(domain_err) => oxy_shared::errors::OxyError::RuntimeError(
                format!("Agentic pipeline fatal error: {domain_err:?}"),
            ),
        })
}
