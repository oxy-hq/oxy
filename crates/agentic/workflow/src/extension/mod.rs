//! Workflow state extension table: per-run Temporal-style workflow state.
//!
//! The `entity` and `crud` submodules are crate-private — external consumers
//! use the [`WorkflowRunState`] DTO and the facade functions below.

pub(crate) mod commit;
pub(crate) mod crud;
pub(crate) mod entity;
pub mod migration;

pub use commit::{CommitOutcome, DecisionCommit, DecisionTerminal, commit_decision};
pub use migration::WorkflowMigrator;

use std::collections::HashMap;

use sea_orm::{DatabaseConnection, DbErr};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::WorkflowConfig;

// ── Public DTO ─────────────────────────────────────────────────────────────

/// Durable state for a workflow run.
///
/// Persisted in `agentic_workflow_state`. A `WorkflowDecision` task loads
/// this, calls `WorkflowDecider::decide()`, updates state, and exits — no
/// in-memory channels survive a crash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRunState {
    pub run_id: String,
    pub workflow: WorkflowConfig,
    pub workflow_yaml_hash: String,
    pub workflow_context: Value,
    pub variables: Option<Value>,
    pub trace_id: String,
    pub current_step: usize,
    /// Step name → serialized OutputContainer result.
    pub results: HashMap<String, Value>,
    /// Accumulated minijinja render context from prior steps. The DB column
    /// is always `{}` and reconstructed from `results` at load time (see
    /// `rebuild_render_context`); only the in-memory value is read by the
    /// decider.
    pub render_context: Value,
    /// step_index (as string) → list of child task_ids still in flight.
    pub pending_children: HashMap<String, Vec<String>>,
    /// Monotonic counter for optimistic concurrency; incremented on every update.
    pub decision_version: i64,
    /// Per-step content hashes (`step_name → SHA-256 hex`).
    ///
    /// Populated as each step completes — the same JSONB-merge pattern as
    /// `results`. On retry with `retry_from_run_id` set, the decider compares
    /// the current run's computed step hash against the prior run's
    /// `step_hashes[name]`; on match, the prior `results[name]` is copied
    /// through and the step is skipped.
    pub step_hashes: HashMap<String, String>,
    /// Prior run id to compare step hashes against. `None` for fresh runs.
    pub retry_from_run_id: Option<String>,
    /// Opt-in: when `true` (and `retry_from_run_id` is set), the decider
    /// reuses prior step results whose hashes match. Off by default so the
    /// system behaves exactly as before unless a caller explicitly opts in.
    pub cache_enabled: bool,
    /// Snapshot of the prior run's per-step hashes at seed time, already
    /// minus any steps named in the launch-time `invalidate_steps` hint.
    /// Empty unless `cache_enabled && retry_from_run_id.is_some()` at seed.
    ///
    /// The decider compares the current step's hash against
    /// `prior_step_hashes[name]` for a cache-hit decision — exactly what it
    /// used to read off the prior run's freshly-loaded row, but now resolved
    /// inline so each decision pass is a single state load instead of two.
    #[serde(default)]
    pub prior_step_hashes: HashMap<String, String>,
    /// Snapshot of the prior run's results, paired with `prior_step_hashes`.
    /// Same population rules and same lifecycle.
    #[serde(default)]
    pub prior_results: HashMap<String, Value>,
    /// Seed-time render context for synthetic sub-workflows (loop
    /// iteration bodies). Populated by `execute_workflow` when a
    /// `TaskSpec::Workflow { initial_render_context: Some(_), … }`
    /// kicks off a fresh run; merged into `render_context` on every
    /// load so inner template references see the iteration variable
    /// (`{step_name}.value` / `.index`) and the parent's accumulated
    /// context. Empty for top-level workflow runs.
    #[serde(default)]
    pub initial_render_context: Value,
    /// `{step_name: [indices]}` map of loop iterations the caller
    /// asked to force-replay on this retry, even when per-iteration
    /// cache lookups would otherwise reuse the prior outcome. Empty
    /// for fresh runs and for retries that don't pass the field.
    /// Read inline by the decider's loop branch.
    #[serde(default)]
    pub invalidate_iterations: HashMap<String, Vec<usize>>,
}

impl TryFrom<entity::Model> for WorkflowRunState {
    type Error = DbErr;

    fn try_from(m: entity::Model) -> Result<Self, DbErr> {
        let workflow: WorkflowConfig =
            serde_json::from_value(m.workflow_config).map_err(|e| DbErr::Custom(e.to_string()))?;
        let results: HashMap<String, Value> =
            serde_json::from_value(m.results).map_err(|e| DbErr::Custom(e.to_string()))?;
        let pending_children: HashMap<String, Vec<String>> =
            serde_json::from_value(m.pending_children).map_err(|e| DbErr::Custom(e.to_string()))?;
        let step_hashes: HashMap<String, String> =
            serde_json::from_value(m.step_hashes).map_err(|e| DbErr::Custom(e.to_string()))?;
        let prior_step_hashes: HashMap<String, String> =
            serde_json::from_value(m.prior_step_hashes)
                .map_err(|e| DbErr::Custom(e.to_string()))?;
        let prior_results: HashMap<String, Value> =
            serde_json::from_value(m.prior_results).map_err(|e| DbErr::Custom(e.to_string()))?;

        // Reconstruct render_context from step results rather than reading the
        // DB column.  The `render_context` column is always persisted as `{}`
        // by commit_decision (see apply_result_delta_in_txn), making it a
        // vestigial field.  Deriving it here keeps the hot-path writes O(1 new
        // result) while still giving the decider a correct context on every load.
        //
        // For loop-iteration sub-workflows we additionally merge
        // `initial_render_context` underneath the rebuilt one so the iteration
        // variable (`{step_name}.value`/`.index`) and the parent's accumulated
        // context are visible to inner templates. Rebuilt-results win on key
        // conflict — a key the user named both as a parent step result *and*
        // as an inner step result has the inner step's value override.
        let render_context =
            merge_render_contexts(&m.initial_render_context, rebuild_render_context(&results));

        Ok(Self {
            run_id: m.run_id,
            workflow,
            workflow_yaml_hash: m.workflow_yaml_hash,
            workflow_context: m.workflow_context,
            variables: m.variables,
            trace_id: m.trace_id,
            current_step: m.current_step as usize,
            results,
            render_context,
            pending_children,
            decision_version: m.decision_version,
            step_hashes,
            retry_from_run_id: m.retry_from_run_id,
            cache_enabled: m.cache_enabled,
            prior_step_hashes,
            prior_results,
            initial_render_context: m.initial_render_context,
            invalidate_iterations: serde_json::from_value(m.invalidate_iterations)
                .map_err(|e| DbErr::Custom(e.to_string()))?,
        })
    }
}

/// Merge `initial_render_context` (seed-time iteration context) under
/// the rebuilt-from-results context. Keys in the rebuilt context win on
/// conflict — an inner step result with the same name as a parent step
/// result shadows the parent, matching the decider's natural shadowing
/// behavior when both are present in `state.render_context`.
fn merge_render_contexts(initial: &Value, rebuilt: Value) -> Value {
    let Some(initial_obj) = initial.as_object() else {
        return rebuilt;
    };
    if initial_obj.is_empty() {
        return rebuilt;
    }
    let Some(rebuilt_obj) = rebuilt.as_object() else {
        return Value::Object(initial_obj.clone());
    };
    let mut merged = initial_obj.clone();
    for (k, v) in rebuilt_obj {
        merged.insert(k.clone(), v.clone());
    }
    Value::Object(merged)
}

/// Build the minijinja render context from accumulated step results.
///
/// This is the same transformation that `update_render_context` applies
/// incrementally during execution: convert each row-oriented result to
/// column-oriented format and merge into an object keyed by step name.
fn rebuild_render_context(results: &HashMap<String, Value>) -> Value {
    let mut ctx = serde_json::Map::with_capacity(results.len());
    for (name, value) in results {
        ctx.insert(
            name.clone(),
            crate::step_orchestrator::to_column_oriented(value),
        );
    }
    Value::Object(ctx)
}

// ── Facade functions ───────────────────────────────────────────────────────

/// Insert the initial workflow state row when a workflow run is seeded.
pub async fn insert_workflow_state(
    db: &DatabaseConnection,
    state: &WorkflowRunState,
) -> Result<(), DbErr> {
    let workflow_config =
        serde_json::to_value(&state.workflow).map_err(|e| DbErr::Custom(e.to_string()))?;
    let prior_step_hashes =
        serde_json::to_value(&state.prior_step_hashes).map_err(|e| DbErr::Custom(e.to_string()))?;
    let prior_results =
        serde_json::to_value(&state.prior_results).map_err(|e| DbErr::Custom(e.to_string()))?;
    let invalidate_iterations = serde_json::to_value(&state.invalidate_iterations)
        .map_err(|e| DbErr::Custom(e.to_string()))?;
    crud::insert_state(
        db,
        &state.run_id,
        &state.workflow_yaml_hash,
        workflow_config,
        state.workflow_context.clone(),
        state.variables.clone(),
        &state.trace_id,
        state.retry_from_run_id.as_deref(),
        state.cache_enabled,
        prior_step_hashes,
        prior_results,
        state.initial_render_context.clone(),
        invalidate_iterations,
    )
    .await
}

/// Load the workflow state for a run. Returns `None` if not found.
pub async fn load_workflow_state(
    db: &DatabaseConnection,
    run_id: &str,
) -> Result<Option<WorkflowRunState>, DbErr> {
    match crud::load_state(db, run_id).await? {
        Some(model) => Ok(Some(WorkflowRunState::try_from(model)?)),
        None => Ok(None),
    }
}

/// Persist updated workflow state with optimistic concurrency.
///
/// **Prefer [`commit_decision`] in production code paths.** This function
/// rewrites the entire `results` JSONB column on every call, producing the
/// O(S²·R) write pattern that `commit_decision`'s incremental delta path
/// was introduced to eliminate. It is retained for test scaffolding (e.g.
/// simulating a racing worker that bumps `decision_version`).
///
/// Returns `Ok(true)` on success, `Ok(false)` if another worker raced ahead
/// (version mismatch — caller should discard and retry from fresh state).
///
/// Uses `decision_version` as the expected version for the `WHERE` clause
/// and increments it atomically. The decider does NOT modify `decision_version`
/// — version management is owned by the persistence layer.
pub async fn update_workflow_state(
    db: &DatabaseConnection,
    state: &WorkflowRunState,
) -> Result<bool, DbErr> {
    let results = serde_json::to_value(&state.results).map_err(|e| DbErr::Custom(e.to_string()))?;
    let pending_children =
        serde_json::to_value(&state.pending_children).map_err(|e| DbErr::Custom(e.to_string()))?;
    crud::update_state(
        db,
        &state.run_id,
        state.decision_version,
        state.current_step as i32,
        results,
        state.render_context.clone(),
        pending_children,
    )
    .await
}
