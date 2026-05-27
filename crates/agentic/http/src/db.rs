//! Database helpers for the agentic HTTP module.
//!
//! Pure re-exports from `agentic_runtime::crud` (domain-agnostic)
//! and `agentic_pipeline` (domain-aware facades).

// ── Re-exports from runtime (generic, domain-agnostic) ──────────────────────

pub use agentic_runtime::crud::{
    AirwayTableSummary, EventRow, LlmTokenSummary, LlmTokenSummaryByRun, QueueStats, QueueTaskRow,
    ScheduleDurationBaseline, ToolExchangeRow, WorkflowStepSummary, airway_table_summary_for_run,
    batch_insert_events, cleanup_stale_runs, delete_events_from_seq, fetch_duration_baselines,
    get_all_events, get_effective_run_state, get_events_after, get_max_seq,
    get_outcomes_for_parent, get_queue_stats, get_run, get_run_by_thread, get_runs_by_thread,
    get_suspension, get_thread_history_with_events, insert_event, list_active_runs,
    list_recent_runs, list_runs_filtered, llm_usage_for_run, llm_usage_for_runs, load_task_tree,
    load_task_tree_in_workspace, runs_in_workspace, transition_run, update_run_failed,
    update_run_running, update_run_suspended, update_run_terminal_from_events, upsert_suspension,
    user_facing_status, workflow_step_summary_for_run,
};

// ── Re-exports from pipeline (domain-aware facades) ─────────────────────────

pub use agentic_pipeline::{
    ThreadHistoryTurn, get_analytics_extension, get_analytics_extensions, get_thread_history,
    insert_run, update_run_done, update_run_thinking_mode,
};
