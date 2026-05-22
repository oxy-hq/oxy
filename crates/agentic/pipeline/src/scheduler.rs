//! Phase 2 scheduler: the periodic tick + schedule CRUD facade.
//!
//! Composition only — no execution logic. The tick reads due
//! `agentic_schedules` rows, CAS-advances `next_run_at`, and fires via the
//! existing seed fns with `TaskScope::Global` so the Phase-1 standalone
//! consumer drives them.
//!
//! - Exactly-once across replicas without leader election: the advance is
//!   a conditional UPDATE on the observed `next_run_at` (mirrors the
//!   `run_sequences` / driver-lease CAS idiom). Only the replica whose
//!   UPDATE affects one row fires the schedule.
//! - Misfire = run-once-then-resume: `next_run_at` is recomputed *strictly
//!   after now* ([`agentic_runtime::cron::next_occurrence_after`]), so
//!   missed slots during an outage collapse to one catch-up run.

use agentic_runtime::cron::{count_occurrences_between, next_occurrence_after, validate_cron};
use agentic_runtime::entity::schedule;
use sea_orm::ActiveValue::Set;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, Statement,
};
use serde::Deserialize;

use crate::airway_run::{StartAirwayRequest, start_airway_run};
use crate::workflow_run::{StartWorkflowRequest, start_workflow_run};

/// Create/update payload. Deserialized straight from the HTTP body.
#[derive(Debug, Clone, Deserialize)]
pub struct ScheduleInput {
    pub name: String,
    /// `"workflow"` | `"airway"`.
    pub target_kind: String,
    /// `workflow_ref` / `pipeline_ref`, workspace-relative.
    pub target_ref: String,
    #[serde(default)]
    pub variables: Option<serde_json::Value>,
    pub cron_expr: String,
    #[serde(default = "default_tz")]
    pub timezone: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_tz() -> String {
    "UTC".to_string()
}
fn default_enabled() -> bool {
    true
}

/// Validation error vs. an internal DB error — lets the HTTP layer map to
/// 400 vs 500 without string-sniffing.
#[derive(Debug)]
pub enum ScheduleError {
    Invalid(String),
    Db(DbErr),
    NotFound,
}

impl std::fmt::Display for ScheduleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScheduleError::Invalid(m) => write!(f, "{m}"),
            ScheduleError::Db(e) => write!(f, "db error: {e}"),
            ScheduleError::NotFound => write!(f, "schedule not found"),
        }
    }
}

impl From<DbErr> for ScheduleError {
    fn from(e: DbErr) -> Self {
        ScheduleError::Db(e)
    }
}

fn validate_input(input: &ScheduleInput) -> Result<(), ScheduleError> {
    if input.name.trim().is_empty() {
        return Err(ScheduleError::Invalid("name must not be empty".into()));
    }
    if !matches!(input.target_kind.as_str(), "workflow" | "airway") {
        return Err(ScheduleError::Invalid(format!(
            "target_kind must be 'workflow' or 'airway', got {:?}",
            input.target_kind
        )));
    }
    if input.target_ref.trim().is_empty() {
        return Err(ScheduleError::Invalid(
            "target_ref must not be empty".into(),
        ));
    }
    validate_cron(&input.cron_expr, &input.timezone).map_err(ScheduleError::Invalid)
}

// ── CRUD (workspace-scoped) ─────────────────────────────────────────────────
//
// All CRUD operations are scoped by `workspace_id` (§12 FU4b): list filters,
// get/update/delete return `NotFound` on cross-workspace access, create
// stamps the row. The handler supplies `workspace_id` from the
// `/{workspace_id}/...` path; clients never set it via the body.

pub async fn list_schedules(
    db: &DatabaseConnection,
    workspace_id: uuid::Uuid,
) -> Result<Vec<schedule::Model>, ScheduleError> {
    Ok(schedule::Entity::find()
        .filter(schedule::Column::WorkspaceId.eq(workspace_id))
        .all(db)
        .await?)
}

pub async fn get_schedule(
    db: &DatabaseConnection,
    workspace_id: uuid::Uuid,
    id: &str,
) -> Result<schedule::Model, ScheduleError> {
    schedule::Entity::find_by_id(id.to_string())
        .filter(schedule::Column::WorkspaceId.eq(workspace_id))
        .one(db)
        .await?
        .ok_or(ScheduleError::NotFound)
}

pub async fn create_schedule(
    db: &DatabaseConnection,
    workspace_id: uuid::Uuid,
    input: ScheduleInput,
) -> Result<schedule::Model, ScheduleError> {
    validate_input(&input)?;
    let now = agentic_runtime::crud::now();
    let next = next_occurrence_after(&input.cron_expr, &input.timezone, chrono::Utc::now())
        .map_err(ScheduleError::Invalid)?
        .fixed_offset();
    let model = schedule::ActiveModel {
        id: Set(uuid::Uuid::new_v4().to_string()),
        workspace_id: Set(workspace_id),
        project_id: Set(None),
        branch_id: Set(None),
        name: Set(input.name),
        target_kind: Set(input.target_kind),
        target_ref: Set(input.target_ref),
        variables: Set(input.variables),
        cron_expr: Set(input.cron_expr),
        timezone: Set(input.timezone),
        enabled: Set(input.enabled),
        next_run_at: Set(next),
        last_fired_at: Set(None),
        last_run_id: Set(None),
        last_error: Set(None),
        missed_runs: Set(0),
        last_missed_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    };
    Ok(schedule::Entity::insert(model)
        .exec_with_returning(db)
        .await?)
}

pub async fn update_schedule(
    db: &DatabaseConnection,
    workspace_id: uuid::Uuid,
    id: &str,
    input: ScheduleInput,
) -> Result<schedule::Model, ScheduleError> {
    validate_input(&input)?;
    // Cross-workspace updates surface as NotFound (not Forbidden), so the
    // existence of a schedule in another workspace is not probeable.
    let existing = get_schedule(db, workspace_id, id).await?;
    let cadence_changed =
        existing.cron_expr != input.cron_expr || existing.timezone != input.timezone;
    let next = if cadence_changed {
        next_occurrence_after(&input.cron_expr, &input.timezone, chrono::Utc::now())
            .map_err(ScheduleError::Invalid)?
            .fixed_offset()
    } else {
        existing.next_run_at
    };
    let model = schedule::ActiveModel {
        id: Set(existing.id),
        workspace_id: Set(existing.workspace_id),
        project_id: Set(existing.project_id),
        branch_id: Set(existing.branch_id),
        name: Set(input.name),
        target_kind: Set(input.target_kind),
        target_ref: Set(input.target_ref),
        variables: Set(input.variables),
        cron_expr: Set(input.cron_expr),
        timezone: Set(input.timezone),
        enabled: Set(input.enabled),
        next_run_at: Set(next),
        last_fired_at: Set(existing.last_fired_at),
        last_run_id: Set(existing.last_run_id),
        last_error: Set(existing.last_error),
        // Carry the audit fields across — an edit isn't a reset, and
        // dropping them would erase visible "you missed N runs" state.
        missed_runs: Set(existing.missed_runs),
        last_missed_at: Set(existing.last_missed_at),
        created_at: Set(existing.created_at),
        updated_at: Set(agentic_runtime::crud::now()),
    };
    Ok(schedule::Entity::update(model).exec(db).await?)
}

pub async fn delete_schedule(
    db: &DatabaseConnection,
    workspace_id: uuid::Uuid,
    id: &str,
) -> Result<(), ScheduleError> {
    let res = schedule::Entity::delete_many()
        .filter(schedule::Column::Id.eq(id.to_string()))
        .filter(schedule::Column::WorkspaceId.eq(workspace_id))
        .exec(db)
        .await?;
    if res.rows_affected == 0 {
        return Err(ScheduleError::NotFound);
    }
    Ok(())
}

/// Fire a schedule out-of-band, now, without touching `next_run_at` (the
/// recurring cadence is unaffected). Returns the seeded run id.
pub async fn run_schedule_now(
    db: &DatabaseConnection,
    workspace_id: uuid::Uuid,
    workspace: &dyn crate::WorkflowWorkspaceContext,
    id: &str,
) -> Result<String, ScheduleError> {
    let s = get_schedule(db, workspace_id, id).await?;
    match fire_schedule(db, workspace, &s).await {
        Ok(run_id) => {
            record_fire_success(db, &s.id, &run_id).await;
            Ok(run_id)
        }
        Err(e) => {
            set_last_error(db, &s.id, Some(&e)).await;
            Err(ScheduleError::Invalid(e))
        }
    }
}

// ── Tick ────────────────────────────────────────────────────────────────────

/// Run one scheduler pass for a single workspace. Returns the number of
/// schedules fired by *this* replica. Never errors out the caller —
/// per-schedule failures are logged and skipped so one bad row can't stall
/// the loop.
///
/// Scoped to `workspace_id` (§12 FU4b): airway targets resolve their
/// pipeline file on the supplied workspace's filesystem, so each
/// workspace's tick must be invoked with its own matching context.
pub async fn tick_schedules(
    db: &DatabaseConnection,
    workspace_id: uuid::Uuid,
    workspace: &dyn crate::WorkflowWorkspaceContext,
) -> usize {
    let now = chrono::Utc::now().fixed_offset();
    let due = match schedule::Entity::find()
        .filter(schedule::Column::WorkspaceId.eq(workspace_id))
        .filter(schedule::Column::Enabled.eq(true))
        .filter(schedule::Column::NextRunAt.lte(now))
        .all(db)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(target: "scheduler", error = %e, "tick: query due schedules failed");
            return 0;
        }
    };

    let mut fired = 0;
    for s in due {
        // Strictly-after-now → run-once-then-resume misfire policy.
        let next = match next_occurrence_after(&s.cron_expr, &s.timezone, chrono::Utc::now()) {
            Ok(n) => n.fixed_offset(),
            Err(e) => {
                tracing::warn!(
                    target: "scheduler",
                    schedule_id = %s.id,
                    error = %e,
                    "tick: bad cron/timezone; skipping (fix via CRUD)"
                );
                set_last_error(db, &s.id, Some(&e)).await;
                continue;
            }
        };

        // Count occurrences between the slot we're about to fire and
        // `now`. Anything in that open range was a slot the tick
        // skipped — the run-once-then-resume policy fires only the
        // due-since slot itself, so each occurrence past `s.next_run_at`
        // is a miss we want to surface (without changing the policy).
        // Capped at 1000 so a misconfigured five-second cron with a
        // multi-year gap doesn't lock the tick.
        let prev_due_utc = s.next_run_at.with_timezone(&chrono::Utc);
        let missed = count_occurrences_between(
            &s.cron_expr,
            &s.timezone,
            prev_due_utc,
            chrono::Utc::now(),
            1000,
        )
        .unwrap_or(0);

        // CAS: advance only if next_run_at still equals what we read. The
        // loser (another replica / concurrent tick) affects 0 rows and
        // skips — exactly-once fire. `missed_runs` and `last_missed_at`
        // are stamped in the same UPDATE so a single replica owns the
        // catch-up accounting too.
        let won = match db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "UPDATE agentic_schedules \
                 SET next_run_at = $1, \
                     last_fired_at = now(), \
                     missed_runs = missed_runs + $4, \
                     last_missed_at = CASE WHEN $4 > 0 THEN now() ELSE last_missed_at END, \
                     updated_at = now() \
                 WHERE id = $2 AND next_run_at = $3",
                [
                    next.into(),
                    s.id.clone().into(),
                    s.next_run_at.into(),
                    (missed as i32).into(),
                ],
            ))
            .await
        {
            Ok(r) => r.rows_affected() == 1,
            Err(e) => {
                tracing::error!(target: "scheduler", schedule_id = %s.id, error = %e, "tick: CAS failed");
                continue;
            }
        };
        if !won {
            continue;
        }
        if missed > 0 {
            // Warn only — policy is still "fire one, resume". The count
            // tells the user (via the UI) how many slots were skipped.
            tracing::warn!(
                target: "scheduler",
                schedule_id = %s.id,
                missed,
                schedule_name = %s.name,
                "tick: catch-up fire skipped {} occurrences (policy: run-once-then-resume)",
                missed,
            );
        }

        match fire_schedule(db, workspace, &s).await {
            Ok(rid) => {
                fired += 1;
                tracing::info!(
                    target: "scheduler",
                    schedule_id = %s.id,
                    run_id = %rid,
                    target_kind = %s.target_kind,
                    "tick: fired schedule (Global run seeded)"
                );
                record_fire_success(db, &s.id, &rid).await;
            }
            Err(e) => {
                // next_run_at was already advanced — the seed failed
                // (e.g. bad ref). The schedule keeps its cadence rather
                // than tight-looping on a bad row.
                tracing::error!(
                    target: "scheduler",
                    schedule_id = %s.id,
                    error = %e,
                    "tick: seed failed; schedule advanced, will retry next slot"
                );
                set_last_error(db, &s.id, Some(&e)).await;
            }
        }
    }

    fired
}

// ── Shared firing ───────────────────────────────────────────────────────────

/// Seed a `TaskScope::Global` run for `s`. Shared by the tick and run-now.
async fn fire_schedule(
    db: &DatabaseConnection,
    workspace: &dyn crate::WorkflowWorkspaceContext,
    s: &schedule::Model,
) -> Result<String, String> {
    match s.target_kind.as_str() {
        "workflow" => {
            let req = StartWorkflowRequest {
                workflow_ref: s.target_ref.clone(),
                variables: s.variables.clone(),
                retry_from_run_id: None,
                cache_enabled: false,
                invalidate_steps: None,
                invalidate_iterations: None,
                thread_id: None,
            };
            start_workflow_run(
                db,
                req,
                agentic_runtime::crud::TaskScope::Global,
                s.workspace_id,
            )
            .await
            .map_err(|e| e.to_string())
        }
        "airway" => {
            let req = StartAirwayRequest {
                pipeline_ref: s.target_ref.clone(),
                variables: s.variables.clone(),
                thread_id: None,
            };
            start_airway_run(
                db,
                workspace,
                req,
                agentic_runtime::crud::TaskScope::Global,
                s.workspace_id,
            )
            .await
            .map_err(|e| e.to_string())
        }
        other => Err(format!("unknown target_kind {other:?}")),
    }
}

/// Record a successful fire: link the run and clear any prior error, in
/// one UPDATE. Best-effort — the run is already seeded, so a failure here
/// doesn't lose work.
async fn record_fire_success(db: &DatabaseConnection, schedule_id: &str, run_id: &str) {
    if let Err(e) = db
        .execute(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "UPDATE agentic_schedules SET last_run_id = $1, last_error = NULL WHERE id = $2",
            [run_id.into(), schedule_id.into()],
        ))
        .await
    {
        tracing::warn!(target: "scheduler", %schedule_id, error = %e, "record fire success failed");
    }
}

/// Record the most recent fire/seed/cron failure for UI surfacing.
/// Best-effort observability — never blocks the loop.
async fn set_last_error(db: &DatabaseConnection, schedule_id: &str, msg: Option<&str>) {
    let msg: Option<String> = msg.map(str::to_string);
    if let Err(e) = db
        .execute(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "UPDATE agentic_schedules SET last_error = $1 WHERE id = $2",
            [msg.into(), schedule_id.into()],
        ))
        .await
    {
        tracing::warn!(target: "scheduler", %schedule_id, error = %e, "set last_error failed");
    }
}
