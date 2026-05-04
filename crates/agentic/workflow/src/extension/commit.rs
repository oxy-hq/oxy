//! Atomic write-point for a workflow decision boundary.
//!
//! A workflow decision ("decide what to do next, write the result") used to be
//! a scatter of independent writes: `update_state_in_txn` for the CAS, event
//! inserts through an mpsc channel + bridge, and — only after the worker's
//! outcome returned to the coordinator — the queue-status flip on the decision
//! task. Any failure or silent drop between those writes could advance the
//! workflow state without persisting the matching events or enqueueing the
//! follow-up, stranding the workflow indefinitely.
//!
//! [`commit_decision`] collapses the whole boundary into one SeaORM
//! transaction, conditional on `decision_version`. If the transaction commits,
//! state + events + (for terminal decisions) run-status + queue-status all
//! advanced together. If anything fails, nothing persists and the next worker
//! claim can retry cleanly.
//!
//! This is the same pattern Temporal calls `UpdateWorkflowExecutionAsActive`
//! (`service/history/workflow/transaction_impl.go`): history events, mutable
//! state, and transfer-queue tasks are written conditionally on the stored
//! `dbRecordVersion`, so a workflow task is either fully applied or fully
//! rolled back.

use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, FromQueryResult, Statement,
    TransactionTrait,
};
use serde_json::Value;

use super::WorkflowRunState;
use super::crud::apply_result_delta_in_txn;

/// A single decision-boundary commit.
///
/// **Invariant:** `run_id == new_state.run_id`. Both fields exist because the
/// commit writes to two tables (`agentic_run_events` keyed by `run_id`, and
/// `agentic_workflow_state` keyed by `new_state.run_id`). Callsites must keep
/// them in lockstep — a divergence would silently route events to the wrong
/// run while the state update succeeds.
#[derive(Debug)]
pub struct DecisionCommit {
    /// Workflow run id — both the `agentic_runs.id` and `agentic_workflow_state.run_id`.
    /// Must equal `new_state.run_id`.
    pub run_id: String,
    /// Queue task id for the decision worker's claim. Flipped to `completed`
    /// or `failed` only for terminal variants; left untouched on `Continuing`.
    pub decision_task_id: String,
    /// `decision_version` read alongside `new_state` — the CAS predicate.
    pub expected_version: i64,
    /// Post-decision state. Version is managed by the persistence layer; the
    /// decider does not set it.
    pub new_state: WorkflowRunState,
    /// The single new step result produced by this decision, as a one-key JSON
    /// object `{"step_name": result}`.  Pass `serde_json::json!({})` when this
    /// decision did not produce a new result (delegation, wait-for-siblings).
    ///
    /// Used for an incremental JSONB merge (`results || delta`) so each decision
    /// writes O(1 result) instead of O(all results), eliminating the O(S²) write
    /// pattern that caused slowdowns for long and loop-heavy workflows.
    pub result_delta: Value,
    /// Events to append to `agentic_run_events`. Assigned monotonic seqs
    /// starting at `max(seq) + 1` within the transaction.
    pub events: Vec<(String, Value)>,
    /// `attempt` value to tag inserted events with.
    pub attempt: i32,
    /// How to finalize the decision task's queue row + the workflow run.
    pub terminal: DecisionTerminal,
}

#[derive(Debug)]
pub enum DecisionTerminal {
    /// Workflow continuing — decision queue row untouched. The worker will
    /// still emit a `Suspended`/`Done` outcome that drives the next action
    /// (child enqueue, chaining) via the coordinator.
    Continuing,
    /// Workflow complete. Flips `agentic_runs.task_status` to `done` (with
    /// `final_answer`) and `agentic_task_queue.queue_status` to `completed`
    /// inside this transaction.
    CompleteWorkflow { final_answer: String },
    /// Workflow failed. Flips `agentic_runs.task_status` to `failed` (with
    /// `error`) and `agentic_task_queue.queue_status` to `failed`.
    FailWorkflow { error: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum CommitOutcome {
    /// All writes committed atomically.
    Committed,
    /// The `decision_version` CAS predicate didn't match. Nothing persisted —
    /// the decision task that produced this commit is stale and should be
    /// discarded. Another worker already advanced the workflow.
    VersionConflict,
}

/// Atomically apply a workflow decision.
///
/// See module docs for the write ordering guarantee. On `CompleteWorkflow`,
/// the state invariant `current_step >= workflow.tasks.len()` is enforced;
/// violating it is a decider bug and returns `DbErr::Custom` without writing
/// anything.
pub async fn commit_decision(
    db: &DatabaseConnection,
    commit: DecisionCommit,
) -> Result<CommitOutcome, DbErr> {
    debug_assert_eq!(
        commit.run_id, commit.new_state.run_id,
        "DecisionCommit.run_id must match new_state.run_id; \
         divergence would route events to a different run than the state update"
    );
    validate_invariants(&commit)?;

    let txn = db.begin().await?;

    let pending_json = serde_json::to_value(&commit.new_state.pending_children)
        .map_err(|e| DbErr::Custom(format!("serialize pending_children: {e}")))?;

    // Use the incremental delta path: merge only the new step result via
    // JSONB `||` rather than overwriting the full results map.  render_context
    // is always written as `{}` because it is derived from results at load time.
    let updated = apply_result_delta_in_txn(
        &txn,
        &commit.new_state.run_id,
        commit.expected_version,
        commit.new_state.current_step as i32,
        commit.result_delta,
        pending_json,
    )
    .await?;
    if !updated {
        txn.rollback().await?;
        return Ok(CommitOutcome::VersionConflict);
    }

    append_events_in_txn(&txn, &commit.run_id, &commit.events, commit.attempt).await?;

    match &commit.terminal {
        DecisionTerminal::Continuing => {}
        DecisionTerminal::CompleteWorkflow { final_answer } => {
            set_run_terminal_in_txn(&txn, &commit.run_id, "done", Some(final_answer), None).await?;
            set_queue_status_in_txn(&txn, &commit.decision_task_id, "completed").await?;
        }
        DecisionTerminal::FailWorkflow { error } => {
            set_run_terminal_in_txn(&txn, &commit.run_id, "failed", None, Some(error)).await?;
            set_queue_status_in_txn(&txn, &commit.decision_task_id, "failed").await?;
        }
    }

    txn.commit().await?;
    Ok(CommitOutcome::Committed)
}

fn validate_invariants(commit: &DecisionCommit) -> Result<(), DbErr> {
    if let DecisionTerminal::CompleteWorkflow { .. } = &commit.terminal {
        let total = commit.new_state.workflow.tasks.len();
        if commit.new_state.current_step < total {
            return Err(DbErr::Custom(format!(
                "CompleteWorkflow invariant: current_step ({}) < tasks.len ({})",
                commit.new_state.current_step, total
            )));
        }
    }
    Ok(())
}

/// Returns `($start, $start+1, ..., $start+cols-1)` and advances the caller's
/// counter by `cols`. The parentheses are included so callers can join groups
/// with ", " and paste directly into a VALUES clause.
fn build_row_placeholders(start: usize, cols: usize) -> String {
    let params: Vec<String> = (start..start + cols).map(|n| format!("${n}")).collect();
    format!("({})", params.join(", "))
}

async fn append_events_in_txn(
    txn: &DatabaseTransaction,
    run_id: &str,
    events: &[(String, Value)],
    attempt: i32,
) -> Result<(), DbErr> {
    if events.is_empty() {
        return Ok(());
    }
    let next_seq = next_seq_in_txn(txn, run_id).await?;
    let now = chrono::Utc::now().fixed_offset();

    // Build a single multi-row INSERT instead of one statement per event.
    // For N events this reduces from N+1 round-trips (1 SELECT + N inserts)
    // to exactly 2 (1 SELECT + 1 multi-row INSERT), which matters on
    // high-latency connections and for long/nested workflows.
    let mut param_groups: Vec<String> = Vec::with_capacity(events.len());
    let mut values: Vec<sea_orm::Value> = Vec::with_capacity(events.len() * 6);
    let mut p = 1usize;

    for (i, (event_type, payload)) in events.iter().enumerate() {
        let seq = next_seq + i as i64;
        param_groups.push(build_row_placeholders(p, 6));
        let row: [sea_orm::Value; 6] = [
            run_id.into(),
            seq.into(),
            event_type.as_str().into(),
            payload.clone().into(),
            attempt.into(),
            now.into(),
        ];
        values.extend(row);
        p += 6;
    }

    let sql = format!(
        "INSERT INTO agentic_run_events (run_id, seq, event_type, payload, attempt, created_at) \
         VALUES {} ON CONFLICT (run_id, seq) DO NOTHING",
        param_groups.join(", ")
    );

    let stmt = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Postgres, sql, values);
    txn.execute(stmt).await?;
    Ok(())
}

async fn next_seq_in_txn(txn: &DatabaseTransaction, run_id: &str) -> Result<i64, DbErr> {
    #[derive(FromQueryResult)]
    struct MaxSeqRow {
        max_seq: Option<i64>,
    }

    let row = MaxSeqRow::find_by_statement(Statement::from_sql_and_values(
        sea_orm::DatabaseBackend::Postgres,
        "SELECT MAX(seq) AS max_seq FROM agentic_run_events WHERE run_id = $1",
        [run_id.into()],
    ))
    .one(txn)
    .await?;
    Ok(row.and_then(|r| r.max_seq).map(|s| s + 1).unwrap_or(0))
}

async fn set_run_terminal_in_txn(
    txn: &DatabaseTransaction,
    run_id: &str,
    status: &str,
    answer: Option<&str>,
    error: Option<&str>,
) -> Result<(), DbErr> {
    let sql = "\
        UPDATE agentic_runs \
        SET task_status = $1, \
            answer = COALESCE($2, answer), \
            error_message = COALESCE($3, error_message), \
            updated_at = $4 \
        WHERE id = $5";
    let stmt = Statement::from_sql_and_values(
        sea_orm::DatabaseBackend::Postgres,
        sql,
        [
            status.into(),
            answer.into(),
            error.into(),
            chrono::Utc::now().fixed_offset().into(),
            run_id.into(),
        ],
    );
    txn.execute(stmt).await?;
    Ok(())
}

async fn set_queue_status_in_txn(
    txn: &DatabaseTransaction,
    task_id: &str,
    status: &str,
) -> Result<(), DbErr> {
    let sql = "\
        UPDATE agentic_task_queue \
        SET queue_status = $1, updated_at = $2 \
        WHERE task_id = $3";
    let stmt = Statement::from_sql_and_values(
        sea_orm::DatabaseBackend::Postgres,
        sql,
        [
            status.into(),
            chrono::Utc::now().fixed_offset().into(),
            task_id.into(),
        ],
    );
    txn.execute(stmt).await?;
    Ok(())
}
