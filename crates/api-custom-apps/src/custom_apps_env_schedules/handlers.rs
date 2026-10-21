//! `GET` and `PUT /api/customer-apps/{id}/environments/{name}/schedules` —
//! read and flip a non-production environment's schedules switch
//! (`internal-docs/custom-app-sandboxes.md` §5.1).
//!
//! Mounted beside the sandbox routes in `server::router::global`, behind the
//! same staff console layers, and admitted by the same three checks
//! (`custom_apps_sandboxes::handlers::admit`): a publish token is refused, the
//! app must exist, and the caller must be able to open the environment
//! (`Action::AppNonProduction`). The flip also refuses a sandbox agent token
//! (`RefuseSandboxAgent`): turning schedules on costs warehouse reads and
//! third-party calls on a timer, which is staff's to decide; a token is not
//! on this route's allow-list either.

use axum::extract::Path;
use axum::{Extension, Json};
use chrono::{DateTime, FixedOffset};
use entity::{app_environments, apps};
use oxy_app_core::audit::{self, RequestActor};
use oxy_app_core::custom_app_environment::AppEnvironment;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_auth::types::AppPublishTokenAuth;
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::SwitchError;
use super::reconcile::{Scope, owned, reconcile_environment};
use crate::custom_apps_sandboxes::SandboxError;
use crate::custom_apps_sandboxes::handlers::{Admitted, Door, admit, parse};
use crate::custom_apps_sandboxes::ops::scoped;
use oxy_app_core::agent_refusal::RefuseSandboxAgent;

/// `PUT` body.
#[derive(Debug, Deserialize)]
pub struct SwitchRequest {
    pub enabled: bool,
}

/// One schedule row of the environment, as the scheduler last left it.
#[derive(Debug, Serialize)]
pub struct ScheduleRow {
    pub id: String,
    pub function: String,
    pub cron_expr: String,
    pub timezone: String,
    pub next_run_at: DateTime<FixedOffset>,
    pub last_fired_at: Option<DateTime<FixedOffset>>,
    pub last_run_id: Option<String>,
    pub last_error: Option<String>,
}

/// The switch and the rows it has today. `warnings` is what the reconcile a
/// `PUT` ran could not do; always `[]` on a `GET`.
#[derive(Debug, Serialize)]
pub struct EnvironmentSchedules {
    pub environment: String,
    pub enabled: bool,
    pub schedules: Vec<ScheduleRow>,
    pub warnings: Vec<String>,
}

/// `name`, when it is an environment with a switch: production has none.
fn switchable(name: &str) -> Result<AppEnvironment, SwitchError> {
    match parse(name)? {
        AppEnvironment::Production => Err(SwitchError::Production),
        environment => Ok(environment),
    }
}

/// `GET …/environments/{name}/schedules`
pub async fn show(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    marker: Option<Extension<AppPublishTokenAuth>>,
    Path((id, name)): Path<(Uuid, String)>,
) -> Result<Json<EnvironmentSchedules>, SwitchError> {
    let Admitted { db, app, .. } = admit(&user, marker, id, Door::Named(&name)).await?;
    let environment = switchable(&name)?;
    let row = environment_row(&db, &app, &environment).await?;
    let shown = read_back(&db, &app, &environment, row.schedules_enabled).await?;
    Ok(Json(shown))
}

/// `PUT …/environments/{name}/schedules` `{"enabled": bool}` — flip the
/// switch, reconcile the environment's rows to it, and answer the read-back.
///
/// The flip is audited the moment it lands, before the reconcile: the switch
/// is what decides whether the environment fires, and a reconcile that then
/// fails (its lock busy, the database away) leaves it flipped. Such a
/// failure is a warning on the answer, not an error — the next publish to
/// the environment, or flipping it again, reconciles the rows.
pub async fn set(
    _: RefuseSandboxAgent,
    actor: RequestActor,
    marker: Option<Extension<AppPublishTokenAuth>>,
    Path((id, name)): Path<(Uuid, String)>,
    Json(body): Json<SwitchRequest>,
) -> Result<Json<EnvironmentSchedules>, SwitchError> {
    let Admitted { db, app, .. } = admit(&actor.user, marker, id, Door::Named(&name)).await?;
    let environment = switchable(&name)?;
    flip(&db, &app, &environment, body.enabled).await?;
    let entry = audit::AuditEntry::for_request(&actor, "app.environment.schedules")
        .metadata(serde_json::json!({ "enabled": body.enabled }));
    audit::record_best_effort(&db, scoped(entry, &app, &environment)).await;
    let warnings = reconcile_environment(&db, &app, &environment)
        .await
        .unwrap_or_else(|e| vec![unreconciled(&environment, &e)]);
    let mut shown = read_back(&db, &app, &environment, body.enabled).await?;
    shown.warnings = warnings;
    Ok(Json(shown))
}

/// The warning a flip answers when its reconcile did not run to the end.
fn unreconciled(environment: &AppEnvironment, e: &sea_orm::DbErr) -> String {
    tracing::warn!(%environment, error = %e, "schedules switch: the reconcile failed");
    format!(
        "the switch was set, but the {environment} environment's schedule rows could not be \
         reconciled to it ({e}); set it again, or publish to the environment, to retry"
    )
}

/// The environment's row: `NotFound` when there is none — a sandbox nobody
/// created, or a staging nothing was ever published to.
async fn environment_row(
    db: &DatabaseConnection,
    app: &apps::Model,
    environment: &AppEnvironment,
) -> Result<app_environments::Model, SandboxError> {
    app_environments::Entity::find_by_id((app.id, environment.name()))
        .one(db)
        .await
        .map_err(|e| SandboxError::db("read the environment", e))?
        .ok_or_else(|| SandboxError::NotFound(environment.name()))
}

/// Set the switch on a row that is not being deleted; one that is answers
/// `Deleting`, a missing one `NotFound`. Not a pointer move, so not activity.
async fn flip(
    db: &DatabaseConnection,
    app: &apps::Model,
    environment: &AppEnvironment,
    enabled: bool,
) -> Result<(), SandboxError> {
    let flipped = app_environments::Entity::update_many()
        .col_expr(
            app_environments::Column::SchedulesEnabled,
            Expr::value(enabled),
        )
        .filter(app_environments::Column::AppId.eq(app.id))
        .filter(app_environments::Column::Name.eq(environment.name()))
        .filter(app_environments::Column::DeletingAt.is_null())
        .exec(db)
        .await
        .map_err(|e| SandboxError::db("set the schedules switch", e))?;
    if flipped.rows_affected == 1 {
        return Ok(());
    }
    environment_row(db, app, environment).await?;
    Err(SandboxError::Deleting(environment.name()))
}

/// The switch as `enabled`, and the environment's rows as they are now.
async fn read_back(
    db: &DatabaseConnection,
    app: &apps::Model,
    environment: &AppEnvironment,
    enabled: bool,
) -> Result<EnvironmentSchedules, SandboxError> {
    let name = environment.name();
    let rows = owned(db, app.project_id, app.id, Scope::Environment(&name))
        .await
        .map_err(|e| SandboxError::db("read the environment's schedules", e))?;
    let mut schedules: Vec<ScheduleRow> = rows
        .into_iter()
        .map(|row| ScheduleRow {
            function: row
                .target_ref
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_string(),
            id: row.id,
            cron_expr: row.cron_expr,
            timezone: row.timezone,
            next_run_at: row.next_run_at,
            last_fired_at: row.last_fired_at,
            last_run_id: row.last_run_id,
            last_error: row.last_error,
        })
        .collect();
    schedules.sort_by(|a, b| a.function.cmp(&b.function));
    Ok(EnvironmentSchedules {
        environment: name,
        enabled,
        schedules,
        warnings: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_has_no_switch_and_a_bad_name_is_invalid() {
        assert!(matches!(
            switchable("production"),
            Err(SwitchError::Production)
        ));
        assert!(matches!(
            switchable("Staging"),
            Err(SwitchError::Refused(SandboxError::InvalidName(_)))
        ));
        assert_eq!(switchable("staging").ok(), Some(AppEnvironment::Staging));
        assert_eq!(
            switchable("dev-a1").ok(),
            Some(AppEnvironment::Dev {
                handle: "a1".into()
            })
        );
    }
}
