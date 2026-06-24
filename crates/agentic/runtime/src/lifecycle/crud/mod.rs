//! Lifecycle-side CRUD: the run row, its event log, suspensions, and
//! the read-side queries that join them.
//!
//! The shared `now()` / `user_facing_status` / `transition_run`
//! helpers live here because they operate on the run row. Orchestrator
//! CRUD reaches into them via `crate::orchestrator::crud::super::…`
//! (re-exported at `crate::crud::*` for the back-compat surface).

use sea_orm::{ActiveValue::*, DatabaseConnection, DbErr, EntityTrait};
use serde_json::Value;

use crate::lifecycle::entity::run;

pub mod events;
pub mod queries;
pub mod runs;
pub mod suspension;

pub use events::{
    EventRow, batch_insert_events, delete_events_from_seq, get_all_events, get_all_events_for_runs,
    get_events_after, get_max_seq, insert_event,
};
pub use queries::{
    AirwayTableSummary, AutomationStepSummary, LlmTokenSummary, LlmTokenSummaryByRun,
    ScheduleDurationBaseline, ThreadHistoryTurn, ToolExchangeRow, airway_table_summary_for_run,
    automation_step_summary_for_run, fetch_duration_baselines, get_effective_run_state, get_run,
    get_run_by_thread, get_runs_by_thread, get_thread_history, get_thread_history_with_events,
    list_active_runs, list_recent_runs, list_runs_filtered, llm_usage_for_run, llm_usage_for_runs,
    runs_in_workspace,
};
pub use runs::{
    heartbeat_driver, insert_run, insert_run_with_parent, insert_run_with_schedule,
    is_cancel_requested, load_task_tree, load_task_tree_in_workspace, release_driver,
    request_cancel, try_acquire_driver, update_run_done, update_run_failed, update_run_running,
    update_run_suspended, update_run_terminal_from_events, update_task_status,
};
pub use suspension::{get_suspension, upsert_suspension};

pub fn now() -> chrono::DateTime<chrono::FixedOffset> {
    chrono::Utc::now().fixed_offset()
}

/// How long a driver lease (`agentic_runs.driver_id` /
/// `driver_heartbeat_at`) is honored without a heartbeat before another
/// driver may steal it. The driving loop must heartbeat well inside this
/// window (Task 6 owns the ticker). Gates recovery selection so a periodic
/// loop cannot double-drive a run a live driver already owns.
pub const DRIVER_LEASE_TTL_SECS: i64 = 90;

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
    // A terminal run needs no driver — clear the lease so it isn't left
    // dangling (and so observability doesn't show a "held" lease on a
    // finished run). Unconditional here is safe: the run is terminal, no
    // driver should still be acting on it.
    if matches!(task_status, "done" | "failed" | "cancelled" | "timed_out") {
        model.driver_id = Set(None);
        model.driver_heartbeat_at = Set(None);
    }
    run::Entity::update(model).exec(db).await?;
    Ok(())
}
