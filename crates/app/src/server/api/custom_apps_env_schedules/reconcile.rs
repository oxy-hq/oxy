//! Keep a custom app's `function` schedule rows equal to what a build
//! declares, one environment at a time.
//!
//! **Each reconcile touches its own rows and no others.** Production's — at a
//! promoting publish — are the two-segment refs `<app_id>/<function>`; an
//! environment's are `<app_id>/<env>/<function>`
//! (`agentic_pipeline::function_schedule`). Production's used to retire
//! everything under `<app_id>/`, which would erase every environment's rows
//! on each promote; it now reads the ref and leaves a three-segment one alone.
//!
//! An environment's rows follow three facts: its schedules switch, whether it
//! is being deleted, and the build it serves. Switch off, deleting or no build
//! → no rows. Reconciled on every publish to the environment and every flip of
//! the switch, under a lock per `(app, environment)` so the two never write a
//! row twice. The runner decides again when a row fires
//! (`custom_apps_functions::queued_admission`), so a row this left a moment
//! too long never runs where the rule says it may not.

use std::collections::HashSet;

use agentic_pipeline::function_schedule::FunctionScheduleRef;
use agentic_pipeline::scheduler::{self, ScheduleInput};
use agentic_runtime::entity::schedule;
use entity::{app_environments, app_functions, apps};
use oxy_app_core::custom_app_environment::AppEnvironment;
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::server::api::custom_apps_env_resolve::resolve_function_environment;
use crate::server::api::custom_apps_functions::function_task_policy;
use crate::server::api::custom_apps_functions::queued_admission::declared_schedule;

/// Whose rows one reconcile owns: production's, or one environment's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Scope<'a> {
    Production,
    Environment(&'a str),
}

impl<'a> Scope<'a> {
    fn environment(&self) -> Option<&'a str> {
        match self {
            Scope::Production => None,
            Scope::Environment(name) => Some(name),
        }
    }

    pub(super) fn owns(&self, app_id: &str, target: &FunctionScheduleRef<'_>) -> bool {
        target.app_id == app_id && target.environment == self.environment()
    }
}

/// What one reconcile did: its warnings, and how many rows it retired.
#[derive(Debug, Default)]
pub(crate) struct Applied {
    pub warnings: Vec<String>,
    pub retired: usize,
}

/// The rows `specs` — `(function name, manifest entry)` — want in `scope`:
/// one per function that declares a `schedule`, carrying its retry policy so
/// the entity-free fire arm can attach it to the queued task.
pub(super) fn wanted(
    app_id: Uuid,
    scope: Scope<'_>,
    specs: &[(String, serde_json::Value)],
) -> Vec<ScheduleInput> {
    let app = app_id.to_string();
    specs
        .iter()
        .filter_map(|(name, spec)| {
            let cron = declared_schedule(spec)?;
            let target = FunctionScheduleRef {
                app_id: &app,
                environment: scope.environment(),
                function_name: name,
            };
            let timezone = spec.get("timezone").and_then(|v| v.as_str());
            let variables = function_task_policy(spec)
                .and_then(|p| serde_json::to_value(&p).ok())
                .map(|p| serde_json::json!({ "task_policy": p }));
            Some(ScheduleInput {
                name: target.schedule_name(),
                target_kind: "function".to_string(),
                target_ref: target.target_ref(),
                question: None,
                variables,
                cron_expr: cron.to_string(),
                timezone: timezone.unwrap_or("UTC").to_string(),
                enabled: true,
            })
        })
        .collect()
}

/// Production's publish reconcile: register or update a row for every
/// function of the promoted build that declares a `schedule`, and retire
/// production's rows whose function no longer does — two-segment refs only.
/// Best-effort: a failure is a publish warning, never a failed publish. When
/// the rows cannot even be read, nothing is written — creating them blind
/// would duplicate every one that exists — and the warning says so.
pub(crate) async fn reconcile_production(
    db: &DatabaseConnection,
    app_id: Uuid,
    workspace_id: Uuid,
    specs: &[(String, serde_json::Value)],
) -> Vec<String> {
    let rows = wanted(app_id, Scope::Production, specs);
    match apply(db, workspace_id, app_id, Scope::Production, rows).await {
        Ok(applied) => applied.warnings,
        Err(e) => vec![format!(
            "this app's schedules could not be read ({e}), so none was registered or retired; \
             they keep their previous cadence until the next successful publish"
        )],
    }
}

/// An environment's reconcile: the rows its switch, its state and its build
/// call for (module docs). The warnings are the publish's or the switch's.
pub(crate) async fn reconcile_environment(
    db: &DatabaseConnection,
    app: &apps::Model,
    environment: &AppEnvironment,
) -> Result<Vec<String>, DbErr> {
    let name = environment.name();
    let lock = super::lock::ReconcileLock::acquire(db, app.id, &name).await?;
    let applied = match wanted_specs(db, app, environment).await {
        Ok(specs) => {
            let rows = wanted(app.id, Scope::Environment(&name), &specs);
            apply(db, app.project_id, app.id, Scope::Environment(&name), rows).await
        }
        Err(e) => Err(e),
    };
    lock.release().await;
    Ok(applied?.warnings)
}

/// The `(name, manifest)` of the functions an environment's rows should
/// follow: none while its switch is off, it is being deleted, or it serves no
/// build; otherwise every function of the build it serves.
async fn wanted_specs(
    db: &DatabaseConnection,
    app: &apps::Model,
    environment: &AppEnvironment,
) -> Result<Vec<(String, serde_json::Value)>, DbErr> {
    let row = app_environments::Entity::find_by_id((app.id, environment.name()))
        .one(db)
        .await?;
    if !row.is_some_and(|row| row.schedules_enabled && row.deleting_at.is_none()) {
        return Ok(Vec::new());
    }
    let resolved = resolve_function_environment(db, app, environment).await?;
    let Some(build_id) = resolved.build_id else {
        return Ok(Vec::new());
    };
    let functions = app_functions::Entity::find()
        .filter(app_functions::Column::BuildId.eq(build_id))
        .all(db)
        .await?;
    Ok(functions
        .into_iter()
        .filter_map(|f| Some((f.name, f.manifest_json?)))
        .collect())
}

/// Remove every row of `environment` of `app_id`: its teardown, which runs
/// whether or not the app row is still there. `Err` — rows that could not be
/// read, or one that could not be deleted — fails the teardown's step, so the
/// sandbox stays `deleting` and its teardown is retried rather than leaving
/// rows nothing else can remove.
pub(crate) async fn retire_environment(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    app_id: Uuid,
    environment: &AppEnvironment,
) -> Result<usize, String> {
    let name = environment.name();
    let lock = super::lock::ReconcileLock::acquire(db, app_id, &name)
        .await
        .map_err(|e| e.to_string())?;
    let applied = apply(
        db,
        workspace_id,
        app_id,
        Scope::Environment(&name),
        Vec::new(),
    )
    .await;
    lock.release().await;
    let applied = applied.map_err(|e| format!("could not read its schedules: {e}"))?;
    match applied.warnings.into_iter().next() {
        None => Ok(applied.retired),
        Some(first) => Err(first),
    }
}

/// The `function` rows of `workspace_id` that `scope` of `app_id` owns.
pub(super) async fn owned(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    app_id: Uuid,
    scope: Scope<'_>,
) -> Result<Vec<schedule::Model>, DbErr> {
    let app = app_id.to_string();
    let rows = schedule::Entity::find()
        .filter(schedule::Column::WorkspaceId.eq(workspace_id))
        .filter(schedule::Column::TargetKind.eq("function"))
        .all(db)
        .await?;
    Ok(rows
        .into_iter()
        .filter(|row| {
            FunctionScheduleRef::parse(&row.target_ref).is_some_and(|t| scope.owns(&app, &t))
        })
        .collect())
}

/// Make `scope`'s rows exactly `rows`: update the one already there for a
/// ref, create a missing one, and delete every row whose ref is no longer
/// wanted. In an environment, where only this module writes rows, a second
/// row for one ref is deleted too; production's are left as they always
/// were. A row that cannot be written is a warning; rows that cannot be
/// **read** are `Err`, and nothing is written — reading "no rows" there would
/// create duplicates and retire nothing.
async fn apply(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    app_id: Uuid,
    scope: Scope<'_>,
    rows: Vec<ScheduleInput>,
) -> Result<Applied, DbErr> {
    let existing = owned(db, workspace_id, app_id, scope)
        .await
        .inspect_err(|e| {
            tracing::warn!(%app_id, error = %e, "function schedules: could not list the rows");
        })?;
    let mut applied = Applied::default();
    let wanted_refs: HashSet<String> = rows.iter().map(|r| r.target_ref.clone()).collect();
    let mut kept: HashSet<&str> = HashSet::new();
    for input in rows {
        let target_ref = input.target_ref.clone();
        let found = existing.iter().find(|s| s.target_ref == target_ref);
        let result = match found {
            Some(s) => {
                kept.insert(s.id.as_str());
                scheduler::update_schedule(db, workspace_id, &s.id, input).await
            }
            None => scheduler::create_schedule(db, workspace_id, input).await,
        };
        if let Err(e) = result {
            applied
                .warnings
                .push(register_warning(&target_ref, scope, &e.to_string()));
        }
    }
    let duplicates_go = matches!(scope, Scope::Environment(_));
    let stale = existing.iter().filter(|s| {
        !kept.contains(s.id.as_str()) && (duplicates_go || !wanted_refs.contains(&s.target_ref))
    });
    for row in stale {
        match scheduler::delete_schedule(db, workspace_id, &row.id).await {
            Ok(()) => applied.retired += 1,
            Err(e) => applied
                .warnings
                .push(retire_warning(&row.target_ref, &e.to_string())),
        }
    }
    Ok(applied)
}

fn register_warning(target_ref: &str, scope: Scope<'_>, error: &str) -> String {
    tracing::warn!("function schedules: failed to register {target_ref}: {error}");
    let function = target_ref.rsplit('/').next().unwrap_or(target_ref);
    let place = scope
        .environment()
        .map_or(String::new(), |name| format!(" in {name}"));
    format!(
        "function '{function}' was published but its schedule{place} could not be registered \
         ({error}); it will not fire on its cron until the next successful publish"
    )
}

fn retire_warning(target_ref: &str, error: &str) -> String {
    tracing::warn!("function schedules: failed to retire {target_ref}: {error}");
    format!(
        "a stale schedule for '{target_ref}' could not be retired ({error}); it may keep firing \
         against a function that no longer exists"
    )
}

#[cfg(test)]
#[path = "reconcile_tests.rs"]
mod tests;
