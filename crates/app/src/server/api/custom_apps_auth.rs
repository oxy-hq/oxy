//! The request-facing half of custom-app access control: the auth flow every
//! handler calls ([`authenticate_and_authorize`]) and its outcome, which take
//! axum headers and the shared serve resolver and so stay in `oxy-app`.
//!
//! The decisions themselves (who may open an app, app and org roles, the
//! access cache, the admin bootstrap) live in `oxy_custom_apps::auth`, which
//! has no HTTP types and is re-exported here under the old path.

use axum::http::StatusCode;
use entity::{app_members, apps};
use oxy::database::client::establish_connection;
use tracing::error;
use uuid::Uuid;

pub use oxy_custom_apps::auth::*;

// ── Auth helper ──────────────────────────────────────────────────────────────

/// Narrow an [`AuthOutcome`] from "may open this app" to "may operate it".
///
/// [`authenticate_and_authorize`] ends at [`user_can_access_app`], which for a
/// default-visibility published app is true for **every member of the owning
/// org** — the app's ordinary viewers. That is the right gate for the bundle's
/// own bytes. It is the wrong gate for anything that exposes the app's
/// *operator-facing* internals: server-side `ctx.log()` output (which routinely
/// carries query results and upstream API responses the author printed while
/// debugging), and client stacks resolved against source maps (original file
/// paths and function names). An app's data is what it chose to show a viewer;
/// its log output is not, and "the same gate the app's own data already has" is
/// not a defence for the second one.
///
/// The verdict comes from `oxy-authz`'s `Ring::AppAdmin` via
/// [`resolve_app_role`] — never a hand-rolled staff/owner check here — so it
/// already covers Oxy staff, the org owner and an app-admin row.
pub(crate) async fn require_app_admin(outcome: &AuthOutcome) -> Result<(), StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        error!("db connect failed for app-admin check: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let role = resolve_app_role(&db, &outcome.caller, &outcome.app)
        .await
        .map_err(|e| {
            error!("app role lookup failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if role == Some(app_members::ROLE_ADMIN) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

/// What the auth flow returns on success.
pub(crate) struct AuthOutcome {
    pub app: apps::Model,
    /// The builds the app's environments serve, from the same cached
    /// resolution as `app`: what a hot path (`/fn`) resolves production from
    /// without a query of its own.
    pub environments: oxy_custom_apps::env_resolve::EnvironmentBuilds,
    pub user_id: Uuid,
    /// `None` for a frontline worker enrolled without a mailbox. Reaches an
    /// app as `ctx.user.email`, so an app that assumes a string gets `null`
    /// rather than `""` — the difference between "has no address" and "has an
    /// address that is empty" is exactly what a notification feature needs.
    pub user_email: Option<String>,
    /// `users.name`. Display identity, carried so a function reading
    /// `ctx.user.name` doesn't have to take the client's word for it — the whole
    /// value of server-side identity is that it can't be spoofed by the caller.
    pub user_name: String,
    /// `users.picture`, when they have one.
    pub user_picture: Option<String>,
    pub is_staff: bool,
    /// The user with the credential the request arrived with — what every
    /// further authorization question about this request must be asked of, so
    /// an API token's grants and standing flags hold on the custom-app paths too.
    pub caller: oxy_server_authz::Caller,
}

/// Authenticate the request and confirm the caller has access to
/// (org, app). Returns the app row + user info on success; an HTTP
/// status on any failure.
///
/// One call into the shared resolver (`custom_apps_serve::resolve`), so the
/// function, health, debug, logs and availability surfaces run the same
/// cached chain the bundle path does: user, rows and verdict all from the
/// 60 s caches, zero queries on a warm replica. The statuses are the
/// resolver's refusals, unchanged: a credential naming nobody is `401`; an
/// unknown org or app, or one outside an API token's grant, is `404`; a known
/// app the caller may not open is `403`.
///
/// Access is decided by the production rule whatever environment the request
/// addresses — a surface that serves a non-production environment asks
/// `may_open_environment` itself afterwards, as `/fn` does.
///
/// `sandbox_agent` is this caller's answer to a sandbox agent token. `/fn` and
/// `/logs` admit one; `/errors`, `/debug`, `/health` and `/availability` share
/// this function and refuse it, which is why the answer is an argument and not
/// a property of the function (sandbox agent credential design §3.1).
pub(crate) async fn authenticate_and_authorize(
    headers: &axum::http::HeaderMap,
    org_slug: &str,
    app_slug: &str,
    sandbox_agent: oxy_auth::token::SandboxAgent,
) -> Result<AuthOutcome, axum::http::StatusCode> {
    let request = super::custom_apps_serve::resolve::resolve_app_request(
        headers,
        org_slug,
        app_slug,
        sandbox_agent,
        &oxy_app_core::custom_app_environment::AppEnvironment::Production,
    )
    .await
    .map_err(|refusal| refusal.status())?;
    let user = request.user;
    Ok(AuthOutcome {
        app: request.app,
        environments: request.environments,
        user_id: user.id,
        user_email: user.email,
        user_name: user.name,
        user_picture: user.picture,
        is_staff: request.is_staff,
        caller: request.caller,
    })
}
