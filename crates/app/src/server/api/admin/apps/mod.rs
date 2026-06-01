//! Customer-apps registry — admin endpoints and types.
//!
//! Mounted at `/api/admin/apps` via `router()`. Gated by oxy_owner_guard
//! at the router layer (in `router/global.rs`). Customer apps are routed and
//! served entirely inside oxy (see `customer_apps_serve`); no external
//! routing infra (CloudFront/Route53) sits in the data path.
//!
//! Build-time config (project_id + branch) is served publicly via
//! `GET /apps/{id}/build-config` so that CI can fetch it without any
//! per-app repo variables.

pub mod fs;
pub mod handlers;
pub mod templates;

use axum::Router;
use axum::routing::{delete, get, patch, post};

use crate::server::router::AppState;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/apps", post(handlers::create_app))
        .route("/apps", get(handlers::list_apps))
        .route("/apps/{id}", get(handlers::get_app))
        .route("/apps/{id}", patch(handlers::update_app))
        .route("/apps/{id}", delete(handlers::delete_app))
        .route(
            "/apps/{id}/publish",
            post(handlers::publish_app).delete(handlers::unpublish_app),
        )
        .route("/apps/fs/listdir", get(fs::listdir))
        .route("/apps/fs/probe", get(fs::probe))
}
