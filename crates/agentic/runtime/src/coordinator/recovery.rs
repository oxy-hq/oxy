//! Rebuild the coordinator's in-memory state from persisted runs after a restart.

use std::collections::HashMap;
use std::sync::Arc;

use agentic_core::delegation::{TaskPolicy, TaskSpec};
use agentic_core::transport::CoordinatorTransport;
use sea_orm::DatabaseConnection;

use crate::crud;
use crate::state::RuntimeState;

use super::{
    ChildResult, Coordinator, DEFAULT_DRAIN_TIMEOUT, DEFAULT_SUSPEND_TIMEOUT, TaskNode, TaskStatus,
};

/// A parent task that needs to be resumed after crash recovery because all
/// its children completed (outcomes found in `agentic_task_outcomes`) but the
/// parent was never resumed before the crash.
#[derive(Debug)]
pub struct PendingResume {
    pub parent_task_id: String,
    pub answer: String,
}

impl Coordinator {
    /// Reconstruct a coordinator from persisted task tree state.
    ///
    /// Loads the task tree for `root_run_id` from the database and rebuilds
    /// the in-memory `tasks` map. Event sequence counters are derived from
    /// `get_max_seq`. Suspended tasks get a fresh timeout clock.
    ///
    /// **Crash-consistency**: For tasks in `WaitingOnChildren`, the `completed`
    /// map is rebuilt from the `agentic_task_outcomes` table (the atomic source
    /// of truth), not from `task_metadata` JSONB. This closes the window where
    /// a child completes but the parent's metadata hasn't been updated yet.
    ///
    /// After rebuilding, any parent whose children are all terminal will be
    /// detected and queued for resume via the returned `pending_resumes` list.
    pub async fn from_db(
        db: DatabaseConnection,
        state: Arc<RuntimeState>,
        transport: Arc<dyn CoordinatorTransport>,
        root_run_id: &str,
    ) -> Result<(Self, Vec<PendingResume>), sea_orm::DbErr> {
        let tree = crud::load_task_tree(&db, root_run_id).await?;
        let mut tasks = HashMap::new();

        for row in &tree {
            let next_seq = crud::get_max_seq(&db, &row.id).await? + 1;

            let status = match row.task_status.as_deref() {
                Some("running") => TaskStatus::Running,
                Some("awaiting_input") => TaskStatus::SuspendedHuman,
                Some("delegating") => {
                    let meta = row.task_metadata.as_ref();

                    // Get child_task_ids from metadata (needed for the list of
                    // expected children).
                    let child_task_ids: Vec<String> = meta
                        .and_then(|m| m["child_task_ids"].as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(ToString::to_string))
                                .collect()
                        })
                        .or_else(|| {
                            // Legacy single-child format.
                            meta.and_then(|m| m["child_task_id"].as_str())
                                .map(|id| vec![id.to_string()])
                        })
                        .unwrap_or_default();

                    // Rebuild `completed` from the task_outcomes table — the
                    // atomic source of truth — instead of task_metadata JSONB.
                    let outcomes = crud::get_outcomes_for_parent(&db, &row.id).await?;
                    let completed: HashMap<String, ChildResult> = outcomes
                        .into_iter()
                        .map(|o| {
                            let result = match o.status.as_str() {
                                "done" => ChildResult::Done(o.answer.unwrap_or_default()),
                                _ => ChildResult::Failed(o.answer.unwrap_or_default()),
                            };
                            (o.child_id, result)
                        })
                        .collect();

                    let failure_policy = meta
                        .and_then(|m| serde_json::from_value(m["failure_policy"].clone()).ok())
                        .unwrap_or_default();

                    TaskStatus::WaitingOnChildren {
                        child_task_ids,
                        completed,
                        failure_policy,
                    }
                }
                Some("done") => TaskStatus::Done,
                Some("failed") => TaskStatus::Failed,

                // "needs_resume" / "shutdown" / "running" — check if this task
                // has children in the tree. If so, it was delegating before the
                // crash and the reaper changed its status; reconstruct as
                // WaitingOnChildren so the coordinator correctly waits for
                // children to complete before resuming this task.
                _ => {
                    let has_children = tree
                        .iter()
                        .any(|t| t.parent_run_id.as_deref() == Some(&row.id));
                    if has_children {
                        let meta = row.task_metadata.as_ref();
                        let child_task_ids: Vec<String> = meta
                            .and_then(|m| m["child_task_ids"].as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(ToString::to_string))
                                    .collect()
                            })
                            .or_else(|| {
                                // Derive from tree if metadata is missing.
                                Some(
                                    tree.iter()
                                        .filter(|t| t.parent_run_id.as_deref() == Some(&row.id))
                                        .map(|t| t.id.clone())
                                        .collect(),
                                )
                            })
                            .unwrap_or_default();

                        let outcomes = crud::get_outcomes_for_parent(&db, &row.id).await?;
                        let completed: HashMap<String, ChildResult> = outcomes
                            .into_iter()
                            .map(|o| {
                                let result = match o.status.as_str() {
                                    "done" => ChildResult::Done(o.answer.unwrap_or_default()),
                                    _ => ChildResult::Failed(o.answer.unwrap_or_default()),
                                };
                                (o.child_id, result)
                            })
                            .collect();

                        let failure_policy = meta
                            .and_then(|m| serde_json::from_value(m["failure_policy"].clone()).ok())
                            .unwrap_or_default();

                        TaskStatus::WaitingOnChildren {
                            child_task_ids,
                            completed,
                            failure_policy,
                        }
                    } else {
                        TaskStatus::Running
                    }
                }
            };

            let suspended_at = match &status {
                TaskStatus::SuspendedHuman | TaskStatus::WaitingOnChildren { .. } => {
                    Some(tokio::time::Instant::now())
                }
                _ => None,
            };

            // Reload suspension data for suspended tasks.
            let suspend_data = match &status {
                TaskStatus::SuspendedHuman | TaskStatus::WaitingOnChildren { .. } => {
                    crud::get_suspension(&db, &row.id).await?
                }
                _ => None,
            };

            // Restore retry state from task_metadata if present.
            let meta = row.task_metadata.as_ref();
            let attempt = meta.and_then(|m| m["attempt"].as_u64()).unwrap_or(0) as u32;
            let fallback_index =
                meta.and_then(|m| m["fallback_index"].as_u64()).unwrap_or(0) as usize;
            let policy: Option<TaskPolicy> =
                meta.and_then(|m| serde_json::from_value(m["policy"].clone()).ok());
            let original_spec: Option<TaskSpec> =
                meta.and_then(|m| serde_json::from_value(m["original_spec"].clone()).ok());

            tasks.insert(
                row.id.clone(),
                TaskNode {
                    run_id: row.id.clone(),
                    parent_task_id: row.parent_run_id.clone(),
                    status,
                    suspend_data,
                    next_seq,
                    suspended_at,
                    original_spec,
                    policy,
                    attempt,
                    fallback_index,
                },
            );
        }

        // Detect parents whose children are all terminal — these need to be
        // resumed. This handles the crash window where a child's outcome was
        // written to `agentic_task_outcomes` but the parent was never resumed.
        let mut pending_resumes = Vec::new();
        let task_ids: Vec<String> = tasks.keys().cloned().collect();
        for task_id in &task_ids {
            let all_done = {
                let Some(node) = tasks.get(task_id) else {
                    continue;
                };
                match &node.status {
                    TaskStatus::WaitingOnChildren {
                        child_task_ids,
                        completed,
                        ..
                    } => !child_task_ids.is_empty() && completed.len() >= child_task_ids.len(),
                    _ => false,
                }
            };

            if all_done {
                // Aggregate the answer exactly as the live code path does.
                let answer = Self::aggregate_child_results_static(&tasks, task_id);
                pending_resumes.push(PendingResume {
                    parent_task_id: task_id.clone(),
                    answer,
                });
            }
        }

        // Query DB for the max child counter across ALL runs in the tree,
        // not just the ones loaded into the tasks HashMap. This prevents PK
        // collisions when previous recovery attempts created children that
        // may not be in the current tree (e.g., if they were orphaned).
        let max_counter = crud::get_max_child_counter(&db, root_run_id).await?;

        // Get attempt from root run (already incremented by recovery caller).
        let root_run = crud::get_run(&db, root_run_id).await?;
        let attempt = root_run.map(|r| r.attempt).unwrap_or(0);

        Ok((
            Self {
                db,
                state,
                transport,
                tasks,
                child_counter: max_counter,
                attempt,
                answer_rxs: HashMap::new(),
                suspend_timeout: DEFAULT_SUSPEND_TIMEOUT,
                drain_timeout: DEFAULT_DRAIN_TIMEOUT,
            },
            pending_resumes,
        ))
    }

    /// Aggregate child results without needing `&self` (used during recovery).
    fn aggregate_child_results_static(
        tasks: &HashMap<String, TaskNode>,
        parent_id: &str,
    ) -> String {
        let Some(parent_node) = tasks.get(parent_id) else {
            return "No results".to_string();
        };
        let TaskStatus::WaitingOnChildren {
            child_task_ids,
            completed,
            ..
        } = &parent_node.status
        else {
            return "No results".to_string();
        };

        // Single-child: return the answer directly (backward compatible).
        if child_task_ids.len() == 1 {
            if let Some(result) = completed.get(&child_task_ids[0]) {
                return match result {
                    ChildResult::Done(a) => a.clone(),
                    ChildResult::Failed(msg) => format!("Delegation failed: {msg}"),
                };
            }
        }

        // Multi-child: aggregate as JSON.
        let aggregated = Self::serialize_completed(completed);
        serde_json::to_string(&aggregated).unwrap_or_else(|_| "{}".to_string())
    }
}
