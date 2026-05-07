//! Axum HTTP routes for per-user Airhouse credential access.
//!
//! Routes (all under the host's auth middleware):
//! - `GET    /airhouse/me/connection`         — wire endpoint + role,
//!   `is_provisioned` flag. Safe; no side effects.
//! - `POST   /airhouse/me/credentials`        — mint a fresh ephemeral
//!   credential. Each call writes an audit row on the airhouse side and
//!   counts against the per-SA mint quota, so the verb has to be POST
//!   (HTTP requires GET to be safe and idempotent).
//! - `POST   /airhouse/me/provision`          — ensure tenant + service
//!   account exist. Idempotent.
//! - `DELETE /airhouse/me/tokens/:username`   — revoke a single ephemeral
//!   credential the caller previously minted.

pub mod handlers;

use axum::Router;
use axum::routing::{delete, get, post};

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
        .route("/airhouse/me/credentials", post(handlers::get_credentials))
        .route("/airhouse/me/provision", post(handlers::provision))
        .route(
            "/airhouse/me/tokens/{username}",
            delete(handlers::revoke_token),
        )
}
