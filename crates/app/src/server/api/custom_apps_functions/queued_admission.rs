//! What a queued function run may be outside production, decided by what
//! queued it (`internal-docs/2026-09-10-custom-app-environments-design.md`
//! §4.5).
//!
//! Production runs every queued run, as it always has. Outside production a
//! queued run is admitted only as one of four things, each by its own rule:
//!
//! | Trigger | Admitted outside production when |
//! | --- | --- |
//! | **manual** (Run now, a check run) | the function is `"check": true` |
//! | **schedule_now** (Run now `as=schedule`) | the function declares a `schedule` — production included; the switch is not asked, it is a person's one-off |
//! | **scheduled** (a cron fire) | it declares a `schedule`, the environment's schedules switch is on, and a sandbox is not past its idle expiry |
//! | **webhook** | it declares a `webhook` block, and a sandbox is not past its idle expiry |
//!
//! "Declares" is read from the build the runner resolved, never trusted from
//! whoever queued the task: the environment's pointer may have moved while the
//! task waited. A deleting environment serves no build, so it never gets this
//! far. The rule is pure ([`decide`]); [`environment_state`] reads the two
//! facts it needs about the environment, and only for the triggers that ask.

use chrono::Utc;
use entity::{app_environments, apps};
use oxy_app_core::custom_app_environment::AppEnvironment;
use sea_orm::{ConnectionTrait, DbErr, EntityTrait};

use crate::server::api::custom_apps_env_resolve::ResolvedEnvironment;
use crate::server::api::custom_apps_sandboxes::activity;

/// What queued a run, as its task's trigger label says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueuedTrigger {
    /// A cron fire (`scheduled`), and every task queued before the label was
    /// carried, which were all cron fires.
    Cron,
    /// Run now, or a check run (`manual`).
    Manual,
    /// Run now as the function's schedule would run it (`schedule_now`).
    ScheduleNow,
    /// A verified inbound webhook (`webhook`).
    Webhook,
}

impl QueuedTrigger {
    /// The trigger a task's label names. An unknown label is a cron fire, as
    /// `app_function_executor::invocation_mode` reads it.
    pub(crate) fn from_label(label: Option<&str>) -> Self {
        match label {
            Some("manual") => Self::Manual,
            Some("schedule_now") => Self::ScheduleNow,
            Some("webhook") => Self::Webhook,
            _ => Self::Cron,
        }
    }
}

/// What a function declares in one build's manifest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Declared {
    pub check: bool,
    pub schedule: bool,
    pub webhook: bool,
}

impl Declared {
    /// Read from the function's `manifest_json`. A missing manifest declares
    /// nothing.
    pub(crate) fn of(manifest: Option<&serde_json::Value>) -> Self {
        Self {
            check: super::check_run::manifest_marks_check(manifest),
            schedule: manifest.and_then(declared_schedule).is_some(),
            webhook: manifest
                .and_then(super::function_webhook_contract)
                .is_some(),
        }
    }
}

/// The cron a function's manifest declares, if any — the same reading the
/// schedule rows are registered from.
pub(crate) fn declared_schedule(manifest: &serde_json::Value) -> Option<&str> {
    manifest.get("schedule").and_then(serde_json::Value::as_str)
}

/// What the rule needs to know of a non-production environment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct EnvironmentState {
    /// `app_environments.schedules_enabled`.
    pub schedules_enabled: bool,
    /// A sandbox idle past its expiry, not yet swept. Never staging.
    pub expired: bool,
}

/// Why a queued run outside production was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueuedRefusal {
    NotACheck,
    NotScheduled,
    SchedulesOff,
    NoWebhook,
    Expired,
}

/// A queued run refused by [`decide`], in the environment it named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct QueuedRefused {
    pub environment: AppEnvironment,
    pub why: QueuedRefusal,
}

impl QueuedRefused {
    /// The run's failure, in one sentence that says what to change.
    pub(crate) fn message(&self) -> String {
        let env = &self.environment;
        match self.why {
            QueuedRefusal::NotACheck => format!(
                "a manual run in the {env} environment runs only a function its build marks \
                 `\"check\": true`; run a scheduled function as its schedule would with \
                 `as=schedule`"
            ),
            QueuedRefusal::NotScheduled => format!(
                "the function declares no `schedule` in the build the {env} environment serves, \
                 so it does not run as a schedule there"
            ),
            QueuedRefusal::SchedulesOff => format!(
                "schedules are off in the {env} environment; turn them on with \
                 `oxyc env schedules <app> {env} --on`"
            ),
            QueuedRefusal::NoWebhook => format!(
                "the function declares no `webhook` block in the build the {env} environment \
                 serves"
            ),
            QueuedRefusal::Expired => format!(
                "the sandbox {env} is past its idle expiry; publish to it or call a function in \
                 it to keep it"
            ),
        }
    }
}

/// May a run queued by `trigger` run here? `production` admits everything
/// but a `schedule_now` of a function with no schedule; `state` is the
/// environment's, read by [`environment_state`] — `None` where no rule asks.
pub(crate) fn decide(
    trigger: QueuedTrigger,
    declared: Declared,
    production: bool,
    state: Option<EnvironmentState>,
) -> Result<(), QueuedRefusal> {
    let state = state.unwrap_or_default();
    let refuse_unless = |ok: bool, why| if ok { Ok(()) } else { Err(why) };
    match trigger {
        QueuedTrigger::ScheduleNow => refuse_unless(declared.schedule, QueuedRefusal::NotScheduled),
        _ if production => Ok(()),
        QueuedTrigger::Manual => refuse_unless(declared.check, QueuedRefusal::NotACheck),
        QueuedTrigger::Cron => {
            refuse_unless(declared.schedule, QueuedRefusal::NotScheduled)?;
            refuse_unless(state.schedules_enabled, QueuedRefusal::SchedulesOff)?;
            refuse_unless(!state.expired, QueuedRefusal::Expired)
        }
        QueuedTrigger::Webhook => {
            refuse_unless(declared.webhook, QueuedRefusal::NoWebhook)?;
            refuse_unless(!state.expired, QueuedRefusal::Expired)
        }
    }
}

/// The environment's state, for a cron fire or a webhook outside production —
/// the two triggers whose rule reads it. `None` for every other run, which
/// pays no read.
#[cfg_attr(not(feature = "custom-app-functions"), allow(dead_code))]
pub(crate) async fn environment_state<C: ConnectionTrait>(
    db: &C,
    app: &apps::Model,
    resolved: &ResolvedEnvironment,
    trigger: QueuedTrigger,
) -> Result<Option<EnvironmentState>, DbErr> {
    if resolved.is_production() || !matches!(trigger, QueuedTrigger::Cron | QueuedTrigger::Webhook)
    {
        return Ok(None);
    }
    let name = resolved.environment.name();
    let Some(row) = app_environments::Entity::find_by_id((app.id, name))
        .one(db)
        .await?
    else {
        return Ok(Some(EnvironmentState::default()));
    };
    let expired = matches!(resolved.environment, AppEnvironment::Dev { .. })
        && activity::is_past_idle_expiry(db, &row, Utc::now()).await?;
    Ok(Some(EnvironmentState {
        schedules_enabled: row.schedules_enabled,
        expired,
    }))
}

#[cfg(test)]
#[path = "queued_admission_tests.rs"]
mod tests;
