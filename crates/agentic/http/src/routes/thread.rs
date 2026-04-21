//! Thread-scoped read handlers.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, Path},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use uuid::Uuid;

use oxy_auth::extractor::AuthenticatedUserExtractor;

use crate::{db, sse, state::AgenticState};

use super::{RunSummary, ThreadIdPath};

pub async fn list_runs_by_thread(
    Path(ThreadIdPath { thread_id }): Path<ThreadIdPath>,
    Extension(state): Extension<Arc<AgenticState>>,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Response {
    let thread_uuid = match Uuid::parse_str(&thread_id) {
        Ok(u) => u,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid thread_id").into_response(),
    };

    let db = state.db.clone();

    match state.thread_owner.thread_owner(thread_uuid).await {
        Ok(None) => return (StatusCode::NOT_FOUND, "thread not found").into_response(),
        Ok(Some(Some(owner_id))) if owner_id != user.id => {
            return (StatusCode::FORBIDDEN, "access denied").into_response();
        }
        Ok(_) => {}
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response();
        }
    }

    match db::get_runs_by_thread(&db, thread_uuid).await {
        Ok(runs) => {
            let run_ids: Vec<String> = runs.iter().map(|r| r.id.clone()).collect();
            let extensions = db::get_analytics_extensions(&db, &run_ids)
                .await
                .unwrap_or_default();
            let ext_map: std::collections::HashMap<String, _> = extensions
                .into_iter()
                .map(|e| (e.run_id.clone(), e))
                .collect();

            let mut summaries: Vec<RunSummary> = Vec::with_capacity(runs.len());
            for r in runs {
                let (status, error_message) = db::get_effective_run_state(&db, &r)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::warn!(run_id = %r.id, error = %e, "get_effective_run_state failed");
                        (db::user_facing_status(r.task_status.as_deref()).to_string(), r.error_message.clone())
                    });
                let raw_rows = db::get_all_events(&db, &r.id).await.unwrap_or_else(|e| {
                    tracing::warn!(run_id = %r.id, error = %e, "get_all_events failed");
                    vec![]
                });
                let source = r.source_type.as_deref().unwrap_or("analytics");
                let mut processor = state.event_registry.stream_processor(source);
                let mut ui_events: Vec<sse::UiEvent> = Vec::new();
                for row in raw_rows {
                    // Pass recovery_resumed events directly.
                    if row.event_type == "recovery_resumed" {
                        ui_events.push(sse::UiEvent {
                            seq: row.seq,
                            event_type: "recovery_resumed".to_string(),
                            payload: row.payload,
                            attempt: row.attempt,
                        });
                        continue;
                    }
                    for (event_type, mut payload) in
                        processor.process(&row.event_type, &row.payload)
                    {
                        if let serde_json::Value::Object(ref mut map) = payload {
                            map.insert("attempt".into(), row.attempt.into());
                        }
                        ui_events.push(sse::UiEvent {
                            seq: row.seq,
                            event_type,
                            payload,
                            attempt: row.attempt,
                        });
                    }
                }
                let ext = ext_map.get(&r.id);
                summaries.push(RunSummary {
                    run_id: r.id,
                    status,
                    agent_id: ext
                        .map(|e| e.agent_id.clone())
                        .or_else(|| {
                            r.metadata
                                .as_ref()
                                .and_then(|m| m.get("agent_id"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                        })
                        .unwrap_or_default(),
                    question: r.question,
                    answer: r.answer,
                    error_message,
                    thinking_mode: ext.and_then(|e| e.thinking_mode.clone()),
                    ui_events: Some(sse::squash_deltas(ui_events)),
                });
            }
            Json(summaries).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response(),
    }
}

// ── GET /threads/:thread_id/run ───────────────────────────────────────────────

pub async fn get_run_by_thread(
    Path(ThreadIdPath { thread_id }): Path<ThreadIdPath>,
    Extension(state): Extension<Arc<AgenticState>>,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Response {
    let thread_uuid = match Uuid::parse_str(&thread_id) {
        Ok(u) => u,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid thread_id").into_response(),
    };

    let db = state.db.clone();

    match state.thread_owner.thread_owner(thread_uuid).await {
        Ok(None) => return (StatusCode::NOT_FOUND, "thread not found").into_response(),
        Ok(Some(Some(owner_id))) if owner_id != user.id => {
            return (StatusCode::FORBIDDEN, "access denied").into_response();
        }
        Ok(_) => {}
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response();
        }
    }

    match db::get_run_by_thread(&db, thread_uuid).await {
        Ok(Some(run)) => {
            let (status, error_message) = db::get_effective_run_state(&db, &run)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(run_id = %run.id, error = %e, "get_effective_run_state failed");
                    (
                        db::user_facing_status(run.task_status.as_deref()).to_string(),
                        run.error_message.clone(),
                    )
                });
            let ext = db::get_analytics_extension(&db, &run.id)
                .await
                .ok()
                .flatten();
            Json(RunSummary {
                run_id: run.id,
                status,
                agent_id: ext
                    .as_ref()
                    .map(|e| e.agent_id.clone())
                    .or_else(|| {
                        run.metadata
                            .as_ref()
                            .and_then(|m| m.get("agent_id"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_default(),
                question: run.question,
                answer: run.answer,
                error_message,
                thinking_mode: ext.and_then(|e| e.thinking_mode),
                ui_events: None,
            })
            .into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "no run for this thread").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response(),
    }
}
