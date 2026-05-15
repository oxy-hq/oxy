use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "agentic_workflow_state")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub run_id: String,
    pub workflow_yaml_hash: String,
    pub workflow_config: Json,
    pub workflow_context: Json,
    pub variables: Option<Json>,
    pub trace_id: String,
    pub current_step: i32,
    pub results: Json,
    /// Vestigial: always persisted as `{}` by `apply_result_delta_in_txn` and
    /// reconstructed from `results` at load time (see `rebuild_render_context`
    /// in `extension/mod.rs`). Kept on the schema for backward compatibility
    /// with existing rows; do not write to it.
    pub render_context: Json,
    pub pending_children: Json,
    pub decision_version: i64,
    pub updated_at: ChronoDateTimeUtc,
    /// Per-step content hashes (`step_name → SHA-256 hex`) used to decide
    /// whether a retry can reuse a prior run's output for that step.
    /// Populated incrementally as steps complete, via the same JSONB-merge
    /// pattern as `results`.
    pub step_hashes: Json,
    /// Prior run to compare against on a "resume unchanged steps" retry.
    /// `None` for fresh runs — the decider then treats every step as dirty.
    pub retry_from_run_id: Option<String>,
    /// Opt-in flag for cache-driven step skipping. When `false` the decider
    /// behaves exactly as before regardless of `retry_from_run_id`.
    pub cache_enabled: bool,
    /// Pre-materialised snapshot of the prior run's `step_hashes`, with any
    /// entries named in the launch-time `invalidate_steps` already stripped.
    /// Populated once at seed (see `executor::workflow::execute_workflow`);
    /// the decider reads this directly instead of re-loading the prior row.
    pub prior_step_hashes: Json,
    /// Pre-materialised snapshot of the prior run's `results`, mirroring
    /// `prior_step_hashes`. Both maps are kept in lockstep so the decider's
    /// cache-hit check sees a consistent pair.
    pub prior_results: Json,
    /// Seed-time render context for synthetic sub-workflow runs (the
    /// product of fanning out a `loop_sequential` step). Carries the
    /// iteration variable (`{step_name}.value` / `.index`) plus the
    /// parent run's accumulated context so inner template references
    /// resolve correctly. Written once at insert; the load path merges
    /// it into `render_context` after `rebuild_render_context`.
    pub initial_render_context: Json,
    /// Per-step iteration indices to force-replay on this retry,
    /// ignoring per-iteration cache entries. Shape:
    /// `{step_name: [indices]}`. Empty for fresh runs and for retries
    /// that don't pass the field. Stamped at seed; the decider's loop
    /// branch reads it inline without re-querying `agentic_runs`.
    pub invalidate_iterations: Json,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
