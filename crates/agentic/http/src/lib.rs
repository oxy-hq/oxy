//! `agentic-http` — Axum HTTP routes for the agentic analytics pipeline.
//!
//! # Wiring into your axum app
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use agentic_http::{AgenticState, router};
//!
//! let state = Arc::new(AgenticState::new());
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

pub mod app_builder_routes;
pub mod db;
pub mod routes;
pub mod sse;
pub mod state;

pub use state::{AgenticState, RunStatus};

use oxy::database::client::establish_connection;

/// Run startup maintenance: mark any stale (running/suspended) runs as failed.
///
/// Call this once after migrations complete, before the HTTP server begins
/// accepting requests.  Idempotent — safe to call every boot.
pub async fn cleanup_stale_runs() -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let db = establish_connection().await?;
    let count = db::cleanup_stale_runs(&db).await?;
    if count > 0 {
        tracing::warn!(count, "marked stale agentic runs as failed on startup");
    }
    Ok(count)
}

use axum::{
    routing::{get, post},
    Router,
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
        .route("/threads/{thread_id}/run", get(routes::get_run_by_thread))
        .route(
            "/threads/{thread_id}/runs",
            get(routes::list_runs_by_thread),
        )
        .layer(axum::Extension(state))
}

/// Build the app-builder sub-router.  Mount with `.nest("/app-builder", app_builder_router(state))`.
pub fn app_builder_router<S>(state: Arc<AgenticState>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/app-runs", post(app_builder_routes::create_app_run))
        .route(
            "/app-runs/{id}/events",
            get(app_builder_routes::stream_app_events),
        )
        .route(
            "/app-runs/{id}/answer",
            post(app_builder_routes::answer_app_run),
        )
        .route(
            "/app-runs/{id}/cancel",
            post(app_builder_routes::cancel_app_run),
        )
        .route(
            "/app-runs/{id}/retry",
            post(app_builder_routes::retry_app_run),
        )
        .route(
            "/threads/{thread_id}/runs",
            get(app_builder_routes::list_app_runs_by_thread),
        )
        .layer(axum::Extension(state))
}
