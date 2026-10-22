//! The fences every per-invocation read-back route shares (`/held`,
//! `/calls`): the invocation must belong to the app in the path (one `404`
//! for an id of another app and for one that does not exist), and the caller
//! must be able to read its environment — a publish token production only, a
//! sandbox agent token only its own sandbox instance, anyone else by
//! `environment_scope::require_reach` (oxy-authz `may_open_non_production`).

use axum::http::StatusCode;
use entity::app_function_invocations;
use entity::prelude::{AppBuilds, AppFunctionInvocations};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use uuid::Uuid;

use super::environment_scope::{self, ScopeError};

/// The invocation `invocation_id` **of app `app_id`**. One `404` for an id
/// that does not exist and for one that belongs to another app.
pub(super) async fn invocation_of(
    db: &DatabaseConnection,
    app_id: Uuid,
    invocation_id: Uuid,
) -> Result<app_function_invocations::Model, ScopeError> {
    AppFunctionInvocations::find_by_id(invocation_id)
        .filter(app_function_invocations::Column::AppId.eq(app_id))
        .one(db)
        .await
        .map_err(|e| ScopeError::internal("invocation lookup failed", e))?
        .ok_or_else(invocation_not_found)
}

pub(super) fn invocation_not_found() -> ScopeError {
    ScopeError::new(
        StatusCode::NOT_FOUND,
        "invocation_not_found",
        "no such invocation of this app",
    )
}

/// Whether `caller` may read `invocation` of `app`.
///
/// Two machine credentials are told an invocation they may not read does not
/// exist — the answer for an id that names nothing, so neither can tell the
/// two apart. A **publish token** reads production's and no other: it is its
/// minter, whose reach would otherwise open staging's held list to it on the
/// `/customer-apps/{id}/…` mount a token may `GET`. A **sandbox agent token**
/// reads only an invocation of a sandbox it created.
pub(super) async fn require_readable(
    db: &DatabaseConnection,
    app: &entity::apps::Model,
    caller: &environment_scope::Caller<'_>,
    invocation: &app_function_invocations::Model,
) -> Result<(), ScopeError> {
    let environment = invocation.environment.as_str();
    if caller.marker.is_some() && !environment_scope::is_production(environment) {
        return Err(invocation_not_found());
    }
    let reach = environment_scope::require_reach(db, app, caller.user, environment).await;
    match reach {
        Err(_) if super::agent_scope::is_agent(caller.user) => Err(invocation_not_found()),
        Ok(()) => of_own_instance(db, app, caller, invocation).await,
        other => other,
    }
}

/// A sandbox agent token reads an invocation of the sandbox it has now. One
/// of an earlier sandbox that had the same name is answered as missing
/// (`custom_apps_sandbox_instance`). Every other caller passes, unread.
async fn of_own_instance(
    db: &DatabaseConnection,
    app: &entity::apps::Model,
    caller: &environment_scope::Caller<'_>,
    invocation: &app_function_invocations::Model,
) -> Result<(), ScopeError> {
    use crate::custom_apps_sandbox_instance::is_of_instance;
    let environment = invocation.environment.as_str();
    let since = super::agent_scope::instance_since_named(db, app, caller.user, environment)
        .await
        .map_err(|e| match e.status {
            StatusCode::NOT_FOUND => invocation_not_found(),
            _ => e,
        })?;
    let written_at = invocation.created_at.with_timezone(&chrono::Utc);
    match since {
        Some(since) if !is_of_instance(written_at, since) => Err(invocation_not_found()),
        _ => Ok(()),
    }
}

/// The publish's build id of build `build_id` of app `app_id`.
pub(super) async fn build_label(
    db: &DatabaseConnection,
    app_id: Uuid,
    build_id: Uuid,
) -> Result<Option<String>, ScopeError> {
    Ok(AppBuilds::find_by_id(build_id)
        .filter(entity::app_builds::Column::AppId.eq(app_id))
        .one(db)
        .await
        .map_err(|e| ScopeError::internal("app_builds lookup failed", e))?
        .map(|b| b.build_id))
}
