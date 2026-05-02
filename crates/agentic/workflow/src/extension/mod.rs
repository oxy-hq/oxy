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

        // Reconstruct render_context from step results rather than reading the
        // DB column.  The `render_context` column is now always persisted as
        // `{}` by commit_decision (see apply_result_delta_in_txn), making it a
        // vestigial field.  Deriving it here keeps the hot-path writes O(1 new
        // result) while still giving the decider a correct context on every load.
        let render_context = rebuild_render_context(&results);

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
        })
    }
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
    crud::insert_state(
        db,
        &state.run_id,
        &state.workflow_yaml_hash,
        workflow_config,
        state.workflow_context.clone(),
        state.variables.clone(),
        &state.trace_id,
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
