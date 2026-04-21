//! Handlers for `TaskOutcome::Suspended` and human answers.

use std::collections::HashMap;

use agentic_core::delegation::{
    DelegationTarget, FanoutFailurePolicy, SuspendReason, TaskAssignment, TaskSpec,
};
use serde_json::json;

use crate::crud;
use crate::state::RunStatus;

use super::{ChildResult, Coordinator, TaskNode, TaskStatus, source_type_for_spec};

impl Coordinator {
    pub(super) async fn handle_suspended(
        &mut self,
        task_id: &str,
        reason: SuspendReason,
        resume_data: agentic_core::human_input::SuspendedRunData,
        _trace_id: String,
    ) {
        // Store suspend data (drop mutable borrow before calling other methods).
        {
            let Some(node) = self.tasks.get_mut(task_id) else {
                return;
            };
            node.suspend_data = Some(resume_data.clone());
            node.suspended_at = Some(tokio::time::Instant::now());
        }

        match reason {
            SuspendReason::HumanInput { questions } => {
                tracing::info!(
                    target: "coordinator",
                    task_id,
                    question_count = questions.len(),
                    "suspended for human input"
                );
                let run_id = {
                    let node = self.tasks.get_mut(task_id).unwrap();
                    node.status = TaskStatus::SuspendedHuman;
                    node.run_id.clone()
                };

                // Persist status + suspension data atomically in one transaction.
                let combined_prompt = questions
                    .iter()
                    .map(|q| q.prompt.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                let first_suggestions = questions
                    .first()
                    .map(|q| q.suggestions.clone())
                    .unwrap_or_default();

                if let Err(e) = crud::suspend_with_data_txn(
                    &self.db,
                    &run_id,
                    "awaiting_input",
                    None,
                    &combined_prompt,
                    &first_suggestions,
                    &resume_data,
                )
                .await
                {
                    tracing::error!(
                        target: "coordinator",
                        task_id,
                        run_id = %run_id,
                        error = %e,
                        "failed to persist suspension"
                    );
                }
                self.state.statuses.insert(
                    run_id,
                    RunStatus::Suspended {
                        questions: questions.clone(),
                    },
                );
            }

            SuspendReason::Delegation {
                target,
                request,
                context,
                policy,
            } => {
                // Spawn a child task.
                self.child_counter += 1;
                let child_id = format!("{task_id}.{}", self.child_counter);
                tracing::info!(
                    target: "coordinator",
                    task_id,
                    child_id = %child_id,
                    target = ?target,
                    "suspended for delegation, spawning child task"
                );
                let child_run_id = child_id.clone();
                let task_id_owned = task_id.to_string();

                // Emit delegation_started on parent's stream.
                self.emit_delegation_started(&task_id_owned, &child_id, &target, &request)
                    .await;

                // Update parent status (after emit to avoid borrow conflict).
                let run_id = {
                    let node = self.tasks.get_mut(task_id).unwrap();
                    node.status = TaskStatus::WaitingOnChildren {
                        child_task_ids: vec![child_id.clone()],
                        completed: HashMap::new(),
                        failure_policy: FanoutFailurePolicy::FailFast,
                    };
                    node.run_id.clone()
                };

                // Persist status + suspension data atomically. Use "delegating"
                // (not "awaiting_input") so crash recovery correctly identifies
                // this as WaitingOnChildren via from_db().
                if let Err(e) = crud::suspend_with_data_txn(
                    &self.db,
                    &run_id,
                    "delegating",
                    Some(json!({
                        "child_task_ids": [&child_id],
                        "completed": {},
                        "failure_policy": "fail_fast",
                    })),
                    &format!("Delegation to {request}"),
                    &[],
                    &resume_data,
                )
                .await
                {
                    tracing::error!(
                        target: "coordinator",
                        task_id,
                        run_id = %run_id,
                        error = %e,
                        "failed to persist delegation state"
                    );
                }
                // Save request before it's moved into child_spec.
                let request_for_child = request.clone();
                let run_id_for_child = run_id.clone();

                self.state
                    .statuses
                    .insert(run_id, RunStatus::Suspended { questions: vec![] });

                let child_spec = match target {
                    DelegationTarget::Agent { agent_id } => TaskSpec::Agent {
                        agent_id,
                        question: request,
                    },
                    DelegationTarget::Workflow { workflow_ref } => {
                        // Check if the context carries a WorkflowStep spec
                        // (the orchestrator uses this for individual step delegation).
                        if context.get("step_config").is_some() {
                            TaskSpec::WorkflowStep {
                                step_config: context
                                    .get("step_config")
                                    .cloned()
                                    .unwrap_or_default(),
                                render_context: context
                                    .get("render_context")
                                    .cloned()
                                    .unwrap_or_default(),
                                workflow_context: context
                                    .get("workflow_context")
                                    .cloned()
                                    .unwrap_or_default(),
                            }
                        } else {
                            TaskSpec::Workflow {
                                workflow_ref,
                                variables: if context.is_null() {
                                    None
                                } else {
                                    Some(context)
                                },
                            }
                        }
                    }
                };

                // Register child in task tree (store spec + policy for retries).
                self.tasks.insert(
                    child_id.clone(),
                    TaskNode {
                        run_id: child_run_id.clone(),
                        parent_task_id: Some(task_id_owned.clone()),
                        status: TaskStatus::Running,
                        suspend_data: None,
                        next_seq: 0,
                        suspended_at: None,
                        original_spec: Some(child_spec.clone()),
                        policy,
                        attempt: 0,
                        fallback_index: 0,
                    },
                );

                // Derive source_type from the child's TaskSpec.
                let child_source_type = source_type_for_spec(&child_spec);

                // Build task_metadata with policy/spec for restart recovery.
                let child_task_metadata = {
                    let node = self.tasks.get(&child_id);
                    let policy_json = node
                        .and_then(|n| n.policy.as_ref())
                        .and_then(|p| serde_json::to_value(p).ok());
                    let spec_json = node
                        .and_then(|n| n.original_spec.as_ref())
                        .and_then(|s| serde_json::to_value(s).ok());
                    if policy_json.is_some() || spec_json.is_some() {
                        Some(json!({
                            "policy": policy_json,
                            "original_spec": spec_json,
                        }))
                    } else {
                        None
                    }
                };

                // Persist child run with parent reference + metadata in one INSERT.
                if let Err(e) = crud::insert_child_run(
                    &self.db,
                    &child_run_id,
                    &run_id_for_child,
                    &request_for_child,
                    child_source_type,
                    self.attempt,
                    child_task_metadata,
                )
                .await
                {
                    tracing::error!(
                        target: "coordinator",
                        task_id,
                        child_run_id = %child_run_id,
                        error = %e,
                        "failed to persist child run"
                    );
                }

                // Register a notifier so SSE subscribers can stream child events.
                self.state.register_notifier(&child_run_id);

                // Assign child to a worker.
                let assignment = TaskAssignment {
                    task_id: child_id.clone(),
                    parent_task_id: Some(task_id_owned.clone()),
                    run_id: child_run_id,
                    spec: child_spec,
                    policy: None,
                };

                tracing::info!(target: "coordinator", task_id, child_task_id = %assignment.task_id, "assigning child task to worker");
                if let Err(e) = self.transport.assign(assignment).await {
                    tracing::error!(target: "coordinator", task_id, error = %e, "failed to assign child task, failing it");
                    // Mark the child as failed and resume the parent so it
                    // doesn't hang forever waiting on a child that never started.
                    if let Some(child_node) = self.tasks.get_mut(&child_id) {
                        child_node.status = TaskStatus::Failed;
                    }
                    self.emit_delegation_completed(
                        &task_id_owned,
                        &child_id,
                        false,
                        None,
                        Some(&format!("failed to assign child task: {e}")),
                    )
                    .await;
                    self.resume_parent(
                        &task_id_owned,
                        format!("Delegation failed: could not assign child task: {e}"),
                    )
                    .await;
                } else {
                    tracing::info!(target: "coordinator", task_id, "child task assigned successfully");
                }
            }

            SuspendReason::ParallelDelegation {
                targets,
                failure_policy,
            } => {
                let task_id_owned = task_id.to_string();
                let child_count = targets.len();
                tracing::info!(
                    target: "coordinator",
                    task_id,
                    child_count,
                    "suspended for parallel delegation, spawning children"
                );

                // Generate child IDs and specs.
                let mut child_ids = Vec::with_capacity(child_count);
                let mut child_specs = Vec::with_capacity(child_count);
                let mut child_requests = Vec::with_capacity(child_count);

                for item in &targets {
                    self.child_counter += 1;
                    let child_id = format!("{task_id}.{}", self.child_counter);
                    child_ids.push(child_id);

                    let spec = match &item.target {
                        DelegationTarget::Agent { agent_id } => TaskSpec::Agent {
                            agent_id: agent_id.clone(),
                            question: item.request.clone(),
                        },
                        DelegationTarget::Workflow { workflow_ref } => TaskSpec::Workflow {
                            workflow_ref: workflow_ref.clone(),
                            variables: if item.context.is_null() {
                                None
                            } else {
                                Some(item.context.clone())
                            },
                        },
                    };
                    child_specs.push(spec);
                    child_requests.push(item.request.clone());
                }

                // Emit delegation_started for each child on parent's stream.
                for (i, item) in targets.iter().enumerate() {
                    self.emit_delegation_started(
                        &task_id_owned,
                        &child_ids[i],
                        &item.target,
                        &item.request,
                    )
                    .await;
                }

                // Update parent status.
                let run_id = {
                    let node = self.tasks.get_mut(task_id).unwrap();
                    node.status = TaskStatus::WaitingOnChildren {
                        child_task_ids: child_ids.clone(),
                        completed: HashMap::new(),
                        failure_policy: failure_policy.clone(),
                    };
                    node.run_id.clone()
                };

                // Persist parent status + suspension data atomically.
                let child_ids_json: Vec<&str> = child_ids.iter().map(|s| s.as_str()).collect();
                if let Err(e) = crud::suspend_with_data_txn(
                    &self.db,
                    &run_id,
                    "delegating",
                    Some(json!({
                        "child_task_ids": child_ids_json,
                        "completed": {},
                        "failure_policy": serde_json::to_value(&failure_policy).unwrap_or_default(),
                    })),
                    "Parallel delegation",
                    &[],
                    &resume_data,
                )
                .await
                {
                    tracing::error!(
                        target: "coordinator",
                        task_id,
                        run_id = %run_id,
                        error = %e,
                        "failed to persist parallel delegation state"
                    );
                }

                self.state
                    .statuses
                    .insert(run_id.clone(), RunStatus::Suspended { questions: vec![] });

                // Register and assign all children.
                for (i, child_id) in child_ids.iter().enumerate() {
                    self.tasks.insert(
                        child_id.clone(),
                        TaskNode {
                            run_id: child_id.clone(),
                            parent_task_id: Some(task_id_owned.clone()),
                            status: TaskStatus::Running,
                            suspend_data: None,
                            next_seq: 0,
                            suspended_at: None,
                            original_spec: Some(child_specs[i].clone()),
                            policy: None,
                            attempt: 0,
                            fallback_index: 0,
                        },
                    );

                    // Persist child run with parent reference + spec for recovery.
                    let child_source_type = source_type_for_spec(&child_specs[i]);
                    let child_spec_json = serde_json::to_value(&child_specs[i]).ok();
                    let child_meta = child_spec_json.map(|s| json!({ "original_spec": s }));
                    if let Err(e) = crud::insert_child_run(
                        &self.db,
                        child_id,
                        &run_id,
                        &child_requests[i],
                        child_source_type,
                        self.attempt,
                        child_meta,
                    )
                    .await
                    {
                        tracing::error!(
                            target: "coordinator",
                            task_id,
                            child_id,
                            error = %e,
                            "failed to persist child run"
                        );
                    }

                    // Register a notifier so SSE subscribers can stream child events.
                    self.state.register_notifier(child_id);

                    let assignment = TaskAssignment {
                        task_id: child_id.clone(),
                        parent_task_id: Some(task_id_owned.clone()),
                        run_id: child_id.clone(),
                        spec: child_specs[i].clone(),
                        policy: None,
                    };

                    if let Err(e) = self.transport.assign(assignment).await {
                        tracing::error!(
                            target: "coordinator",
                            task_id,
                            child_id,
                            error = %e,
                            "failed to assign child task"
                        );
                        // Mark this child as failed immediately.
                        if let Some(child_node) = self.tasks.get_mut(child_id) {
                            child_node.status = TaskStatus::Failed;
                        }
                        self.emit_delegation_completed(
                            &task_id_owned,
                            child_id,
                            false,
                            None,
                            Some(&format!("failed to assign: {e}")),
                        )
                        .await;
                        self.record_child_result(
                            &task_id_owned,
                            child_id,
                            ChildResult::Failed(format!("failed to assign: {e}")),
                        )
                        .await;
                    }
                }
            }
        }
    }

    pub(super) async fn handle_human_answer(&mut self, task_id: &str, answer: &str) {
        let Some(node) = self.tasks.get(task_id) else {
            tracing::warn!(target: "coordinator", task_id, "human answer for unknown task");
            return;
        };

        match &node.status {
            TaskStatus::SuspendedHuman => {
                tracing::info!(target: "coordinator", task_id, "human answer: direct answer");
                // resume_parent handles: status update, input_resolved
                // event, DB update, and TaskSpec::Resume assignment.
                self.resume_parent(task_id, answer.to_string()).await;
            }
            TaskStatus::WaitingOnChildren { child_task_ids, .. } => {
                let child_ids = child_task_ids.clone();
                tracing::info!(
                    target: "coordinator",
                    task_id,
                    child_count = child_ids.len(),
                    "human answer: override (cancelling all children)"
                );

                // Human override — cancel all children and resume parent.
                for child_id in &child_ids {
                    self.transport.cancel(child_id).await.ok();
                    if let Some(child_node) = self.tasks.get_mut(child_id) {
                        child_node.status = TaskStatus::Failed;
                        child_node.suspended_at = None;
                    }
                    self.emit_delegation_completed(
                        task_id,
                        child_id,
                        false,
                        None,
                        Some("overridden by human answer"),
                    )
                    .await;
                }

                self.resume_parent(task_id, answer.to_string()).await;
            }
            _ => {
                tracing::warn!(target: "coordinator", task_id, "human answer for non-suspended task");
            }
        }
    }
}
