//! Agentic-events SSE lookup.
//!
//! The legacy `/threads/{id}/task` "Build mode" endpoint was retired with the
//! classic agent system; the chat panel now drives the agentic pipeline at
//! `/analytics/runs/*` directly. The SSE event lookup for an active run is
//! the only handler that survives here.

use crate::{
    api::middlewares::workspace_context::WorkspaceManagerExtractor, service::statics::BROADCASTER,
};
use axum::extract::Query;
use axum::http::StatusCode;
use oxy::utils::create_sse_broadcast;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct AgenticEventsRequest {
    pub lookup_id: String,
}

pub async fn agentic_events(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Query(request): Query<AgenticEventsRequest>,
) -> Result<impl axum::response::IntoResponse, StatusCode> {
    let runs_manager = workspace_manager.runs_manager.as_ref().ok_or_else(|| {
        tracing::error!("Failed to initialize RunsManager");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let run_details = runs_manager
        .lookup(&request.lookup_id)
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to lookup run by lookup_id {}: {:?}",
                request.lookup_id,
                e
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            tracing::warn!("No run found for lookup_id: {}", request.lookup_id);
            StatusCode::NOT_FOUND
        })?;
    let run_info = run_details.run_info;
    let run_info = match run_info.root_ref {
        Some(root_ref) => runs_manager
            .find_run(&root_ref.source_id, root_ref.run_index)
            .await
            .map_err(|e| {
                tracing::error!("Failed to find root run: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .ok_or(StatusCode::NOT_FOUND)?,
        None => run_info,
    };

    let run_id = run_info.task_id().map_err(|_| StatusCode::BAD_REQUEST)?;
    tracing::info!("Subscribing to events for run ID: {}", run_id);
    let subscribed = BROADCASTER.subscribe(&run_id).await.map_err(|err| {
        tracing::error!("Failed to subscribe to topic {run_id}: {err}");
        Into::<StatusCode>::into(err)
    })?;
    tracing::info!("Subscribed to events for run ID: {}", run_id);
    Ok(axum::response::sse::Sse::new(create_sse_broadcast(
        subscribed.items,
        subscribed.receiver,
    )))
}
