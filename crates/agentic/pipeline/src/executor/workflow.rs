//! Workflow-related executor methods + the workflow decision runner.

use std::sync::Arc;

use agentic_core::delegation::{TaskOutcome, TaskSpec};
use agentic_runtime::worker::ExecutingTask;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::PipelineTaskExecutor;

impl PipelineTaskExecutor {
    pub(super) async fn execute_workflow_step(
        &self,
        step_config: Value,
        render_context: Value,
        workflow_context: Value,
    ) -> Result<ExecutingTask, String> {
        let (event_tx, event_rx) = mpsc::channel::<(String, Value)>(256);
        let (outcome_tx, outcome_rx) = mpsc::channel::<TaskOutcome>(4);
        let cancel = CancellationToken::new();

        let workspace: Arc<dyn agentic_workflow::WorkspaceContext> = self.platform.clone();
        tokio::spawn(async move {
            let result = agentic_workflow::run_workflow_step(
                workspace.as_ref(),
                step_config,
                render_context,
                workflow_context,
            )
            .await;
            match result {
                Ok(output) => {
                    let _ = outcome_tx
                        .send(TaskOutcome::Done {
                            answer: output,
                            metadata: None,
                        })
                        .await;
                }
                Err(e) => {
                    let _ = outcome_tx.send(TaskOutcome::Failed(e)).await;
                }
            }
            drop(event_tx);
        });

        Ok(ExecutingTask {
            events: event_rx,
            outcomes: outcome_rx,
            cancel,
            answers: None,
        })
    }

    /// Seed a workflow run and chain to the first `WorkflowDecision`.
    ///
    /// This is the entry point for `TaskSpec::Workflow`. It:
    /// 1. Loads and parses the workflow YAML.
    /// 2. Inserts `agentic_workflow_state` into the DB.
    /// 3. Returns `Done { workflow_continue: true }` immediately.
    ///
    /// The coordinator's `handle_done` detects `workflow_continue` and
    /// enqueues `TaskSpec::WorkflowDecision` — the stateless decider then
    /// drives the workflow from DB state. No long-lived channels survive a crash.
    pub(super) async fn execute_workflow(
        &self,
        run_id: &str,
        workflow_ref: &str,
        variables: Option<Value>,
    ) -> Result<ExecutingTask, String> {
        let workspace: Arc<dyn agentic_workflow::WorkspaceContext> = self.platform.clone();
        let workflow_context = serde_json::json!({
            "workspace_path": workspace.workspace_path().to_string_lossy(),
        });

        // Load and parse workflow YAML.
        let yaml = workspace
            .resolve_workflow_yaml(workflow_ref)
            .await
            .map_err(|e| format!("failed to load workflow: {e}"))?;
        let workflow_config: agentic_workflow::WorkflowConfig =
            serde_yaml::from_str(&yaml).map_err(|e| format!("failed to parse workflow: {e}"))?;

        let yaml_hash = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            yaml.hash(&mut h);
            format!("{:x}", h.finish())
        };

        // Seed durable workflow state.
        let initial_state = agentic_workflow::extension::WorkflowRunState {
            run_id: run_id.to_string(),
            workflow: workflow_config,
            workflow_yaml_hash: yaml_hash,
            workflow_context,
            variables,
            trace_id: format!("wf-{}", uuid::Uuid::new_v4()),
            current_step: 0,
            results: std::collections::HashMap::new(),
            render_context: serde_json::json!({}),
            pending_children: std::collections::HashMap::new(),
            decision_version: 0,
        };
        agentic_workflow::extension::insert_workflow_state(&self.db, &initial_state)
            .await
            .map_err(|e| format!("failed to seed workflow state: {e}"))?;

        // Immediately signal the coordinator to chain the first WorkflowDecision.
        let (_, event_rx) = mpsc::channel::<(String, Value)>(1);
        let (outcome_tx, outcome_rx) = mpsc::channel::<TaskOutcome>(1);
        let _ = outcome_tx
            .send(TaskOutcome::Done {
                answer: String::new(),
                metadata: Some(serde_json::json!({"workflow_continue": true})),
            })
            .await;

        Ok(ExecutingTask {
            events: event_rx,
            outcomes: outcome_rx,
            cancel: CancellationToken::new(),
            answers: None,
        })
    }

    /// Execute a stateless `WorkflowDecision` task.
    ///
    /// Loads `agentic_workflow_state`, calls `WorkflowDecider::decide`, and
    /// atomically commits the resulting state patch, emitted events, and any
    /// terminal queue/run transition via
    /// [`agentic_workflow::extension::commit_decision`]. Only after that commit
    /// succeeds does it drive the decision as an [`ExecutingTask`] — the
    /// outcome channel merely signals the coordinator; durable state is
    /// already on disk.
    ///
    /// On a `decision_version` mismatch the commit rolls back and this
    /// function returns a no-op task (`workflow_version_conflict`), exactly as
    /// before. Because the pre-refactor flow wrote state, events, and queue
    /// status as three independent statements, a silent failure between them
    /// could strand a workflow with advanced state but no follow-up events or
    /// queue entries — this rewrite closes that gap.
    pub(super) async fn execute_workflow_decision(
        &self,
        run_id: &str,
        pending_child_answer: Option<agentic_core::delegation::ChildCompletion>,
    ) -> Result<ExecutingTask, String> {
        let state = agentic_workflow::extension::load_workflow_state(&self.db, run_id)
            .await
            .map_err(|e| format!("load workflow state: {e}"))?
            .ok_or_else(|| format!("workflow state not found for run {run_id}"))?;

        let expected_version = state.decision_version;
        let decider = agentic_workflow::WorkflowDecider::new(None);
        let (new_state, decision) = decider.decide(state, pending_child_answer).await;

        let events = decision_events(&decision).to_vec();
        let terminal = decision_terminal(&decision);

        // WorkflowDecision tasks use their run_id as their queue task_id.
        let decision_task_id = run_id.to_string();
        let outcome = agentic_workflow::extension::commit_decision(
            &self.db,
            agentic_workflow::extension::DecisionCommit {
                run_id: run_id.to_string(),
                decision_task_id,
                expected_version,
                new_state,
                events,
                attempt: 0,
                terminal,
            },
        )
        .await
        .map_err(|e| format!("commit_decision: {e}"))?;

        if matches!(
            outcome,
            agentic_workflow::extension::CommitOutcome::VersionConflict
        ) {
            tracing::debug!(run_id = %run_id, "WorkflowDecision: version conflict — discarding");
            return Ok(noop_version_conflict_task().await);
        }

        run_decision_task(decision)
    }
}

async fn noop_version_conflict_task() -> ExecutingTask {
    let (_, event_rx) = mpsc::channel::<(String, Value)>(1);
    let (outcome_tx, outcome_rx) = mpsc::channel::<TaskOutcome>(1);
    let _ = outcome_tx
        .send(TaskOutcome::Done {
            answer: String::new(),
            metadata: Some(serde_json::json!({"workflow_version_conflict": true})),
        })
        .await;
    ExecutingTask {
        events: event_rx,
        outcomes: outcome_rx,
        cancel: CancellationToken::new(),
        answers: None,
    }
}

fn decision_events(d: &agentic_workflow::WorkflowDecision) -> &[(String, Value)] {
    use agentic_workflow::WorkflowDecision as D;
    match d {
        D::Complete { emitted_events, .. }
        | D::StepExecutedInline { emitted_events, .. }
        | D::DelegateStep { emitted_events, .. }
        | D::DelegateParallel { emitted_events, .. } => emitted_events.as_slice(),
        D::WaitForMoreChildren | D::Fail(_) => &[],
    }
}

/// Map a [`WorkflowDecision`] variant onto the terminal behavior
/// [`commit_decision`] applies to the decision task's queue row and the
/// workflow run row.
///
/// Only `Complete` and `Fail` flip the run + queue rows to `done`/`failed`
/// inside the commit; every other variant leaves them alone because the
/// worker's downstream `Suspended`/`Done` outcome still has to flow through
/// the coordinator to schedule the next activity.
fn decision_terminal(
    d: &agentic_workflow::WorkflowDecision,
) -> agentic_workflow::extension::DecisionTerminal {
    use agentic_workflow::WorkflowDecision as D;
    use agentic_workflow::extension::DecisionTerminal;
    match d {
        D::Complete { final_answer, .. } => DecisionTerminal::CompleteWorkflow {
            final_answer: final_answer.clone(),
        },
        D::Fail(e) => DecisionTerminal::FailWorkflow { error: e.clone() },
        _ => DecisionTerminal::Continuing,
    }
}

/// Convert a `WorkflowDecision` into an `ExecutingTask` that emits the
/// appropriate events and outcome on its channels, then exits.
pub fn run_decision_task(
    decision: agentic_workflow::WorkflowDecision,
) -> Result<ExecutingTask, String> {
    use agentic_core::delegation::SuspendReason;
    use agentic_core::human_input::SuspendedRunData;
    use agentic_workflow::WorkflowDecision as D;

    let (event_tx, event_rx) = mpsc::channel::<(String, Value)>(32);
    let (outcome_tx, outcome_rx) = mpsc::channel::<TaskOutcome>(4);
    let cancel = CancellationToken::new();

    tokio::spawn(async move {
        match decision {
            D::Complete {
                final_answer,
                emitted_events,
            } => {
                for (et, p) in emitted_events {
                    let _ = event_tx.send((et, p)).await;
                }
                let _ = outcome_tx
                    .send(TaskOutcome::Done {
                        answer: final_answer,
                        metadata: None,
                    })
                    .await;
            }

            D::StepExecutedInline { emitted_events, .. } => {
                for (et, p) in emitted_events {
                    let _ = event_tx.send((et, p)).await;
                }
                // Chain to next decision immediately.
                let _ = outcome_tx
                    .send(TaskOutcome::Done {
                        answer: String::new(),
                        metadata: Some(serde_json::json!({"workflow_continue": true})),
                    })
                    .await;
            }

            D::WaitForMoreChildren => {
                let _ = outcome_tx
                    .send(TaskOutcome::Done {
                        answer: String::new(),
                        metadata: Some(serde_json::json!({"workflow_waiting_siblings": true})),
                    })
                    .await;
            }

            D::DelegateStep {
                step_index,
                step_name,
                spec,
                trace_id,
                emitted_events,
            } => {
                for (et, p) in emitted_events {
                    let _ = event_tx.send((et, p)).await;
                }
                let (target, request, context) = spec_to_delegation_parts(&spec, &step_name);
                let resume_data = SuspendedRunData {
                    from_state: "workflow_decision".to_string(),
                    original_input: step_name.clone(),
                    trace_id,
                    stage_data: serde_json::json!({"step_name": step_name, "step_index": step_index}),
                    question: format!("Executing step: {step_name}"),
                    suggestions: vec![],
                };
                let _ = outcome_tx
                    .send(TaskOutcome::Suspended {
                        reason: SuspendReason::Delegation {
                            target,
                            request,
                            context,
                            policy: None,
                        },
                        resume_data,
                        trace_id: String::new(),
                    })
                    .await;
            }

            D::DelegateParallel {
                step_index,
                step_name,
                items,
                failure_policy,
                trace_id,
                emitted_events,
            } => {
                for (et, p) in emitted_events {
                    let _ = event_tx.send((et, p)).await;
                }
                let resume_data = SuspendedRunData {
                    from_state: "workflow_decision".to_string(),
                    original_input: step_name.clone(),
                    trace_id,
                    stage_data: serde_json::json!({"step_name": step_name, "step_index": step_index}),
                    question: format!("Executing step: {step_name}"),
                    suggestions: vec![],
                };
                let _ = outcome_tx
                    .send(TaskOutcome::Suspended {
                        reason: SuspendReason::ParallelDelegation {
                            targets: items,
                            failure_policy,
                        },
                        resume_data,
                        trace_id: String::new(),
                    })
                    .await;
            }

            D::Fail(e) => {
                let _ = outcome_tx.send(TaskOutcome::Failed(e)).await;
            }
        }
    });

    Ok(ExecutingTask {
        events: event_rx,
        outcomes: outcome_rx,
        cancel,
        answers: None,
    })
}

/// Extract delegation target, request, and context from a `TaskSpec`.
fn spec_to_delegation_parts(
    spec: &TaskSpec,
    step_name: &str,
) -> (
    agentic_core::delegation::DelegationTarget,
    String,
    serde_json::Value,
) {
    use agentic_core::delegation::DelegationTarget;
    match spec {
        TaskSpec::Agent { agent_id, question } => (
            DelegationTarget::Agent {
                agent_id: agent_id.clone(),
            },
            question.clone(),
            serde_json::json!({}),
        ),
        TaskSpec::Workflow {
            workflow_ref,
            variables,
        } => (
            DelegationTarget::Workflow {
                workflow_ref: workflow_ref.clone(),
            },
            format!("Execute sub-workflow: {workflow_ref}"),
            variables.clone().unwrap_or(serde_json::json!({})),
        ),
        TaskSpec::WorkflowStep {
            step_config,
            render_context,
            workflow_context,
        } => (
            DelegationTarget::Workflow {
                workflow_ref: "__workflow_step__".to_string(),
            },
            step_name.to_string(),
            serde_json::json!({
                "step_config": step_config,
                "render_context": render_context,
                "workflow_context": workflow_context,
            }),
        ),
        _ => (
            DelegationTarget::Workflow {
                workflow_ref: "__unknown__".to_string(),
            },
            step_name.to_string(),
            serde_json::json!({}),
        ),
    }
}
