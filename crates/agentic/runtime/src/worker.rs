//! Worker: pulls task assignments from the transport and executes them.
//!
//! The worker is domain-agnostic — it delegates actual pipeline/workflow
//! execution to a [`TaskExecutor`] injected by the pipeline layer.

use std::sync::Arc;

use agentic_core::delegation::{TaskAssignment, TaskOutcome};
use agentic_core::transport::{WorkerMessage, WorkerTransport};
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

// ── ExecutingTask ────────────────────────────────────────────────────────────

/// Handle to a running task, returned by [`TaskExecutor::execute`].
///
/// Events are pre-serialized `(event_type, payload)` pairs — the worker
/// forwards them to the coordinator without inspecting domain-specific content.
pub struct ExecutingTask {
    /// Pre-serialized events from the running task.
    pub events: mpsc::Receiver<(String, Value)>,
    /// Outcomes from the task. A pipeline may produce multiple outcomes
    /// (e.g. `Suspended` followed by `Done` after resume).
    pub outcomes: mpsc::Receiver<TaskOutcome>,
    /// Cancel the task.
    pub cancel: CancellationToken,
    /// Send an answer to resume a suspended task.
    ///
    /// For pipelines, this feeds the orchestrator's internal suspend/resume
    /// loop.  For workflows and other tasks that don't suspend, this is `None`.
    pub answers: Option<mpsc::Sender<String>>,
}

// ── TaskExecutor ─────────────────────────────────────────────────────────────

/// Knows how to start pipelines and workflows.
///
/// Implemented by the pipeline layer (`agentic-pipeline`), which has access to
/// all domain crates. The runtime only sees this trait.
#[async_trait]
pub trait TaskExecutor: Send + Sync + 'static {
    /// Start executing a task assignment, returning a handle to the running task.
    async fn execute(&self, assignment: TaskAssignment) -> Result<ExecutingTask, String>;

    /// Resume a task from saved state after a server restart.
    ///
    /// Called by the recovery pipeline for tasks that were running or
    /// suspended when the server crashed. The default implementation
    /// returns an error — implementors that support restart-resume
    /// should override this.
    async fn resume_from_state(
        &self,
        _run: &crate::entity::run::Model,
        _suspend_data: Option<agentic_core::human_input::SuspendedRunData>,
    ) -> Result<ExecutingTask, String> {
        Err("resume_from_state not supported".to_string())
    }
}

// ── Worker ───────────────────────────────────────────────────────────────────

/// A pull-based worker that receives assignments and executes them.
pub struct Worker {
    transport: Arc<dyn WorkerTransport>,
    executor: Arc<dyn TaskExecutor>,
}

impl Worker {
    pub fn new(transport: Arc<dyn WorkerTransport>, executor: Arc<dyn TaskExecutor>) -> Self {
        Self {
            transport,
            executor,
        }
    }

    /// Run the worker loop. Pulls assignments, executes them, and forwards
    /// events and outcomes back to the coordinator via the transport.
    ///
    /// Returns when the transport's assignment channel is closed.
    pub async fn run(&self) {
        tracing::info!(target: "worker", "worker run loop started");
        while let Some(assignment) = self.transport.recv_assignment().await {
            let task_id = assignment.task_id.clone();
            let transport = Arc::clone(&self.transport);
            let executor = Arc::clone(&self.executor);

            tokio::spawn(async move {
                Self::handle_task(transport, executor, task_id, assignment).await;
            });
        }
        tracing::info!(target: "worker", "assignment channel closed, shutting down");
    }

    async fn handle_task(
        transport: Arc<dyn WorkerTransport>,
        executor: Arc<dyn TaskExecutor>,
        task_id: String,
        assignment: TaskAssignment,
    ) {
        tracing::info!(target: "worker", task_id = %task_id, spec_type = ?assignment.spec, "received task assignment");

        // Get the cancellation token for this task from the transport.
        let cancel_token = transport.cancellation_token(&task_id);

        // Execute the task.
        let executing = match executor.execute(assignment).await {
            Ok(e) => e,
            Err(msg) => {
                tracing::error!(target: "worker", task_id = %task_id, error = %msg, "executor failed to start task");
                let _ = transport
                    .send(WorkerMessage::Outcome {
                        task_id,
                        outcome: TaskOutcome::Failed(msg),
                    })
                    .await;
                return;
            }
        };

        // Spawn heartbeat loop — DurableTransport updates DB, LocalTransport no-ops.
        let heartbeat_cancel =
            transport.spawn_heartbeat(&task_id, std::time::Duration::from_secs(15));

        // Forward cancellation from transport to the executing task.
        let task_cancel = executing.cancel.clone();
        let cancel_fwd = tokio::spawn({
            let cancel_token = cancel_token.clone();
            let task_id = task_id.clone();
            async move {
                cancel_token.cancelled().await;
                tracing::info!(target: "worker", task_id = %task_id, "cancellation forwarded to executing task");
                task_cancel.cancel();
            }
        });

        // Forward events to coordinator.
        let event_fwd = {
            let transport = Arc::clone(&transport);
            let task_id = task_id.clone();
            tokio::spawn(async move {
                let mut events = executing.events;
                while let Some((event_type, payload)) = events.recv().await {
                    if transport
                        .send(WorkerMessage::Event {
                            task_id: task_id.clone(),
                            event_type,
                            payload,
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            })
        };

        // Forward all outcomes (pipeline may produce Suspended then Done).
        let mut outcomes = executing.outcomes;
        while let Some(outcome) = outcomes.recv().await {
            let is_terminal = matches!(
                outcome,
                TaskOutcome::Done { .. } | TaskOutcome::Failed(_) | TaskOutcome::Cancelled
            );
            let outcome_type = match &outcome {
                TaskOutcome::Done { .. } => "Done",
                TaskOutcome::Suspended { .. } => "Suspended",
                TaskOutcome::Failed(_) => "Failed",
                TaskOutcome::Cancelled => "Cancelled",
            };
            tracing::info!(
                target: "worker",
                task_id = %task_id,
                outcome_type,
                is_terminal,
                "forwarding outcome"
            );
            let _ = transport
                .send(WorkerMessage::Outcome {
                    task_id: task_id.clone(),
                    outcome,
                })
                .await;
            if is_terminal {
                break;
            }
        }

        // Clean up.
        heartbeat_cancel.cancel();
        event_fwd.await.ok();
        cancel_fwd.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::LocalTransport;
    use agentic_core::delegation::TaskSpec;
    use agentic_core::transport::{CoordinatorTransport, WorkerTransport};
    use serde_json::json;

    /// Helper: coordinator-side trait reference.
    fn coord(t: &LocalTransport) -> &dyn CoordinatorTransport {
        t
    }

    /// Mock executor that emits a few events and returns Done.
    struct MockExecutor;

    #[async_trait]
    impl TaskExecutor for MockExecutor {
        async fn execute(&self, assignment: TaskAssignment) -> Result<ExecutingTask, String> {
            let (event_tx, event_rx) = mpsc::channel(16);
            let (outcome_tx, outcome_rx) = mpsc::channel(4);
            let cancel = CancellationToken::new();

            let task_id = assignment.task_id.clone();
            tokio::spawn(async move {
                // Emit 3 events.
                for i in 0..3 {
                    let _ = event_tx
                        .send(("test_event".into(), json!({"index": i, "task": &task_id})))
                        .await;
                }
                drop(event_tx);
                let _ = outcome_tx
                    .send(TaskOutcome::Done {
                        answer: format!("done:{task_id}"),
                        metadata: None,
                    })
                    .await;
            });

            Ok(ExecutingTask {
                events: event_rx,
                outcomes: outcome_rx,
                cancel,
                answers: None,
            })
        }
    }

    /// Mock executor that always fails.
    struct FailingExecutor;

    #[async_trait]
    impl TaskExecutor for FailingExecutor {
        async fn execute(&self, _assignment: TaskAssignment) -> Result<ExecutingTask, String> {
            Err("executor error".into())
        }
    }

    #[tokio::test]
    async fn test_worker_pulls_and_executes() {
        let transport = LocalTransport::with_defaults();
        let _worker = Worker::new(
            transport.clone() as Arc<dyn WorkerTransport>,
            Arc::new(MockExecutor),
        );

        // Spawn worker.
        let worker_handle = tokio::spawn({
            let worker = Worker::new(
                transport.clone() as Arc<dyn WorkerTransport>,
                Arc::new(MockExecutor),
            );
            async move { worker.run().await }
        });

        // Assign a task.
        coord(&transport)
            .assign(TaskAssignment {
                task_id: "t1".into(),
                parent_task_id: None,
                run_id: "r1".into(),
                spec: TaskSpec::Agent {
                    agent_id: "a".into(),
                    question: "q".into(),
                },
                policy: None,
            })
            .await
            .unwrap();

        // Collect events and outcome.
        let mut events = vec![];
        loop {
            match coord(&transport).recv().await {
                Some(WorkerMessage::Event { event_type, .. }) => events.push(event_type),
                Some(WorkerMessage::Outcome { outcome, .. }) => {
                    assert!(matches!(outcome, TaskOutcome::Done { .. }));
                    break;
                }
                None => panic!("transport closed unexpectedly"),
            }
        }
        assert_eq!(events.len(), 3);

        // Drop transport sender to shut down worker.
        drop(transport);
        let _ = worker_handle;
    }

    #[tokio::test]
    async fn test_worker_executor_error() {
        let transport = LocalTransport::with_defaults();

        tokio::spawn({
            let transport = transport.clone();
            async move {
                let worker = Worker::new(
                    transport as Arc<dyn WorkerTransport>,
                    Arc::new(FailingExecutor),
                );
                worker.run().await;
            }
        });

        coord(&transport)
            .assign(TaskAssignment {
                task_id: "t1".into(),
                parent_task_id: None,
                run_id: "r1".into(),
                spec: TaskSpec::Agent {
                    agent_id: "a".into(),
                    question: "q".into(),
                },
                policy: None,
            })
            .await
            .unwrap();

        // Should get a Failed outcome.
        match coord(&transport).recv().await {
            Some(WorkerMessage::Outcome {
                outcome: TaskOutcome::Failed(msg),
                ..
            }) => {
                assert_eq!(msg, "executor error");
            }
            other => panic!("expected Failed outcome, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_worker_suspended_outcome() {
        use agentic_core::delegation::SuspendReason;
        use agentic_core::human_input::SuspendedRunData;

        /// Executor that emits Suspended for Agent specs, Done for Resume specs.
        struct SuspendingExecutor;

        #[async_trait]
        impl TaskExecutor for SuspendingExecutor {
            async fn execute(&self, assignment: TaskAssignment) -> Result<ExecutingTask, String> {
                let (event_tx, event_rx) = mpsc::channel(16);
                let (outcome_tx, outcome_rx) = mpsc::channel(4);
                let cancel = CancellationToken::new();

                let task_id = assignment.task_id.clone();
                let spec = assignment.spec.clone();
                tokio::spawn(async move {
                    let _ = event_tx
                        .send(("test_event".into(), json!({"task": &task_id})))
                        .await;
                    drop(event_tx);
                    match spec {
                        TaskSpec::Agent { .. } => {
                            let _ = outcome_tx
                                .send(TaskOutcome::Suspended {
                                    reason: SuspendReason::HumanInput { questions: vec![] },
                                    resume_data: SuspendedRunData {
                                        from_state: "clarifying".into(),
                                        original_input: "test".into(),
                                        trace_id: "t1".into(),
                                        stage_data: json!({}),
                                        question: "what?".into(),
                                        suggestions: vec![],
                                    },
                                    trace_id: "t1".into(),
                                })
                                .await;
                        }
                        TaskSpec::Resume { .. } => {
                            let _ = outcome_tx
                                .send(TaskOutcome::Done {
                                    answer: "resumed-done".into(),
                                    metadata: None,
                                })
                                .await;
                        }
                        _ => {
                            let _ = outcome_tx
                                .send(TaskOutcome::Failed("unsupported spec".into()))
                                .await;
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
        }

        let transport = LocalTransport::with_defaults();
        tokio::spawn({
            let transport = transport.clone();
            async move {
                let worker = Worker::new(
                    transport as Arc<dyn WorkerTransport>,
                    Arc::new(SuspendingExecutor),
                );
                worker.run().await;
            }
        });

        // Assign an Agent task → should get Suspended outcome.
        coord(&transport)
            .assign(TaskAssignment {
                task_id: "t1".into(),
                parent_task_id: None,
                run_id: "r1".into(),
                spec: TaskSpec::Agent {
                    agent_id: "a".into(),
                    question: "q".into(),
                },
                policy: None,
            })
            .await
            .unwrap();

        let mut got_event = false;
        let got_suspended;
        loop {
            match coord(&transport).recv().await {
                Some(WorkerMessage::Event { .. }) => {
                    got_event = true;
                }
                Some(WorkerMessage::Outcome {
                    outcome: TaskOutcome::Suspended { .. },
                    ..
                }) => {
                    got_suspended = true;
                    break;
                }
                other => panic!("unexpected: {other:?}"),
            }
        }
        assert!(got_event, "should have received at least one event");
        assert!(got_suspended, "should have received Suspended outcome");

        // Now assign a Resume task → should get Done outcome.
        coord(&transport)
            .assign(TaskAssignment {
                task_id: "t1".into(),
                parent_task_id: None,
                run_id: "r1".into(),
                spec: TaskSpec::Resume {
                    run_id: "r1".into(),
                    resume_data: agentic_core::human_input::SuspendedRunData {
                        from_state: "clarifying".into(),
                        original_input: "test".into(),
                        trace_id: "t1".into(),
                        stage_data: json!({}),
                        question: "what?".into(),
                        suggestions: vec![],
                    },
                    answer: "the answer".into(),
                },
                policy: None,
            })
            .await
            .unwrap();

        let mut got_resume_event = false;
        loop {
            match coord(&transport).recv().await {
                Some(WorkerMessage::Event { .. }) => {
                    got_resume_event = true;
                }
                Some(WorkerMessage::Outcome {
                    outcome: TaskOutcome::Done { answer, .. },
                    ..
                }) => {
                    assert_eq!(answer, "resumed-done");
                    break;
                }
                other => panic!("unexpected on resume: {other:?}"),
            }
        }
        assert!(
            got_resume_event,
            "should have received event from resumed task"
        );
    }

    #[tokio::test]
    async fn test_worker_cancellation() {
        let transport = LocalTransport::with_defaults();

        // Executor that waits for cancellation.
        struct WaitingExecutor;

        #[async_trait]
        impl TaskExecutor for WaitingExecutor {
            async fn execute(&self, _assignment: TaskAssignment) -> Result<ExecutingTask, String> {
                let (_event_tx, event_rx) = mpsc::channel(1);
                let (outcome_tx, outcome_rx) = mpsc::channel(4);
                let cancel = CancellationToken::new();

                let cancel_clone = cancel.clone();
                tokio::spawn(async move {
                    cancel_clone.cancelled().await;
                    let _ = outcome_tx.send(TaskOutcome::Cancelled).await;
                });

                Ok(ExecutingTask {
                    events: event_rx,
                    outcomes: outcome_rx,
                    cancel,
                    answers: None,
                })
            }
        }

        tokio::spawn({
            let transport = transport.clone();
            async move {
                let worker = Worker::new(
                    transport as Arc<dyn WorkerTransport>,
                    Arc::new(WaitingExecutor),
                );
                worker.run().await;
            }
        });

        coord(&transport)
            .assign(TaskAssignment {
                task_id: "t1".into(),
                parent_task_id: None,
                run_id: "r1".into(),
                spec: TaskSpec::Agent {
                    agent_id: "a".into(),
                    question: "q".into(),
                },
                policy: None,
            })
            .await
            .unwrap();

        // Give the worker a moment to start the task.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Cancel via transport.
        coord(&transport).cancel("t1").await.unwrap();

        // Should get Cancelled outcome.
        match coord(&transport).recv().await {
            Some(WorkerMessage::Outcome {
                outcome: TaskOutcome::Cancelled,
                ..
            }) => {}
            other => panic!("expected Cancelled outcome, got {other:?}"),
        }
    }
}
