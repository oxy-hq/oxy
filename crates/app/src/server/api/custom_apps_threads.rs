//! Bundle-gated chat history for the `@oxy-hq/sdk/shell` Ask dock.
//!
//! - `GET /api/projects/{project_id}/threads` — the viewer's recent
//!   threads in this project (id + title + time), for the dock's History
//!   list.
//! - `GET /api/projects/{project_id}/threads/{thread_id}` — a thread's
//!   transcript, rebuilt for restore: the human questions paired with each
//!   run's persisted events replayed through the SAME processor the live
//!   SSE stream uses, so the SDK reconstructs the reasoning trace, charts,
//!   and answer identically.
//!
//! Both are pure Postgres reads behind `check_custom_app_gates` →
//! FleetOk (deliberately not pinned in `role_manifest.rs`). Viewer-scoped
//! (`user_id`), so `Cache-Control: private, no-store` and no shared cache
//! (`oxy-customer-apps-perf`).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use entity::prelude::{Messages, Threads};
use entity::{messages, threads};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::server::api::custom_apps_gates::check_custom_app_gates;
use crate::server::router::AppState;

const HISTORY_LIMIT: u64 = 40;

#[derive(Serialize)]
struct ThreadSummary {
    id: Uuid,
    title: String,
    created_at: String,
}

fn thread_title(t: &threads::Model) -> String {
    if t.title.trim().is_empty() {
        t.input.chars().take(80).collect()
    } else {
        t.title.clone()
    }
}

pub async fn list_threads(Path(project_id): Path<Uuid>, headers: HeaderMap) -> Response {
    let ctx = match check_custom_app_gates(&headers, project_id).await {
        Ok(c) => c,
        Err(r) => return r,
    };

    let rows = match Threads::find()
        .filter(threads::Column::UserId.eq(Some(ctx.user.id)))
        .filter(threads::Column::ProjectId.eq(project_id))
        .filter(threads::Column::Source.eq("custom-app"))
        .order_by_desc(threads::Column::CreatedAt)
        .limit(HISTORY_LIMIT)
        .all(&ctx.db)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("list_threads query failed: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "thread list failed").into_response();
        }
    };

    let out: Vec<ThreadSummary> = rows
        .iter()
        .map(|t| ThreadSummary {
            id: t.id,
            title: thread_title(t),
            created_at: t.created_at.to_rfc3339(),
        })
        .collect();

    ([(header::CACHE_CONTROL, "private, no-store")], Json(out)).into_response()
}

#[derive(Serialize)]
struct UiEvent {
    #[serde(rename = "type")]
    event_type: String,
    data: Value,
}

#[derive(Serialize)]
struct TranscriptTurn {
    question: String,
    events: Vec<UiEvent>,
}

#[derive(Serialize)]
struct TranscriptResponse {
    title: String,
    turns: Vec<TranscriptTurn>,
}

pub async fn get_thread_transcript(
    State(app_state): State<AppState>,
    Path((project_id, thread_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Response {
    let ctx = match check_custom_app_gates(&headers, project_id).await {
        Ok(c) => c,
        Err(r) => return r,
    };

    // Ownership: the thread must belong to this viewer in this project.
    // A 404 (not 403) avoids confirming the existence of others' threads.
    let thread = match Threads::find_by_id(thread_id).one(&ctx.db).await {
        Ok(Some(t)) => t,
        Ok(None) => return (StatusCode::NOT_FOUND, "thread not found").into_response(),
        Err(e) => {
            tracing::error!("transcript thread lookup failed: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "thread lookup failed").into_response();
        }
    };
    // Source gate: transcript pairing below is positional (question i ↔
    // run i), which only holds for custom-app threads — main-app
    // threads can have more messages than runs (clarification resume).
    if thread.project_id != project_id
        || thread.user_id != Some(ctx.user.id)
        || thread.source != "custom-app"
    {
        return (StatusCode::NOT_FOUND, "thread not found").into_response();
    }

    let agentic_state = match app_state.agentic_state.as_ref() {
        Some(s) => s,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "agent runtime not configured",
            )
                .into_response();
        }
    };

    // Questions (human messages) in ask order — one per run.
    let questions: Vec<String> = match Messages::find()
        .filter(messages::Column::ThreadId.eq(thread_id))
        .filter(messages::Column::IsHuman.eq(true))
        .order_by_asc(messages::Column::CreatedAt)
        .all(&ctx.db)
        .await
    {
        Ok(rows) => rows.into_iter().map(|m| m.content).collect(),
        Err(e) => {
            tracing::error!("transcript messages failed: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "messages failed").into_response();
        }
    };

    // Runs for the thread, oldest first — each run is one answer turn.
    let runs = match agentic_runtime::crud::get_runs_by_thread(&agentic_state.db, thread_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("transcript runs failed: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "runs failed").into_response();
        }
    };

    let mut turns: Vec<TranscriptTurn> = Vec::with_capacity(runs.len().max(questions.len()));
    for (i, run) in runs.iter().enumerate() {
        let source_type = run
            .source_type
            .clone()
            .unwrap_or_else(|| "analytics".to_string());
        let mut processor = agentic_state.event_registry.stream_processor(&source_type);
        let rows = agentic_runtime::crud::get_events_after(&agentic_state.db, &run.id, -1)
            .await
            .unwrap_or_default();
        let mut events: Vec<UiEvent> = Vec::new();
        for row in rows {
            // Runtime pass-through events (e.g. recovery_resumed) aren't
            // domain-specific; forward them raw like the live stream does.
            if row.event_type == "recovery_resumed" {
                events.push(UiEvent {
                    event_type: "recovery_resumed".to_string(),
                    data: row.payload,
                });
                continue;
            }
            for (ui_type, ui_payload) in processor.process(&row.event_type, &row.payload) {
                events.push(UiEvent {
                    event_type: ui_type,
                    data: ui_payload,
                });
            }
        }
        turns.push(TranscriptTurn {
            question: questions.get(i).cloned().unwrap_or_default(),
            events,
        });
    }
    // Trailing questions without a persisted run (rare) still surface.
    for q in questions.into_iter().skip(runs.len()) {
        turns.push(TranscriptTurn {
            question: q,
            events: Vec::new(),
        });
    }

    let body = TranscriptResponse {
        title: thread_title(&thread),
        turns,
    };
    ([(header::CACHE_CONTROL, "private, no-store")], Json(body)).into_response()
}
