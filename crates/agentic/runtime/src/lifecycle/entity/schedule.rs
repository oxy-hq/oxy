//! Cron schedule row (Phase 2). A user-defined recurring trigger that the
//! scheduler tick fires by seeding a `TaskScope::Global` run.

use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize)]
#[sea_orm(table_name = "agentic_schedules")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// Owning workspace (multi-tenant scope). Plain Uuid, no FK per the
    /// agentic cross-domain loose-references rule; app-level cleanup on
    /// workspace delete. New rows always set this via the CRUD handlers;
    /// any pre-existing rows backfill to the nil UUID and are inert.
    pub workspace_id: Uuid,
    /// Legacy/optional project + branch identifiers (unused; superseded
    /// by `workspace_id` for scoping). Kept for back-compat / future
    /// per-branch scoping work.
    pub project_id: Option<String>,
    pub branch_id: Option<String>,
    /// User-facing label.
    pub name: String,
    /// `"workflow"` | `"airway"` | `"agent"`.
    pub target_kind: String,
    /// `workflow_ref` / `pipeline_ref` / `agent_id`, workspace-relative.
    pub target_ref: String,
    /// Free-text question for agent schedules. Required when
    /// `target_kind = "agent"`; ignored for workflow / airway. Stored as
    /// `Option<String>` so existing rows (and the two other target kinds)
    /// leave it NULL.
    pub question: Option<String>,
    /// Variables passed to the seed fn (JSON object).
    pub variables: Option<serde_json::Value>,
    /// Standard cron expression.
    pub cron_expr: String,
    /// IANA timezone the cron expression is evaluated in.
    pub timezone: String,
    pub enabled: bool,
    /// The CAS coordination point: the next due time. Advanced atomically
    /// on fire so exactly one replica's tick wins.
    pub next_run_at: DateTimeWithTimeZone,
    /// Observability: when the schedule last fired and the run it seeded.
    pub last_fired_at: Option<DateTimeWithTimeZone>,
    pub last_run_id: Option<String>,
    /// Most recent fire/seed failure (bad cron, missing target, seed
    /// error). `NULL` once a fire succeeds.
    pub last_error: Option<String>,
    /// Cumulative count of cron occurrences that fell between
    /// successive `next_run_at` advances — i.e., the number of slots
    /// silently skipped while the server was down. Incremented by the
    /// scheduler tick at catch-up time; never decremented (purely
    /// audit). 0 in steady state when every tick interval runs.
    pub missed_runs: i32,
    /// Timestamp of the most recent tick that detected a catch-up.
    /// `NULL` at row creation; stamped only when [`missed_runs`] increments.
    pub last_missed_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
