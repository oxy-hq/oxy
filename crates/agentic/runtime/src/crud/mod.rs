//! Generic CRUD operations for agentic pipeline runs, events, and suspensions.
//!
//! This module is split by entity and concern:
//! - [`runs`]: lifecycle transitions on the `agentic_runs` table.
//! - [`queries`]: read-side access to runs and thread history.
//! - [`events`]: inserts and queries on `agentic_run_events`.
//! - [`suspension`]: upsert/read on `agentic_run_suspensions`.
//! - [`outcomes`]: child outcome recording + multi-table transactional helpers.
//! - [`recovery`]: startup cleanup and resume enumeration.
//! - [`queue`]: the durable task queue (`agentic_task_queue`).

use sea_orm::{ActiveValue::*, DatabaseConnection, DbErr, EntityTrait};
use serde_json::Value;

use crate::entity::run;

pub mod events;
pub mod outcomes;
pub mod queries;
pub mod queue;
pub mod recovery;
pub mod runs;
pub mod suspension;

pub use events::{
    EventRow, batch_insert_events, delete_events_from_seq, get_all_events, get_events_after,
    get_max_seq, insert_event,
};
pub use outcomes::{
    complete_child_done_txn, complete_child_failed_txn, get_outcomes_for_parent, get_run_answer,
    insert_child_run, insert_task_outcome, suspend_with_data_txn,
};
pub use queries::{
    ThreadHistoryTurn, ToolExchangeRow, get_effective_run_state, get_run, get_run_by_thread,
    get_runs_by_thread, get_thread_history, get_thread_history_with_events, list_active_runs,
    list_recent_runs, list_runs_filtered,
};
pub use queue::{
    QueueStats, QueueTaskRow, cancel_queued_task, claim_task, complete_queue_task, enqueue_task,
    fail_queue_task, get_queue_entry, get_queue_stats, reap_stale_tasks, requeue_task,
    update_queue_heartbeat,
};
pub use recovery::{
    StuckRun, cleanup_stale_runs, find_stuck_workflow_runs, get_active_root_runs,
    get_max_child_counter, get_resumable_root_runs, increment_attempt, mark_recovery_failed,
};
pub use runs::{
    insert_run, insert_run_with_parent, load_task_tree, update_run_done, update_run_failed,
    update_run_running, update_run_suspended, update_run_terminal_from_events, update_task_status,
};
pub use suspension::{get_suspension, upsert_suspension};

pub(crate) fn now() -> chrono::DateTime<chrono::FixedOffset> {
    chrono::Utc::now().fixed_offset()
}

/// Derive the user-facing status from the internal task_status.
/// Used by the API serialization layer — NOT stored in DB.
pub fn user_facing_status(task_status: Option<&str>) -> &str {
    match task_status {
        Some("running") | Some("delegating") | None => "running",
        Some("awaiting_input") => "suspended",
        Some("done") => "done",
        Some("failed") | Some("timed_out") => "failed",
        Some("cancelled") => "cancelled",
        _ => "running",
    }
}

/// Atomic state transition for a run. Sets task_status and optionally
/// answer/error_message/task_metadata in a single UPDATE.
pub async fn transition_run(
    db: &DatabaseConnection,
    run_id: &str,
    task_status: &str,
    task_metadata: Option<Value>,
    answer: Option<&str>,
    error_message: Option<&str>,
) -> Result<(), DbErr> {
    let mut model = run::ActiveModel {
        id: Set(run_id.to_string()),
        task_status: Set(Some(task_status.to_string())),
        updated_at: Set(now()),
        ..Default::default()
    };
    if let Some(meta) = task_metadata {
        model.task_metadata = Set(Some(meta));
    }
    if let Some(ans) = answer {
        model.answer = Set(Some(ans.to_string()));
    }
    if let Some(err) = error_message {
        model.error_message = Set(Some(err.to_string()));
    }
    run::Entity::update(model).exec(db).await?;
    Ok(())
}
