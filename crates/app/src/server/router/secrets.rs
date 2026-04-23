//! Secret routes. Each handler enforces `WorkspaceAdmin` via its extractor
//! signature, so no route-level middleware is needed.

use axum::Router;
use axum::routing::{delete, get, post, put};

use crate::api::secrets;

use super::AppState;

pub(super) fn build_secret_routes(_app_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(secrets::list_secrets))
        .route("/", post(secrets::create_secret))
        .route("/bulk", post(secrets::bulk_create_secrets))
        .route("/env", get(secrets::list_env_secrets))
        .route("/{id}", get(secrets::get_secret))
        .route("/{id}", put(secrets::update_secret))
        .route("/{id}", delete(secrets::delete_secret))
        .route("/{id}/value", get(secrets::reveal_secret))
}
