//! `agentic-http` — Axum HTTP routes for the agentic analytics pipeline.
//!
//! # Wiring into your axum app
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use agentic_http::{AgenticState, router};
//!
//! let state = Arc::new(AgenticState::new(shutdown_token, db));
//!
//! let app = axum::Router::new()
//!     .nest("/analytics", router(state));
//! ```
//!
//! # Routes exposed
//!
//! | Method | Path                          | Description                          |
//! |--------|-------------------------------|--------------------------------------|
//! | POST   | `/analytics/runs`             | Start a pipeline run                 |
//! | GET    | `/analytics/runs/:id/events`  | SSE stream (live + catch-up)         |
//! | POST   | `/analytics/runs/:id/answer`  | Deliver answer to a suspended run    |
//! | POST   | `/analytics/runs/:id/cancel`  | Cancel a running or suspended run    |

pub mod coordinator;
pub mod db;
pub mod routes;
pub mod sse;
pub mod state;

pub use state::{AgenticState, RunStatus};

use sea_orm::DatabaseConnection;

/// Run startup maintenance: mark any stale (running/suspended) runs as failed.
///
/// Call this once after migrations complete, before the HTTP server begins
/// accepting requests.  Idempotent — safe to call every boot.
pub async fn cleanup_stale_runs(
    db: &DatabaseConnection,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let count = db::cleanup_stale_runs(db).await?;
    if count > 0 {
        tracing::warn!(count, "marked stale agentic runs as failed on startup");
    }
    Ok(count)
}

use axum::{
    Router,
    routing::{get, patch, post},
};
use std::sync::Arc;

/// Build the analytics sub-router.  Mount with `.nest("/analytics", router::<YourState>(state))`.
pub fn router<S>(state: Arc<AgenticState>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/runs", post(routes::create_run))
        .route("/runs/{id}/events", get(routes::stream_events))
        .route("/runs/{id}/answer", post(routes::answer_run))
        .route("/runs/{id}/cancel", post(routes::cancel_run))
        .route(
            "/runs/{id}/thinking_mode",
            patch(routes::update_thinking_mode),
        )
        .route("/threads/{thread_id}/run", get(routes::get_run_by_thread))
        .route(
            "/threads/{thread_id}/runs",
            get(routes::list_runs_by_thread),
        )
        // Coordinator dashboard
        .route(
            "/coordinator/active-runs",
            get(coordinator::list_active_runs),
        )
        .route("/coordinator/runs", get(coordinator::list_runs))
        .route(
            "/coordinator/runs/{id}/tree",
            get(coordinator::get_run_tree),
        )
        .route(
            "/coordinator/recovery",
            get(coordinator::get_recovery_stats),
        )
        .route("/coordinator/queue", get(coordinator::get_queue_health))
        .route("/coordinator/live", get(coordinator::live_stream))
        .layer(axum::Extension(state))
}

/// Build the workflow sub-router. Mount with `.nest("/agentic-workflows", workflow_router(state))`.
///
/// Reuses the same [`AgenticState`] as the analytics router so cancellation
/// and the SSE event registry are shared. Workflow runs flow through the
/// runtime coordinator + worker queue exactly like analytics runs — the only
/// thing this router does is seed the queue and surface state for the UI.
pub fn workflow_router<S>(state: Arc<AgenticState>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/runs",
            post(routes::create_workflow_run).get(routes::list_runs_for_workflow),
        )
        .route("/runs/{id}", get(routes::get_workflow_run))
        // Reuse the existing SSE handler — events are domain-routed by the
        // run's `source_type`, which is `"workflow"` for runs created here.
        .route("/runs/{id}/events", get(routes::stream_events))
        .route("/runs/{id}/cancel", post(routes::cancel_workflow_run))
        .route(
            "/threads/{thread_id}/run",
            get(routes::latest_run_for_thread),
        )
        .route("/files", get(routes::list_workflow_files))
        .route("/files/{path_b64}", get(routes::get_workflow_file))
        .layer(axum::Extension(state))
}

/// Build the airway sub-router. Mount with
/// `.nest("/agentic-airway", airway_router(state))`.
///
/// Shares [`AgenticState`] with the analytics + workflow routers so
/// cancellation and the SSE event registry are common. Airway runs go
/// through the same coordinator + worker queue as workflow runs; this
/// router only seeds the queue and exposes cancel. Events reuse the
/// domain-agnostic `stream_events` handler — they're routed by the
/// run's `source_type` (`"airway"`).
pub fn airway_router<S>(state: Arc<AgenticState>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/runs",
            post(routes::create_airway_run).get(routes::list_runs_for_pipeline),
        )
        // Reuse the domain-agnostic SSE handler.
        .route("/runs/{id}/events", get(routes::stream_events))
        .route("/runs/{id}/cancel", post(routes::cancel_airway_run))
        .layer(axum::Extension(state))
}
