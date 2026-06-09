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

use std::sync::Arc;

use agentic_runtime::cron::{
    count_occurrences_between, next_occurrence_after, occurrences_between, validate_cron,
};
use agentic_runtime::entity::schedule;
use agentic_runtime::lifecycle::crud::runs::{
    insert_run_with_schedule, update_run_done, update_run_failed,
};
use sea_orm::ActiveValue::Set;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, Statement,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::agent_run::{StartAgentRequest, start_agent_run};
use crate::airway_run::{StartAirwayRequest, start_airway_run};
use crate::platform::PlatformContext;
use crate::workflow_run::{StartWorkflowRequest, start_workflow_run};

/// Create/update payload. Deserialized straight from the HTTP body.
#[derive(Debug, Clone, Deserialize)]
pub struct ScheduleInput {
    pub name: String,
    /// `"workflow"` | `"airway"` | `"agent"`.
    pub target_kind: String,
    /// `workflow_ref` / `pipeline_ref` / `agent_id`, workspace-relative.
    pub target_ref: String,
    /// Free-text question — required when `target_kind = "agent"`,
    /// ignored otherwise. Stored on `agentic_schedules.question`.
    #[serde(default)]
    pub question: Option<String>,
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
    if !matches!(
        input.target_kind.as_str(),
        "workflow" | "airway" | "agent" | "monitor_scan"
    ) {
        return Err(ScheduleError::Invalid(format!(
            "target_kind must be 'workflow', 'airway', 'agent', or 'monitor_scan', got {:?}",
            input.target_kind
        )));
    }
    if input.target_ref.trim().is_empty() {
        return Err(ScheduleError::Invalid(
            "target_ref must not be empty".into(),
        ));
    }
    if input.target_kind == "agent"
        && input
            .question
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    {
        return Err(ScheduleError::Invalid(
            "question must not be empty for agent schedules".into(),
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
    // Only persist a question for agent schedules; workflow / airway
    // rows ignore it. Trim before storing so accidental whitespace
    // doesn't slip through.
    let question = if input.target_kind == "agent" {
        input.question.as_deref().map(|q| q.trim().to_string())
    } else {
        None
    };
    let model = schedule::ActiveModel {
        id: Set(uuid::Uuid::new_v4().to_string()),
        workspace_id: Set(workspace_id),
        project_id: Set(None),
        branch_id: Set(None),
        name: Set(input.name),
        target_kind: Set(input.target_kind),
        target_ref: Set(input.target_ref),
        question: Set(question),
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
    let question = if input.target_kind == "agent" {
        input.question.as_deref().map(|q| q.trim().to_string())
    } else {
        None
    };
    let model = schedule::ActiveModel {
        id: Set(existing.id),
        workspace_id: Set(existing.workspace_id),
        project_id: Set(existing.project_id),
        branch_id: Set(existing.branch_id),
        name: Set(input.name),
        target_kind: Set(input.target_kind),
        target_ref: Set(input.target_ref),
        question: Set(question),
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
    // `manual` distinguishes operator-fired runs from `scheduled` (the tick)
    // and `backfill` (out-of-band replay) in the run log.
    match fire_schedule(db, workspace, &s, "manual").await {
        Ok(run_id) => {
            record_fire_success(db, &s.id, &run_id).await;
            Ok(run_id)
        }
        Err(e) => {
            set_schedule_last_error(db, &s.id, Some(&e)).await;
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
                set_schedule_last_error(db, &s.id, Some(&e)).await;
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

        match fire_schedule(db, workspace, &s, "scheduled").await {
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
                set_schedule_last_error(db, &s.id, Some(&e)).await;
            }
        }
    }

    fired
}

/// Run one scheduler pass for `monitor_scan` schedules in the given workspace.
/// Creates an `agentic_runs` row per due schedule and spawns the scan in a
/// background task so the tick loop is not blocked. The run is visible in
/// the coordinator immediately as "running".
/// Returns the number of schedules fired. Never errors the caller —
/// per-schedule failures are logged and skipped.
pub async fn tick_monitor_schedules(
    db: &DatabaseConnection,
    workspace_id: uuid::Uuid,
    platform: Arc<dyn PlatformContext>,
) -> usize {
    let now = chrono::Utc::now().fixed_offset();
    let due = match schedule::Entity::find()
        .filter(schedule::Column::WorkspaceId.eq(workspace_id))
        .filter(schedule::Column::TargetKind.eq("monitor_scan"))
        .filter(schedule::Column::Enabled.eq(true))
        .filter(schedule::Column::NextRunAt.lte(now))
        .all(db)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(target: "scheduler", error = %e, "monitor tick: query failed");
            return 0;
        }
    };

    let mut fired = 0;
    for s in due {
        // Validate granularity before touching next_run_at — a misconfigured
        // row is a data error, not a transient failure, so we don't advance.
        let granularity = match s
            .variables
            .as_ref()
            .and_then(|v| v.get("granularity"))
            .and_then(|g| g.as_str())
        {
            Some(g) => g.to_string(),
            None => {
                set_schedule_last_error(db, &s.id, Some("missing granularity in variables")).await;
                continue;
            }
        };

        let next = match next_occurrence_after(&s.cron_expr, &s.timezone, chrono::Utc::now()) {
            Ok(n) => n.fixed_offset(),
            Err(e) => {
                tracing::warn!(
                    target: "scheduler",
                    schedule_id = %s.id,
                    error = %e,
                    "monitor tick: bad cron/timezone; skipping"
                );
                set_schedule_last_error(db, &s.id, Some(&e)).await;
                continue;
            }
        };

        let prev_due_utc = s.next_run_at.with_timezone(&chrono::Utc);
        let missed = count_occurrences_between(
            &s.cron_expr,
            &s.timezone,
            prev_due_utc,
            chrono::Utc::now(),
            1000,
        )
        .unwrap_or(0);

        // CAS-advance: exactly-once fire across replicas.
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
                tracing::error!(target: "scheduler", schedule_id = %s.id, error = %e, "monitor tick: CAS failed");
                continue;
            }
        };
        if !won {
            continue;
        }
        if missed > 0 {
            tracing::warn!(
                target: "scheduler",
                schedule_id = %s.id,
                missed,
                "monitor tick: catch-up fire skipped {} occurrences (policy: run-once-then-resume)",
                missed,
            );
        }

        let run_id = Uuid::new_v4().to_string();
        let mut meta = serde_json::json!({ "granularity": granularity });
        stamp_trigger_metadata(&mut meta, &Some("scheduled".into()), &None, &None);
        if let Err(e) = insert_run_with_schedule(
            db,
            &run_id,
            &format!("Anomaly scan ({granularity})"),
            None,
            "monitor_scan",
            Some(meta),
            &s.id,
            workspace_id,
        )
        .await
        {
            tracing::error!(target: "scheduler", schedule_id = %s.id, error = %e, "monitor tick: failed to create run row; skipping enqueue");
            continue;
        }

        fired += 1;
        record_fire_success(db, &s.id, &run_id).await;

        // Spawn the scan as a background task so the tick loop advances
        // immediately. The run is already visible as "running" in the
        // coordinator; status is updated to done/failed when the scan finishes.
        let db_scan = db.clone();
        let platform_scan = platform.clone();
        let schedule_id_scan = s.id.clone();
        let run_id_scan = run_id.clone();
        let granularity_scan = granularity.clone();
        tokio::spawn(async move {
            let Some(port) = platform_scan.as_monitor_scan_port() else {
                tracing::error!(target: "monitor_scan", run_id = %run_id_scan, "no MonitorScanPort available");
                let _ = update_run_failed(
                    &db_scan,
                    &run_id_scan,
                    "monitor scan not available in this deployment",
                )
                .await;
                return;
            };
            match port
                .run_monitor_scan(&db_scan, workspace_id, &granularity_scan)
                .await
            {
                Ok(summary) => {
                    tracing::info!(target: "monitor_scan", run_id = %run_id_scan, %summary, "scan complete");
                    let _ = update_run_done(&db_scan, &run_id_scan, &summary, None).await;
                }
                Err(e) => {
                    tracing::warn!(target: "monitor_scan", run_id = %run_id_scan, error = %e, "scan failed");
                    let _ = update_run_failed(&db_scan, &run_id_scan, &e).await;
                    set_schedule_last_error(&db_scan, &schedule_id_scan, Some(&e)).await;
                }
            }
        });
        tracing::info!(target: "monitor_scan", schedule_id = %s.id, run_id = %run_id, %granularity, "scan spawned");
    }

    fired
}

// ── Shared firing ───────────────────────────────────────────────────────────

/// Seed a `TaskScope::Global` run for `s`. Shared by the tick and run-now;
/// caller picks the trigger label so the run log can distinguish
/// `scheduled` / `manual` / `backfill` at a glance.
async fn fire_schedule(
    db: &DatabaseConnection,
    workspace: &dyn crate::WorkflowWorkspaceContext,
    s: &schedule::Model,
    trigger: &str,
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
                schedule_id: Some(s.id.clone()),
                trigger: Some(trigger.to_string()),
                logical_date: None,
                retry_of: None,
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
                resources: Vec::new(),
                schedule_id: Some(s.id.clone()),
                trigger: Some(trigger.to_string()),
                logical_date: None,
                retry_of: None,
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
        "agent" => {
            // validate_input guarantees `question` is present + non-empty
            // for agent schedules, but an old row could legally have NULL
            // — surface a clear error rather than panicking.
            let question = s.question.clone().ok_or_else(|| {
                "agent schedule has no question stored — re-save the schedule".to_string()
            })?;
            let req = StartAgentRequest {
                agent_id: s.target_ref.clone(),
                question,
                thread_id: None,
                schedule_id: Some(s.id.clone()),
                trigger: Some(trigger.to_string()),
                logical_date: None,
                retry_of: None,
            };
            start_agent_run(
                db,
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
pub async fn record_fire_success(db: &DatabaseConnection, schedule_id: &str, run_id: &str) {
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

/// Merge run-provenance fields (trigger source, logical date, retry-of)
/// into a run's `metadata` JSONB.
///
/// Called from both seed paths so the same convention applies whether the
/// run was scheduled, manually triggered, backfilled, or retried.
/// `metadata` is mutated in place — the surrounding seed builds the rest
/// of the JSON object first; this just stamps the well-known keys when
/// each field is set.
pub fn stamp_trigger_metadata(
    metadata: &mut serde_json::Value,
    trigger: &Option<String>,
    logical_date: &Option<chrono::DateTime<chrono::Utc>>,
    retry_of: &Option<String>,
) {
    let serde_json::Value::Object(map) = metadata else {
        return;
    };
    if let Some(t) = trigger {
        map.insert("trigger".to_string(), serde_json::Value::String(t.clone()));
    }
    if let Some(ld) = logical_date {
        map.insert(
            "logical_date".to_string(),
            serde_json::Value::String(ld.to_rfc3339()),
        );
    }
    if let Some(r) = retry_of {
        map.insert("retry_of".to_string(), serde_json::Value::String(r.clone()));
    }
}

// ── Backfill ────────────────────────────────────────────────────────────────
//
// Fill in runs for cron slots the operator wants to replay — typically
// the missing-slot gaps the dashboard surfaces. Each seeded run carries:
//
//   * `schedule_id` — first-class column linking it to the originating job,
//   * `metadata.trigger = "backfill"` — distinguishes it from `scheduled` /
//     `manual` runs in the run log,
//   * `metadata.logical_date` — the cron-scheduled time being replayed,
//     so date-aware downstream logic can use the intended fire time
//     rather than `now()`.

/// Inputs for [`backfill_schedule`]. Body of the backfill HTTP endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct BackfillRequest {
    /// Inclusive lower bound of the range to backfill (UTC).
    pub from: chrono::DateTime<chrono::Utc>,
    /// Inclusive upper bound; must be strictly greater than `from`.
    pub to: chrono::DateTime<chrono::Utc>,
    /// Execution-side throttle hint: `"sequential"` | `"<N>"` | `"all"`.
    /// Currently advisory — recorded on each run's metadata for a future
    /// executor that honors per-schedule throttling. Seeding itself is
    /// sequential.
    #[serde(default)]
    pub concurrency: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BackfillResult {
    /// The runs that were successfully seeded into the queue.
    pub run_ids: Vec<String>,
    /// Total cron occurrences enumerated in the requested window. May
    /// exceed `run_ids.len()` if some seed calls failed.
    pub planned: usize,
}

/// Maximum number of slots a single backfill request may seed. Bounds the
/// blast radius — if an operator requested a year of a 1-minute cron we'd
/// otherwise enqueue 525,600 runs in one call.
const BACKFILL_MAX_OCCURRENCES: usize = 500;

/// Seed one run per cron occurrence in `[from, to]`, tagged as backfill.
/// Returns the seeded `run_id`s. Sequential v1 — see [`BackfillRequest`].
pub async fn backfill_schedule(
    db: &DatabaseConnection,
    workspace_id: uuid::Uuid,
    workspace: &dyn crate::WorkflowWorkspaceContext,
    schedule_id: &str,
    request: BackfillRequest,
) -> Result<BackfillResult, ScheduleError> {
    let s = get_schedule(db, workspace_id, schedule_id).await?;
    if request.to <= request.from {
        return Err(ScheduleError::Invalid(
            "backfill `to` must be after `from`".into(),
        ));
    }

    // The cron evaluator's range is half-open (after, until], so step
    // back one second on the lower bound so the very first occurrence at
    // `from` is included — operators expect the inclusive range they typed
    // in the dialog.
    let after = request.from - chrono::Duration::seconds(1);
    let occurrences = occurrences_between(
        &s.cron_expr,
        &s.timezone,
        after,
        request.to,
        BACKFILL_MAX_OCCURRENCES,
    )
    .map_err(ScheduleError::Invalid)?;

    let planned = occurrences.len();
    if planned == 0 {
        return Ok(BackfillResult {
            run_ids: Vec::new(),
            planned: 0,
        });
    }

    let mut run_ids = Vec::with_capacity(planned);
    for occurrence in occurrences {
        let run_id =
            match seed_backfill_occurrence(db, workspace, &s, occurrence, &request.concurrency)
                .await
            {
                Ok(id) => id,
                Err(e) => {
                    tracing::warn!(
                        target: "scheduler",
                        schedule_id = %s.id,
                        logical_date = %occurrence,
                        error = %e,
                        "backfill: seed failed; stopping at first error",
                    );
                    if run_ids.is_empty() {
                        return Err(ScheduleError::Invalid(e));
                    }
                    // Partial success: return what we have so the operator
                    // can act on it rather than losing the seeded runs.
                    return Ok(BackfillResult { run_ids, planned });
                }
            };
        run_ids.push(run_id);
    }

    Ok(BackfillResult { run_ids, planned })
}

async fn seed_backfill_occurrence(
    db: &DatabaseConnection,
    workspace: &dyn crate::WorkflowWorkspaceContext,
    s: &schedule::Model,
    occurrence: chrono::DateTime<chrono::Utc>,
    _concurrency: &Option<String>,
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
                schedule_id: Some(s.id.clone()),
                trigger: Some("backfill".to_string()),
                logical_date: Some(occurrence),
                retry_of: None,
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
                resources: Vec::new(),
                schedule_id: Some(s.id.clone()),
                trigger: Some("backfill".to_string()),
                logical_date: Some(occurrence),
                retry_of: None,
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
        "agent" => {
            let question = s.question.clone().ok_or_else(|| {
                "agent schedule has no question stored — re-save the schedule".to_string()
            })?;
            let req = StartAgentRequest {
                agent_id: s.target_ref.clone(),
                question,
                thread_id: None,
                schedule_id: Some(s.id.clone()),
                trigger: Some("backfill".to_string()),
                logical_date: Some(occurrence),
                retry_of: None,
            };
            start_agent_run(
                db,
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

/// Record the most recent fire/seed/cron failure for UI surfacing.
/// Best-effort observability — never blocks the loop.
pub async fn set_schedule_last_error(
    db: &DatabaseConnection,
    schedule_id: &str,
    msg: Option<&str>,
) {
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
