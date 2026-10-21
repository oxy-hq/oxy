//! Production's function schedules on a promote or a rollback: what the move
//! would change against the schedules **registered now**, and the reconcile
//! that follows it (environments design §5.1, §5.3; §5.5a rulings 8 and 9).
//!
//! Production's schedules track the build it serves, so a move re-registers
//! them from the target build's manifest through the publish's own reconcile
//! (`custom_apps_env_schedules::reconcile::reconcile_production`, which touches
//! production's two-segment rows only). One that would be
//! retired must have been named in `confirm_schedule_removals` first
//! (`blockers`). Only production's rows are read here: a `target_ref` of
//! `<app_id>/<function>`. An environment's own rows (`<app_id>/<env>/…`) are
//! not production's and are never compared.

use std::collections::BTreeMap;

use entity::{app_builds, apps};
use sea_orm::DatabaseConnection;
use serde::Serialize;

use super::error::PromoteError;
use crate::custom_apps_env_schedules::reconcile::reconcile_production;
use crate::custom_apps_publish::function_specs;

/// One function's cadence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Cadence {
    pub function: String,
    pub cron: String,
    pub timezone: String,
}

/// A cadence that changes: what is registered, and what the build declares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CadenceChange {
    pub function: String,
    pub from: Cadence,
    pub to: Cadence,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct ScheduleChanges {
    pub added: Vec<Cadence>,
    /// Retired by the move: each needs confirming by name.
    pub removed: Vec<Cadence>,
    pub changed: Vec<CadenceChange>,
}

impl ScheduleChanges {
    pub fn removed_functions(&self) -> Vec<String> {
        self.removed.iter().map(|c| c.function.clone()).collect()
    }
}

/// `build`'s cadences against production's registered ones.
pub async fn diff(
    db: &DatabaseConnection,
    app: &apps::Model,
    build: &app_builds::Model,
) -> Result<ScheduleChanges, PromoteError> {
    let registered = registered(db, app).await?;
    Ok(compare(&registered, &declared(build)))
}

/// Re-register production's schedules from `build`'s manifest; the warnings
/// are the reconcile's (a schedule that could not be written).
pub async fn reconcile(
    db: &DatabaseConnection,
    app: &apps::Model,
    build: &app_builds::Model,
) -> Vec<String> {
    let specs = function_specs(build.manifest_json.as_ref());
    reconcile_production(db, app.id, app.project_id, &specs).await
}

/// Production's function schedules for `app`, by function.
async fn registered(
    db: &DatabaseConnection,
    app: &apps::Model,
) -> Result<BTreeMap<String, Cadence>, PromoteError> {
    let rows = agentic_pipeline::scheduler::list_schedules(db, app.project_id)
        .await
        .map_err(|e| PromoteError::internal("read the registered schedules", e))?;
    let prefix = format!("{}/", app.id);
    Ok(rows
        .into_iter()
        .filter(|s| s.target_kind == "function")
        .filter_map(|s| {
            let function = s.target_ref.strip_prefix(&prefix)?;
            // Two segments only: `<app_id>/<env>/<function>` is an
            // environment's row, not production's.
            (!function.contains('/')).then(|| {
                let cadence = Cadence {
                    function: function.to_string(),
                    cron: s.cron_expr,
                    timezone: s.timezone,
                };
                (cadence.function.clone(), cadence)
            })
        })
        .collect())
}

/// The cadences `build` declares, by function.
fn declared(build: &app_builds::Model) -> BTreeMap<String, Cadence> {
    function_specs(build.manifest_json.as_ref())
        .into_iter()
        .filter_map(|(function, spec)| {
            let cron = spec.get("schedule")?.as_str()?.to_string();
            let timezone = spec
                .get("timezone")
                .and_then(|v| v.as_str())
                .unwrap_or("UTC")
                .to_string();
            let cadence = Cadence {
                function: function.clone(),
                cron,
                timezone,
            };
            Some((function, cadence))
        })
        .collect()
}

fn compare(
    registered: &BTreeMap<String, Cadence>,
    declared: &BTreeMap<String, Cadence>,
) -> ScheduleChanges {
    let mut changes = ScheduleChanges::default();
    for (function, to) in declared {
        match registered.get(function) {
            None => changes.added.push(to.clone()),
            Some(from) if from != to => changes.changed.push(CadenceChange {
                function: function.clone(),
                from: from.clone(),
                to: to.clone(),
            }),
            Some(_) => {}
        }
    }
    changes.removed = registered
        .iter()
        .filter(|(function, _)| !declared.contains_key(*function))
        .map(|(_, cadence)| cadence.clone())
        .collect();
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cadence(function: &str, cron: &str) -> (String, Cadence) {
        (
            function.to_string(),
            Cadence {
                function: function.to_string(),
                cron: cron.to_string(),
                timezone: "UTC".to_string(),
            },
        )
    }

    #[test]
    fn a_cadence_is_added_retimed_kept_or_retired() {
        let registered = BTreeMap::from([
            cadence("kept", "0 * * * *"),
            cadence("retimed", "0 * * * *"),
            cadence("retired", "0 0 * * *"),
        ]);
        let declared = BTreeMap::from([
            cadence("kept", "0 * * * *"),
            cadence("retimed", "*/5 * * * *"),
            cadence("fresh", "0 6 * * *"),
        ]);
        let changes = compare(&registered, &declared);
        assert_eq!(changes.added, vec![cadence("fresh", "0 6 * * *").1]);
        assert_eq!(changes.removed_functions(), vec!["retired"]);
        assert_eq!(changes.changed.len(), 1);
        assert_eq!(changes.changed[0].function, "retimed");
        assert_eq!(changes.changed[0].to.cron, "*/5 * * * *");
    }

    #[test]
    fn a_build_with_no_schedules_retires_every_registered_one() {
        let registered = BTreeMap::from([cadence("nightly", "0 0 * * *")]);
        let changes = compare(&registered, &BTreeMap::new());
        assert_eq!(changes.removed_functions(), vec!["nightly"]);
        assert!(changes.added.is_empty() && changes.changed.is_empty());
    }
}
