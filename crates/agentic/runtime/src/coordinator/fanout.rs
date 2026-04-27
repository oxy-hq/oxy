//! Per-parent child-result accumulation and resume logic.

use std::collections::HashMap;

use agentic_core::delegation::{ChildCompletion, FanoutFailurePolicy, TaskAssignment, TaskSpec};
use serde_json::{Value, json};

use crate::crud;
use crate::state::RunStatus;

use super::{ChildResult, Coordinator, TaskStatus};

impl Coordinator {
    /// Record a child's result and check if the fan-out is complete.
    ///
    /// For single-child delegations this behaves exactly as before: the parent
    /// is resumed immediately when the one child finishes.
    ///
    /// For multi-child fan-outs:
    /// - **FailFast**: first child failure cancels siblings and resumes parent.
    /// - **BestEffort**: waits for all children, then resumes with aggregated results.
    pub(super) async fn record_child_result(
        &mut self,
        parent_id: &str,
        child_id: &str,
        result: ChildResult,
    ) {
        enum NextAction {
            ResumeImmediate(String),
            FailFast {
                siblings: Vec<String>,
                parent_run_id: String,
                meta: Value,
            },
            AllDone {
                parent_run_id: String,
                meta: Value,
            },
            StillWaiting {
                parent_run_id: String,
                meta: Value,
            },
        }

        // Determine next action while holding the mutable borrow, then release.
        let action = {
            let Some(parent_node) = self.tasks.get_mut(parent_id) else {
                return;
            };

            match &mut parent_node.status {
                TaskStatus::WaitingOnChildren {
                    child_task_ids,
                    completed,
                    failure_policy,
                } => {
                    completed.insert(child_id.to_string(), result.clone());
                    let total = child_task_ids.len();
                    let done_count = completed.len();

                    tracing::info!(
                        target: "coordinator",
                        parent_id,
                        child_id,
                        done_count,
                        total,
                        "child result recorded"
                    );

                    let completed_json = Self::serialize_completed(completed);
                    let child_ids_json: Vec<String> = child_task_ids.clone();
                    let meta = json!({
                        "child_task_ids": child_ids_json,
                        "completed": completed_json,
                        "failure_policy": serde_json::to_value(&*failure_policy).unwrap_or_default(),
                    });
                    let parent_run_id = parent_node.run_id.clone();

                    let is_failure = matches!(result, ChildResult::Failed(_));
                    let should_fail_fast =
                        is_failure && matches!(failure_policy, FanoutFailurePolicy::FailFast);
                    let all_done = done_count >= total;

                    if should_fail_fast {
                        let siblings: Vec<String> = child_task_ids
                            .iter()
                            .filter(|id| !completed.contains_key(id.as_str()))
                            .cloned()
                            .collect();
                        NextAction::FailFast {
                            siblings,
                            parent_run_id,
                            meta,
                        }
                    } else if all_done {
                        NextAction::AllDone {
                            parent_run_id,
                            meta,
                        }
                    } else {
                        NextAction::StillWaiting {
                            parent_run_id,
                            meta,
                        }
                    }
                }
                _ => {
                    // Parent is not waiting on children — resume directly.
                    let answer = match result {
                        ChildResult::Done(a) => a,
                        ChildResult::Failed(msg) => format!("Delegation failed: {msg}"),
                    };
                    NextAction::ResumeImmediate(answer)
                }
            }
        };

        // Mutable borrow released — do async work.
        match action {
            NextAction::ResumeImmediate(answer) => {
                self.resume_parent(parent_id, answer).await;
            }
            NextAction::FailFast {
                siblings,
                parent_run_id,
                meta,
            } => {
                self.persist_task_status(&parent_run_id, "delegating", Some(meta))
                    .await;
                for sibling_id in &siblings {
                    self.transport.cancel(sibling_id).await.ok();
                    if let Some(sibling_node) = self.tasks.get_mut(sibling_id) {
                        sibling_node.status = TaskStatus::Failed;
                        sibling_node.suspended_at = None;
                    }
                }
                let answer = self.aggregate_child_results(parent_id);
                self.resume_parent(parent_id, answer).await;
            }
            NextAction::AllDone {
                parent_run_id,
                meta,
            } => {
                self.persist_task_status(&parent_run_id, "delegating", Some(meta))
                    .await;
                let answer = self.aggregate_child_results(parent_id);
                self.resume_parent(parent_id, answer).await;
            }
            NextAction::StillWaiting {
                parent_run_id,
                meta,
            } => {
                self.persist_task_status(&parent_run_id, "delegating", Some(meta))
                    .await;
            }
        }
    }

    /// Aggregate completed child results into a single answer string.
    ///
    /// For single-child delegations: returns the child's answer directly.
    /// For multi-child: returns a JSON object `{ "child_id": { "status": ..., "answer"|"error": ... } }`.
    pub(super) fn aggregate_child_results(&self, parent_id: &str) -> String {
        let Some(parent_node) = self.tasks.get(parent_id) else {
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
        if child_task_ids.len() == 1
            && let Some(result) = completed.get(&child_task_ids[0])
        {
            return match result {
                ChildResult::Done(a) => a.clone(),
                ChildResult::Failed(msg) => format!("Delegation failed: {msg}"),
            };
        }

        // Multi-child: aggregate as JSON.
        let aggregated = Self::serialize_completed(completed);
        serde_json::to_string(&aggregated).unwrap_or_else(|_| "{}".to_string())
    }

    pub(super) fn serialize_completed(completed: &HashMap<String, ChildResult>) -> Value {
        let mut obj = serde_json::Map::new();
        for (id, result) in completed {
            let entry = match result {
                ChildResult::Done(answer) => json!({ "status": "done", "answer": answer }),
                ChildResult::Failed(error) => json!({ "status": "failed", "error": error }),
            };
            obj.insert(id.clone(), entry);
        }
        Value::Object(obj)
    }

    /// Walk parent links from `task_id` up to the tree root and return that
    /// root's `run_id`.
    fn root_run_id_of(&self, task_id: &str) -> Option<String> {
        let mut current = task_id.to_string();
        loop {
            let node = self.tasks.get(&current)?;
            match &node.parent_task_id {
                Some(p) => current = p.clone(),
                None => return Some(node.run_id.clone()),
            }
        }
    }

    /// True if the root ancestor of `task_id` has been user-cancelled.
    ///
    /// `RuntimeState::cancel` marks `statuses[root_run_id] = Cancelled`
    /// synchronously; this lookup lets the coordinator short-circuit parent
    /// resumes when a delegated child finishes after the user clicked cancel.
    pub(super) fn is_subtree_user_cancelled(&self, task_id: &str) -> bool {
        let Some(root_run_id) = self.root_run_id_of(task_id) else {
            return false;
        };
        self.state
            .statuses
            .get(&root_run_id)
            .map(|r| matches!(r.value(), RunStatus::Cancelled))
            .unwrap_or(false)
    }

    /// Resume a suspended parent task by assigning a `TaskSpec::Resume` to the
    /// worker.
    ///
    /// Always emits an `input_resolved` event to pair with the
    /// `awaiting_input` event the orchestrator emitted on suspension. This
    /// applies to both human answers and delegation completions — the
    /// awaiting/resolved pair is suspend-reason-agnostic.
    pub(super) async fn resume_parent(&mut self, parent_id: &str, answer: String) {
        // Short-circuit: if the user cancelled the root while this parent was
        // suspended on a delegation, don't rebuild the pipeline just because
        // the child happened to finish. Finalise the parent via
        // `handle_cancelled`, which also cascades the cancellation up to the
        // root run.
        //
        // `handle_cancelled` → `record_child_result` → `resume_parent` forms
        // an async cycle; `Box::pin` breaks the unbounded recursive future
        // size so the compiler is happy.
        if self.is_subtree_user_cancelled(parent_id) {
            let already_finalised = self
                .tasks
                .get(parent_id)
                .map(|n| matches!(n.status, TaskStatus::Failed))
                .unwrap_or(true);
            if already_finalised {
                return;
            }
            tracing::info!(
                target: "coordinator",
                parent_id,
                "root run user-cancelled; skipping parent resume"
            );
            Box::pin(self.handle_cancelled(parent_id)).await;
            return;
        }

        tracing::info!(
            target: "coordinator",
            parent_id,
            answer_len = answer.len(),
            "resuming parent task"
        );
        // Extract all needed data from the mutable borrow, then release it.
        let (run_id, resume_data, seq, child_task_id_hint) = {
            let Some(parent_node) = self.tasks.get_mut(parent_id) else {
                return;
            };

            // For workflow decision chaining, grab the child task ID before
            // transitioning to Running (it lives in WaitingOnChildren).
            let child_task_id_hint = match &parent_node.status {
                TaskStatus::WaitingOnChildren { child_task_ids, .. } => {
                    child_task_ids.first().cloned()
                }
                _ => None,
            };

            parent_node.status = TaskStatus::Running;
            parent_node.suspended_at = None;
            let run_id = parent_node.run_id.clone();

            let resume_data = match parent_node.suspend_data.take() {
                Some(data) => data,
                None => {
                    tracing::error!(target: "coordinator", parent_id, "no suspend data for resume");
                    return;
                }
            };

            let seq = parent_node.next_seq;
            parent_node.next_seq += 1;
            (run_id, resume_data, seq, child_task_id_hint)
        };

        // Persist task_status transition back to running (single write).
        self.persist_task_status(&run_id, "running", None).await;
        let payload = json!({ "answer": &answer, "trace_id": &resume_data.trace_id });
        crud::insert_event(
            &self.db,
            &run_id,
            seq,
            "input_resolved",
            &payload,
            self.attempt,
        )
        .await
        .ok();
        self.state.notify(&run_id);

        self.state
            .statuses
            .insert(run_id.clone(), RunStatus::Running);

        // ── Temporal-style workflow decision task: enqueue WorkflowDecision ─
        if resume_data.from_state == "workflow_decision" {
            let step_name = resume_data
                .stage_data
                .get("step_name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let step_index = resume_data
                .stage_data
                .get("step_index")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;

            let pending_child_answer = child_task_id_hint.map(|ctid| ChildCompletion {
                child_task_id: ctid,
                step_index,
                step_name,
                status: "done".to_string(),
                answer: answer.clone(),
            });

            let assignment = TaskAssignment {
                task_id: parent_id.to_string(),
                parent_task_id: None,
                run_id: run_id.clone(),
                spec: TaskSpec::WorkflowDecision {
                    run_id: run_id.clone(),
                    pending_child_answer,
                },
                policy: None,
            };
            if let Err(e) = self.transport.assign(assignment).await {
                self.fail_parent_on_assign_error(parent_id, &run_id, "WorkflowDecision", e)
                    .await;
            }
            return;
        }

        // ── Non-workflow: assign TaskSpec::Resume ───────────────────────────
        //
        // For analytics/builder pipelines that have a SuspendedRunData checkpoint,
        // assign a TaskSpec::Resume so a fresh pipeline is built from that data.
        let assignment = TaskAssignment {
            task_id: parent_id.to_string(),
            parent_task_id: None,
            run_id: run_id.clone(),
            spec: TaskSpec::Resume {
                run_id: run_id.clone(),
                resume_data,
                answer,
            },
            policy: None,
        };

        if let Err(e) = self.transport.assign(assignment).await {
            self.fail_parent_on_assign_error(parent_id, &run_id, "Resume", e)
                .await;
        }
    }

    /// Fail the parent run when scheduling its next task fails.
    ///
    /// Previously we logged and returned, leaving the run in `task_status =
    /// running` forever: the decision task's queue row had already been
    /// released by the prior worker, so the reaper couldn't resurrect it and
    /// the UI saw a permanent "still running" state. Flipping the run to
    /// `failed` here surfaces the error rather than hanging.
    pub(super) async fn fail_parent_on_assign_error(
        &self,
        parent_id: &str,
        run_id: &str,
        target: &str,
        err: impl std::fmt::Display,
    ) {
        let err_text = format!("failed to schedule {target}: {err}");
        tracing::error!(
            target: "coordinator",
            parent_id,
            run_id,
            assign_target = target,
            error = %err,
            "failed to assign follow-up task to worker — failing parent run"
        );
        if let Err(persist_err) = crud::update_run_failed(&self.db, run_id, &err_text).await {
            tracing::error!(
                target: "coordinator",
                parent_id,
                run_id,
                error = %persist_err,
                "failed to persist parent failure after assign error"
            );
        }
        self.state
            .statuses
            .insert(run_id.to_string(), RunStatus::Failed(err_text));
        self.state.notify(run_id);
    }
}
