//! The two routes, mounted in the staff console nest
//! (`server::router::global`, `/api/customer-apps/{id}/…`) — so the console's
//! layers have already run: owner-or-app-admin, `Action::PlatformApps`, the
//! grant's app scope, and no active assume session. Here, before anything is
//! read: a sandbox agent token is `403 sandbox_token_refused`
//! ([`RefuseSandboxAgent`]) and a publish token `403 publish_token_refused`
//! (§5.6 has not given CI `allowed_app_environments` yet). A sandbox source
//! also needs `Action::AppNonProduction` (`resolve`). FleetOk: Postgres, the
//! object store and migration connections — no working copy.

use axum::extract::Path;
use axum::{Extension, Json};
use oxy::database::client::establish_connection;
use oxy_app_core::audit::RequestActor;
use oxy_auth::types::AppPublishTokenAuth;
use uuid::Uuid;

use super::act;
use super::error::PromoteError;
use super::plan::Plan;
use super::resolve::{PromoteRequest, RollbackRequest};
use crate::server::api::custom_apps_agent_refusal::RefuseSandboxAgent;

/// `POST /api/customer-apps/{id}/environments/{to}/promote` — the plan, or
/// with no `dry_run` the act and its plan (`moved`, `migrations_applied`).
pub async fn promote(
    _: RefuseSandboxAgent,
    actor: RequestActor,
    marker: Option<Extension<AppPublishTokenAuth>>,
    Path((id, to)): Path<(Uuid, String)>,
    Json(req): Json<PromoteRequest>,
) -> Result<Json<Plan>, PromoteError> {
    if marker.is_some() {
        return Err(PromoteError::PublishToken);
    }
    let db = establish_connection()
        .await
        .map_err(|e| PromoteError::internal("connect", e))?;
    Ok(Json(act::promote(&db, &actor, id, &to, &req).await?))
}

/// `POST /api/customer-apps/{id}/environments/{name}/rollback`.
pub async fn rollback(
    _: RefuseSandboxAgent,
    actor: RequestActor,
    marker: Option<Extension<AppPublishTokenAuth>>,
    Path((id, name)): Path<(Uuid, String)>,
    Json(req): Json<RollbackRequest>,
) -> Result<Json<Plan>, PromoteError> {
    if marker.is_some() {
        return Err(PromoteError::PublishToken);
    }
    let db = establish_connection()
        .await
        .map_err(|e| PromoteError::internal("connect", e))?;
    Ok(Json(act::rollback(&db, &actor, id, &name, &req).await?))
}
