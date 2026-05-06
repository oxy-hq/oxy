//! Axum HTTP routes for per-user Airhouse credential access.
//!
//! Routes (all under the host's auth middleware):
//! - `GET  /airhouse/me/connection`        — coordinates only, no password.
//! - `GET  /airhouse/me/credentials`       — coordinates + plaintext password.
//! - `POST /airhouse/me/provision`         — explicit user-triggered provisioning.
//! - `POST /airhouse/me/rotate-password`   — generate new password and rotate.

pub mod handlers;

use axum::Router;
use axum::routing::{get, post};

/// Build the airhouse `/airhouse/me/*` route subtree.
///
/// The router is generic over the host application's state type — handlers
/// don't depend on app state. Mount with `app_router.merge(airhouse::api::router())`.
pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/airhouse/me/connection", get(handlers::get_connection))
        .route("/airhouse/me/credentials", get(handlers::get_credentials))
        .route("/airhouse/me/provision", post(handlers::provision))
        .route(
            "/airhouse/me/rotate-password",
            post(handlers::rotate_password),
        )
}
