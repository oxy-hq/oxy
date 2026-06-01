//! `OxyAppAdminGuard` — gate for `/api/customer-apps/*`.
//!
//! Reads the authenticated user (inserted upstream by `auth_middleware`)
//! and checks the email against the DB-backed `app_admins` table
//! (replaces the legacy `OXY_APP_ADMINS` env-var allow-list). Returns
//! `403 FORBIDDEN` when the user isn't a global app admin and
//! `401 UNAUTHORIZED` when no authenticated user is present.
//!
//! Intentionally separate from `oxy_owner_guard`: app admins manage
//! customer-app registrations, owners manage org/billing/feature-flags
//! and add/remove app admins.

use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use oxy::database::client::establish_connection;
use oxy_auth::types::AuthenticatedUser;

use crate::server::api::customer_apps_auth::is_app_admin_email;

pub async fn oxy_app_admin_guard_middleware(
    request: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let email = request
        .extensions()
        .get::<AuthenticatedUser>()
        .map(|u| u.email.clone())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if !is_oxy_app_admin(&email).await {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(next.run(request).await)
}

/// Returns `true` when `email` is in the `app_admins` table. Used by
/// login responses to expose `is_app_admin` on the user payload so the
/// frontend can show the customer-apps entry point.
///
/// Wraps the cached check in [`is_app_admin_email`] and treats any DB
/// error as "not admin" — a transient outage should fail closed for
/// admin elevation rather than fail open.
pub async fn is_oxy_app_admin(email: &str) -> bool {
    let Ok(db) = establish_connection().await else {
        tracing::warn!("is_oxy_app_admin: DB connection failed; treating as non-admin");
        return false;
    };
    match is_app_admin_email(&db, email).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("is_oxy_app_admin lookup failed: {e}; treating as non-admin");
            false
        }
    }
}
