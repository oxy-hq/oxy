//! Handlers for `WorkerMessage::Event` and `TaskOutcome` variants.

use agentic_core::delegation::{TaskAssignment, TaskOutcome, TaskSpec};
use serde_json::{Value, json};

use crate::crud;
use crate::state::RunStatus;

use super::{ChildResult, Coordinator, RetryAction, TaskStatus};

impl Coordinator {
    // ── Event handling ──────────────────────────────────────────────────

    pub(super) async fn handle_event(&mut self, task_id: &str, event_type: &str, payload: Value) {
        tracing::debug!(target: "coordinator", task_id, event_type, "handle_event");
        let Some(node) = self.tasks.get_mut(task_id) else {
            tracing::warn!(target: "coordinator", task_id, "event for unknown task");
            return;
        };

        let seq = node.next_seq;
        node.next_seq += 1;

        // Persist event for this task's own run.
        if let Err(e) = crud::insert_event(
            &self.db,
            &node.run_id,
            seq,
            event_type,
            &payload,
            self.attempt,
        )
        .await
        {
            tracing::error!(
                target: "coordinator",
                task_id,
                run_id = %node.run_id,
                seq,
                error = %e,
                "failed to persist event"
            );
        }
        self.state.notify(&node.run_id);

        // If this is a child task, inject a wrapped delegation_event into
        // the parent's event stream. Bubble all the way to the root so the
        // SSE stream (which reads from the root run) sees child events.
        let mut current_id = node.parent_task_id.clone();
        while let Some(ancestor_id) = current_id {
            if let Some(ancestor_node) = self.tasks.get_mut(&ancestor_id) {
                let ancestor_seq = ancestor_node.next_seq;
                ancestor_node.next_seq += 1;

                let wrapped = json!({
                    "child_task_id": task_id,
                    "inner_event_type": event_type,
                    "inner": payload,
                });
                if let Err(e) = crud::insert_event(
                    &self.db,
                    &ancestor_node.run_id,
                    ancestor_seq,
                    "delegation_event",
                    &wrapped,
                    self.attempt,
                )
                .await
                {
                    tracing::error!(
                        target: "coordinator",
                        ancestor_id = %ancestor_id,
                        run_id = %ancestor_node.run_id,
                        seq = ancestor_seq,
                        error = %e,
                        "failed to persist delegation_event"
                    );
                }
                self.state.notify(&ancestor_node.run_id);
                current_id = ancestor_node.parent_task_id.clone();
            } else {
                break;
            }
        }
    }

    // ── Outcome handling ────────────────────────────────────────────────

    pub(super) async fn handle_outcome(&mut self, task_id: &str, outcome: TaskOutcome) {
        let outcome_type = match &outcome {
            TaskOutcome::Done { .. } => "Done",
            TaskOutcome::Suspended { .. } => "Suspended",
            TaskOutcome::Failed(_) => "Failed",
            TaskOutcome::Cancelled => "Cancelled",
        };
        tracing::info!(target: "coordinator", task_id, outcome_type, "handle_outcome");
        match outcome {
            TaskOutcome::Done { answer, metadata } => {
                self.handle_done(task_id, answer, metadata).await;
            }
            TaskOutcome::Suspended {
                reason,
                resume_data,
                trace_id,
            } => {
                self.handle_suspended(task_id, reason, resume_data, trace_id)
                    .await;
            }
            TaskOutcome::Failed(msg) => {
                self.handle_failed(task_id, msg).await;
            }
            TaskOutcome::Cancelled => {
                self.handle_cancelled(task_id).await;
            }
        }
    }

    async fn handle_done(&mut self, task_id: &str, answer: String, metadata: Option<Value>) {
        // ── Workflow decision chaining (before setting Done status) ─────────
        if let Some(ref meta) = metadata {
            if meta.get("workflow_continue").and_then(|v| v.as_bool()) == Some(true) {
                // Inline step executed — chain immediately to next decision task.
                if let Some(node) = self.tasks.get(task_id) {
                    let run_id = node.run_id.clone();
                    let assignment = TaskAssignment {
                        task_id: task_id.to_string(),
                        parent_task_id: None,
                        run_id: run_id.clone(),
                        spec: TaskSpec::WorkflowDecision {
                            run_id: run_id.clone(),
                            pending_child_answer: None,
                        },
                        policy: None,
                    };
                    if let Err(e) = self.transport.assign(assignment).await {
                        // Cannot silently swallow: the prior worker's queue row
                        // was already flipped to `completed` by the transport,
                        // so no reaper will re-drive the workflow. Fail the
                        // parent run so the stall is observable.
                        self.fail_parent_on_assign_error(
                            task_id,
                            &run_id,
                            "WorkflowDecision (chain)",
                            e,
                        )
                        .await;
                    }
                }
                return;
            }
            if meta
                .get("workflow_waiting_siblings")
                .and_then(|v| v.as_bool())
                == Some(true)
                || meta
                    .get("workflow_version_conflict")
                    .and_then(|v| v.as_bool())
                    == Some(true)
            {
                // Parallel siblings still in flight or optimistic CC conflict — do nothing.
                return;
            }
        }

        let Some(node) = self.tasks.get_mut(task_id) else {
            return;
        };
        node.status = TaskStatus::Done;
        node.suspended_at = None;
        let parent_id = node.parent_task_id.clone();
        let run_id = node.run_id.clone();
        let is_root = parent_id.is_none();
        tracing::info!(
            target: "coordinator",
            task_id,
            is_root,
            answer_len = answer.len(),
            "handle_done"
        );

        if let Some(parent_id) = parent_id {
            // Atomically mark child done + record outcome in one transaction.
            if let Err(e) =
                crud::complete_child_done_txn(&self.db, &run_id, task_id, &parent_id, &answer).await
            {
                tracing::error!(
                    target: "coordinator",
                    task_id,
                    run_id = %run_id,
                    parent_id = %parent_id,
                    error = %e,
                    "failed to complete child done"
                );
            }

            self.emit_delegation_completed(&parent_id, task_id, true, Some(&answer), None)
                .await;
            self.record_child_result(&parent_id, task_id, ChildResult::Done(answer))
                .await;
            self.state.statuses.insert(run_id.clone(), RunStatus::Done);
            self.state.notify(&run_id);
            self.state.deregister_notifier(&run_id);
        } else {
            // Root task done.
            if let Err(e) = crud::update_run_done(&self.db, &run_id, &answer, metadata).await {
                tracing::error!(
                    target: "coordinator",
                    task_id,
                    run_id = %run_id,
                    error = %e,
                    "failed to persist run done status"
                );
            }
            self.state.statuses.insert(run_id.clone(), RunStatus::Done);
            self.state.notify(&run_id);
        }
    }

    async fn handle_failed(&mut self, task_id: &str, msg: String) {
        tracing::error!(target: "coordinator", task_id, error = %msg, "handle_failed");

        // Check if this child task can be retried or has fallbacks.
        if let Some(retry_action) = self.check_retry_or_fallback(task_id, &msg) {
            match retry_action {
                RetryAction::Retry {
                    delay,
                    attempt,
                    spec,
                    run_id,
                    parent_task_id,
                } => {
                    tracing::info!(
                        target: "coordinator",
                        task_id,
                        attempt,
                        delay_ms = delay.as_millis() as u64,
                        "retrying child task"
                    );
                    self.emit_retry_event(task_id, attempt, &msg).await;

                    // Persist retry state (include policy + spec for restart recovery).
                    let node = self.tasks.get(task_id);
                    let policy_json = node
                        .and_then(|n| n.policy.as_ref())
                        .and_then(|p| serde_json::to_value(p).ok());
                    let spec_json = node
                        .and_then(|n| n.original_spec.as_ref())
                        .and_then(|s| serde_json::to_value(s).ok());
                    self.persist_task_status(
                        &run_id,
                        "running",
                        Some(json!({
                            "attempt": attempt,
                            "retry_reason": &msg,
                            "policy": policy_json,
                            "original_spec": spec_json,
                        })),
                    )
                    .await;

                    // Backoff delay.
                    if !delay.is_zero() {
                        tokio::time::sleep(delay).await;
                    }

                    // Re-assign the same spec to a worker.
                    let assignment = TaskAssignment {
                        task_id: task_id.to_string(),
                        parent_task_id: parent_task_id.clone(),
                        run_id: run_id.clone(),
                        spec,
                        policy: None, // Policy stays on the TaskNode, not the assignment.
                    };
                    if let Err(e) = self.transport.assign(assignment).await {
                        tracing::error!(
                            target: "coordinator",
                            task_id,
                            error = %e,
                            "failed to assign retry, escalating to failure"
                        );
                        self.finalize_failed(task_id, format!("retry assignment failed: {e}"))
                            .await;
                    }
                    return;
                }
                RetryAction::Fallback {
                    new_spec,
                    fallback_index,
                    run_id,
                    parent_task_id,
                } => {
                    tracing::info!(
                        target: "coordinator",
                        task_id,
                        fallback_index,
                        "falling back to alternative target"
                    );
                    self.emit_fallback_event(task_id, fallback_index, &msg)
                        .await;

                    // Reset attempt counter for the fallback target.
                    if let Some(node) = self.tasks.get_mut(task_id) {
                        node.status = TaskStatus::Running;
                        node.attempt = 0;
                        node.fallback_index = fallback_index;
                        node.original_spec = Some(new_spec.clone());
                    }

                    // Persist fallback state (include policy + spec for restart recovery).
                    let node = self.tasks.get(task_id);
                    let policy_json = node
                        .and_then(|n| n.policy.as_ref())
                        .and_then(|p| serde_json::to_value(p).ok());
                    let spec_json = node
                        .and_then(|n| n.original_spec.as_ref())
                        .and_then(|s| serde_json::to_value(s).ok());
                    self.persist_task_status(
                        &run_id,
                        "running",
                        Some(json!({
                            "fallback_index": fallback_index,
                            "fallback_reason": &msg,
                            "policy": policy_json,
                            "original_spec": spec_json,
                        })),
                    )
                    .await;

                    let assignment = TaskAssignment {
                        task_id: task_id.to_string(),
                        parent_task_id,
                        run_id,
                        spec: new_spec,
                        policy: None,
                    };
                    if let Err(e) = self.transport.assign(assignment).await {
                        tracing::error!(
                            target: "coordinator",
                            task_id,
                            error = %e,
                            "failed to assign fallback, escalating to failure"
                        );
                        self.finalize_failed(task_id, format!("fallback assignment failed: {e}"))
                            .await;
                    }
                    return;
                }
            }
        }

        // No retry/fallback — finalize the failure.
        self.finalize_failed(task_id, msg).await;
    }

    /// Terminal failure: mark node failed and propagate to parent or root.
    async fn finalize_failed(&mut self, task_id: &str, msg: String) {
        let Some(node) = self.tasks.get_mut(task_id) else {
            return;
        };
        node.status = TaskStatus::Failed;
        node.suspended_at = None;
        let parent_id = node.parent_task_id.clone();
        let run_id = node.run_id.clone();

        if let Some(parent_id) = parent_id {
            // Atomically mark child failed + record outcome in one transaction.
            if let Err(e) = crud::complete_child_failed_txn(
                &self.db, &run_id, task_id, &parent_id, "failed", &msg,
            )
            .await
            {
                tracing::error!(
                    target: "coordinator",
                    task_id,
                    run_id = %run_id,
                    parent_id = %parent_id,
                    error = %e,
                    "failed to complete child failure"
                );
            }

            self.emit_delegation_completed(&parent_id, task_id, false, None, Some(&msg))
                .await;
            self.record_child_result(&parent_id, task_id, ChildResult::Failed(msg))
                .await;
            self.state
                .statuses
                .insert(run_id.clone(), RunStatus::Failed("child failed".into()));
            self.state.notify(&run_id);
            self.state.deregister_notifier(&run_id);
        } else {
            // Root task failed.
            if let Err(e) = crud::update_run_failed(&self.db, &run_id, &msg).await {
                tracing::error!(
                    target: "coordinator",
                    task_id,
                    run_id = %run_id,
                    error = %e,
                    "failed to persist run failed status"
                );
            }
            self.state
                .statuses
                .insert(run_id.clone(), RunStatus::Failed(msg));
            self.state.notify(&run_id);
        }
    }

    /// Terminal cancellation: mark node cancelled and propagate to parent.
    ///
    /// Unlike `handle_failed`, cancellation skips retry/fallback — the user
    /// intentionally stopped the task.
    async fn handle_cancelled(&mut self, task_id: &str) {
        tracing::info!(target: "coordinator", task_id, "handle_cancelled");

        let Some(node) = self.tasks.get_mut(task_id) else {
            return;
        };
        node.status = TaskStatus::Failed; // In-memory: reuse Failed variant
        node.suspended_at = None;
        let parent_id = node.parent_task_id.clone();
        let run_id = node.run_id.clone();

        let msg = "Operation cancelled";
        if let Some(parent_id) = parent_id {
            // Atomically mark child cancelled + record outcome in one transaction.
            crud::complete_child_failed_txn(
                &self.db,
                &run_id,
                task_id,
                &parent_id,
                "cancelled",
                msg,
            )
            .await
            .ok();

            self.emit_delegation_completed(&parent_id, task_id, false, None, Some(msg))
                .await;
            self.record_child_result(&parent_id, task_id, ChildResult::Failed(msg.into()))
                .await;
            self.state
                .statuses
                .insert(run_id.clone(), RunStatus::Cancelled);
            self.state.notify(&run_id);
            self.state.deregister_notifier(&run_id);
        } else {
            // Root task cancelled — single write.
            crud::transition_run(&self.db, &run_id, "cancelled", None, None, Some(msg))
                .await
                .ok();
            self.state
                .statuses
                .insert(run_id.clone(), RunStatus::Cancelled);
            self.state.notify(&run_id);
        }
    }
}
