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
use super::crud::update_state_in_txn;

/// A single decision-boundary commit.
#[derive(Debug)]
pub struct DecisionCommit {
    /// Workflow run id — both the `agentic_runs.id` and `agentic_workflow_state.run_id`.
    pub run_id: String,
    /// Queue task id for the decision worker's claim. Flipped to `completed`
    /// or `failed` only for terminal variants; left untouched on `Continuing`.
    pub decision_task_id: String,
    /// `decision_version` read alongside `new_state` — the CAS predicate.
    pub expected_version: i64,
    /// Post-decision state. Version is managed by the persistence layer; the
    /// decider does not set it.
    pub new_state: WorkflowRunState,
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
    validate_invariants(&commit)?;

    let txn = db.begin().await?;

    let state_json = serde_json::to_value(&commit.new_state.results)
        .map_err(|e| DbErr::Custom(format!("serialize results: {e}")))?;
    let pending_json = serde_json::to_value(&commit.new_state.pending_children)
        .map_err(|e| DbErr::Custom(format!("serialize pending_children: {e}")))?;

    let updated = update_state_in_txn(
        &txn,
        &commit.new_state.run_id,
        commit.expected_version,
        commit.new_state.current_step as i32,
        state_json,
        commit.new_state.render_context.clone(),
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

    // Insert via raw SQL keyed by (run_id, seq) with ON CONFLICT DO NOTHING
    // so a retried commit that racing workers both attempt is idempotent on
    // the event table — the CAS on decision_version is the correctness gate.
    let sql = "\
        INSERT INTO agentic_run_events (run_id, seq, event_type, payload, attempt, created_at) \
        VALUES ($1, $2, $3, $4, $5, $6) \
        ON CONFLICT (run_id, seq) DO NOTHING";
    for (i, (event_type, payload)) in events.iter().enumerate() {
        let seq = next_seq + i as i64;
        let stmt = Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Postgres,
            sql,
            [
                run_id.into(),
                seq.into(),
                event_type.as_str().into(),
                payload.clone().into(),
                attempt.into(),
                now.into(),
            ],
        );
        txn.execute(stmt).await?;
    }
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
