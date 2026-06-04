//! `OxyOwnerOrAppAdminGuard` — gate that allows either OXY_OWNER or an
//! entry in the `app_admins` table.
//!
//! Used for admin surfaces that span both roles (Internal Jobs in
//! particular — operators need to triage failed tasks regardless of which
//! flavor of admin they hold). Returns `403 FORBIDDEN` when neither role
//! matches and `401 UNAUTHORIZED` when no authenticated user is present.

use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use oxy_auth::types::AuthenticatedUser;

use crate::server::api::middlewares::oxy_app_admin_guard::is_oxy_app_admin;
use crate::server::api::middlewares::oxy_owner_guard::is_oxy_owner;

pub async fn oxy_owner_or_app_admin_guard_middleware(
    request: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let email = request
        .extensions()
        .get::<AuthenticatedUser>()
        .map(|u| u.email.clone())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    // Owner is a synchronous env-var check; app-admin hits the DB. Order
    // the check so owners (the more common admin caller) get the fast
    // path and we only hit the DB for non-owners.
    if is_oxy_owner(&email) {
        return Ok(next.run(request).await);
    }
    if is_oxy_app_admin(&email).await {
        return Ok(next.run(request).await);
    }
    Err(StatusCode::FORBIDDEN)
}
