//! Retry / fallback decisions and delegation event emitters.

use agentic_core::delegation::{DelegationTarget, TaskSpec};
use serde_json::{Value, json};

use crate::crud;

use super::{Coordinator, RetryAction, TaskStatus};

impl Coordinator {
    /// Check if a failed child task should be retried or fallen back.
    /// Returns `None` if the failure should propagate normally.
    pub(super) fn check_retry_or_fallback(
        &mut self,
        task_id: &str,
        error_msg: &str,
    ) -> Option<RetryAction> {
        let node = self.tasks.get_mut(task_id)?;

        // Only child tasks with a policy can be retried.
        let policy = node.policy.as_ref()?;
        let parent_task_id = node.parent_task_id.clone();
        let run_id = node.run_id.clone();

        // Check retry policy.
        if let Some(retry) = &policy.retry {
            if node.attempt < retry.max_retries {
                // Check retry_on filter.
                let should_retry = retry.retry_on.is_empty()
                    || retry.retry_on.iter().any(|p| error_msg.contains(p));

                if should_retry {
                    let delay = retry.backoff.delay_for_attempt(node.attempt);
                    node.attempt += 1;
                    node.status = TaskStatus::Running;
                    let spec = node.original_spec.clone()?;
                    let attempt = node.attempt;

                    return Some(RetryAction::Retry {
                        delay,
                        attempt,
                        spec,
                        run_id,
                        parent_task_id,
                    });
                }
            }
        }

        // Retries exhausted (or no retry policy) — check fallback targets.
        let fallback_targets = policy.fallback_targets.clone();
        if node.fallback_index < fallback_targets.len() {
            let fallback_target = &fallback_targets[node.fallback_index];
            let new_spec = match fallback_target {
                DelegationTarget::Agent { agent_id } => {
                    // Use the original question from the spec.
                    let question = match &node.original_spec {
                        Some(TaskSpec::Agent { question, .. }) => question.clone(),
                        _ => "retry".to_string(),
                    };
                    TaskSpec::Agent {
                        agent_id: agent_id.clone(),
                        question,
                    }
                }
                DelegationTarget::Workflow { workflow_ref } => TaskSpec::Workflow {
                    workflow_ref: workflow_ref.clone(),
                    variables: None,
                },
            };
            let fallback_index = node.fallback_index + 1;

            return Some(RetryAction::Fallback {
                new_spec,
                fallback_index,
                run_id,
                parent_task_id,
            });
        }

        None
    }

    pub(super) async fn emit_retry_event(&mut self, task_id: &str, attempt: u32, error: &str) {
        let Some(node) = self.tasks.get_mut(task_id) else {
            return;
        };
        // Emit on the parent's stream if this is a child task.
        let target_id = node.parent_task_id.clone().unwrap_or(task_id.to_string());
        if let Some(target_node) = self.tasks.get_mut(&target_id) {
            let seq = target_node.next_seq;
            target_node.next_seq += 1;
            let payload = json!({
                "child_task_id": task_id,
                "attempt": attempt,
                "error": error,
            });
            crud::insert_event(
                &self.db,
                &target_node.run_id,
                seq,
                "delegation_retry",
                &payload,
                self.attempt,
            )
            .await
            .ok();
            self.state.notify(&target_node.run_id);
        }
    }

    pub(super) async fn emit_fallback_event(
        &mut self,
        task_id: &str,
        fallback_index: usize,
        error: &str,
    ) {
        let Some(node) = self.tasks.get_mut(task_id) else {
            return;
        };
        let target_id = node.parent_task_id.clone().unwrap_or(task_id.to_string());
        if let Some(target_node) = self.tasks.get_mut(&target_id) {
            let seq = target_node.next_seq;
            target_node.next_seq += 1;
            let payload = json!({
                "child_task_id": task_id,
                "fallback_index": fallback_index,
                "previous_error": error,
            });
            crud::insert_event(
                &self.db,
                &target_node.run_id,
                seq,
                "delegation_fallback",
                &payload,
                self.attempt,
            )
            .await
            .ok();
            self.state.notify(&target_node.run_id);
        }
    }

    // ── Task tree persistence ────────────────────────────────────────────

    /// Persist a task_status transition to the database (best-effort).
    pub(super) async fn persist_task_status(
        &self,
        run_id: &str,
        task_status: &str,
        task_metadata: Option<Value>,
    ) {
        if let Err(e) = crud::update_task_status(&self.db, run_id, task_status, task_metadata).await
        {
            tracing::error!(
                target: "coordinator",
                run_id,
                task_status,
                error = %e,
                "failed to persist task_status"
            );
        }
    }

    // ── Event emission helpers ──────────────────────────────────────────

    pub(super) async fn emit_delegation_started(
        &mut self,
        parent_id: &str,
        child_id: &str,
        target: &DelegationTarget,
        request: &str,
    ) {
        let target_str = match target {
            DelegationTarget::Agent { agent_id } => format!("agent:{agent_id}"),
            DelegationTarget::Workflow { workflow_ref } => format!("workflow:{workflow_ref}"),
        };

        if let Some(node) = self.tasks.get_mut(parent_id) {
            let seq = node.next_seq;
            node.next_seq += 1;
            let payload = json!({
                "event_type": "delegation_started",
                "child_task_id": child_id,
                "target": target_str,
                "request": request,
            });
            if let Err(e) = crud::insert_event(
                &self.db,
                &node.run_id,
                seq,
                "delegation_started",
                &payload,
                self.attempt,
            )
            .await
            {
                tracing::error!(
                    target: "coordinator",
                    parent_id,
                    run_id = %node.run_id,
                    error = %e,
                    "failed to persist delegation_started event"
                );
            }
            self.state.notify(&node.run_id);
        }
    }

    pub(super) async fn emit_delegation_completed(
        &mut self,
        parent_id: &str,
        child_id: &str,
        success: bool,
        answer: Option<&str>,
        error: Option<&str>,
    ) {
        if let Some(node) = self.tasks.get_mut(parent_id) {
            let seq = node.next_seq;
            node.next_seq += 1;
            let payload = json!({
                "event_type": "delegation_completed",
                "child_task_id": child_id,
                "success": success,
                "answer": answer,
                "error": error,
            });
            if let Err(e) = crud::insert_event(
                &self.db,
                &node.run_id,
                seq,
                "delegation_completed",
                &payload,
                self.attempt,
            )
            .await
            {
                tracing::error!(
                    target: "coordinator",
                    parent_id,
                    run_id = %node.run_id,
                    error = %e,
                    "failed to persist delegation_completed event"
                );
            }
            self.state.notify(&node.run_id);
        }
    }
}
