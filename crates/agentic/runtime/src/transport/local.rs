//! In-process transport using tokio channels.
//!
//! Both [`CoordinatorTransport`] and [`WorkerTransport`] are implemented on the
//! same [`LocalTransport`] struct. In a single-process deployment the
//! coordinator and worker(s) share one `Arc<LocalTransport>`.

use std::sync::Arc;

use agentic_core::delegation::TaskAssignment;
#[cfg(test)]
use agentic_core::delegation::TaskOutcome;
use agentic_core::transport::{
    CoordinatorTransport, TransportError, WorkerMessage, WorkerTransport,
};
use async_trait::async_trait;
use dashmap::DashMap;
use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;

/// In-process transport backed by tokio mpsc channels.
///
/// A single instance is shared (via `Arc`) between the coordinator and one or
/// more workers running in the same process. Task assignments flow from the
/// coordinator to workers through one channel; events and outcomes flow back
/// through another.
pub struct LocalTransport {
    /// Coordinator → Worker: task assignments.
    assign_tx: mpsc::Sender<TaskAssignment>,
    assign_rx: Mutex<mpsc::Receiver<TaskAssignment>>,

    /// Worker → Coordinator: events and outcomes.
    message_tx: mpsc::Sender<WorkerMessage>,
    message_rx: Mutex<mpsc::Receiver<WorkerMessage>>,

    /// Per-task cancellation tokens, keyed by task_id.
    cancel_tokens: DashMap<String, CancellationToken>,
}

impl LocalTransport {
    /// Create a new in-process transport.
    ///
    /// `assignment_buffer` controls backpressure on the assignment channel.
    /// `message_buffer` controls backpressure on the worker→coordinator channel.
    pub fn new(assignment_buffer: usize, message_buffer: usize) -> Arc<Self> {
        let (assign_tx, assign_rx) = mpsc::channel(assignment_buffer);
        let (message_tx, message_rx) = mpsc::channel(message_buffer);
        Arc::new(Self {
            assign_tx,
            assign_rx: Mutex::new(assign_rx),
            message_tx,
            message_rx: Mutex::new(message_rx),
            cancel_tokens: DashMap::new(),
        })
    }

    /// Create with default buffer sizes (64 assignments, 1024 messages).
    pub fn with_defaults() -> Arc<Self> {
        Self::new(64, 1024)
    }
}

#[async_trait]
impl CoordinatorTransport for LocalTransport {
    async fn assign(&self, assignment: TaskAssignment) -> Result<(), TransportError> {
        // Register a cancellation token for this task.
        self.cancel_tokens
            .insert(assignment.task_id.clone(), CancellationToken::new());

        self.assign_tx
            .send(assignment)
            .await
            .map_err(|_| TransportError::ChannelClosed)
    }

    async fn recv(&self) -> Option<WorkerMessage> {
        self.message_rx.lock().await.recv().await
    }

    async fn cancel(&self, task_id: &str) -> Result<(), TransportError> {
        match self.cancel_tokens.get(task_id) {
            Some(token) => {
                token.cancel();
                Ok(())
            }
            None => Err(TransportError::NotFound),
        }
    }
}

#[async_trait]
impl WorkerTransport for LocalTransport {
    async fn recv_assignment(&self) -> Option<TaskAssignment> {
        self.assign_rx.lock().await.recv().await
    }

    async fn send(&self, msg: WorkerMessage) -> Result<(), TransportError> {
        self.message_tx
            .send(msg)
            .await
            .map_err(|_| TransportError::ChannelClosed)
    }

    fn cancellation_token(&self, task_id: &str) -> CancellationToken {
        self.cancel_tokens
            .entry(task_id.to_string())
            .or_default()
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentic_core::transport::{CoordinatorTransport, WorkerTransport};
    use serde_json::json;

    /// Helper: get a coordinator-side reference.
    fn coord(t: &LocalTransport) -> &dyn CoordinatorTransport {
        t
    }
    /// Helper: get a worker-side reference.
    fn work(t: &LocalTransport) -> &dyn WorkerTransport {
        t
    }

    #[tokio::test]
    async fn test_assign_and_recv() {
        let transport = LocalTransport::with_defaults();
        let assignment = TaskAssignment {
            task_id: "t1".into(),
            parent_task_id: None,
            run_id: "r1".into(),
            spec: agentic_core::delegation::TaskSpec::Agent {
                agent_id: "analytics".into(),
                question: "test".into(),
            },
            policy: None,
        };

        coord(&transport).assign(assignment).await.unwrap();
        let received = work(&transport).recv_assignment().await.unwrap();
        assert_eq!(received.task_id, "t1");
        assert_eq!(received.run_id, "r1");
    }

    #[tokio::test]
    async fn test_worker_sends_event_coordinator_receives() {
        let transport = LocalTransport::with_defaults();

        let msg = WorkerMessage::Event {
            task_id: "t1".into(),
            event_type: "step_start".into(),
            payload: json!({"state": "clarifying"}),
        };
        work(&transport).send(msg).await.unwrap();

        let received = coord(&transport).recv().await.unwrap();
        match received {
            WorkerMessage::Event {
                task_id,
                event_type,
                ..
            } => {
                assert_eq!(task_id, "t1");
                assert_eq!(event_type, "step_start");
            }
            _ => panic!("expected Event"),
        }
    }

    #[tokio::test]
    async fn test_worker_sends_outcome_coordinator_receives() {
        let transport = LocalTransport::with_defaults();

        let msg = WorkerMessage::Outcome {
            task_id: "t1".into(),
            outcome: TaskOutcome::Done {
                answer: "42".into(),
                metadata: None,
            },
        };
        work(&transport).send(msg).await.unwrap();

        let received = coord(&transport).recv().await.unwrap();
        match received {
            WorkerMessage::Outcome { task_id, outcome } => {
                assert_eq!(task_id, "t1");
                assert!(matches!(outcome, TaskOutcome::Done { .. }));
            }
            _ => panic!("expected Outcome"),
        }
    }

    #[tokio::test]
    async fn test_cancel_propagates() {
        let transport = LocalTransport::with_defaults();

        // Assign a task (creates cancellation token).
        let assignment = TaskAssignment {
            task_id: "t1".into(),
            parent_task_id: None,
            run_id: "r1".into(),
            spec: agentic_core::delegation::TaskSpec::Agent {
                agent_id: "a".into(),
                question: "q".into(),
            },
            policy: None,
        };
        coord(&transport).assign(assignment).await.unwrap();

        // Worker gets the cancellation token.
        let token = work(&transport).cancellation_token("t1");
        assert!(!token.is_cancelled());

        // Coordinator cancels.
        coord(&transport).cancel("t1").await.unwrap();
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn test_multiple_tasks_interleaved() {
        let transport = LocalTransport::with_defaults();

        for i in 0..3 {
            let assignment = TaskAssignment {
                task_id: format!("t{i}"),
                parent_task_id: None,
                run_id: format!("r{i}"),
                spec: agentic_core::delegation::TaskSpec::Agent {
                    agent_id: "a".into(),
                    question: format!("q{i}"),
                },
                policy: None,
            };
            coord(&transport).assign(assignment).await.unwrap();
        }

        // Worker receives all three.
        for i in 0..3 {
            let a = work(&transport).recv_assignment().await.unwrap();
            assert_eq!(a.task_id, format!("t{i}"));
        }

        // Send outcomes in reverse order.
        for i in (0..3).rev() {
            let msg = WorkerMessage::Outcome {
                task_id: format!("t{i}"),
                outcome: TaskOutcome::Done {
                    answer: format!("a{i}"),
                    metadata: None,
                },
            };
            work(&transport).send(msg).await.unwrap();
        }

        // Coordinator receives all three outcomes.
        let mut received_ids = vec![];
        for _ in 0..3 {
            if let Some(WorkerMessage::Outcome { task_id, .. }) = coord(&transport).recv().await {
                received_ids.push(task_id);
            }
        }
        assert_eq!(received_ids, vec!["t2", "t1", "t0"]);
    }

    #[tokio::test]
    async fn test_heartbeat_noop() {
        let transport = LocalTransport::with_defaults();
        // LocalTransport heartbeat is a no-op that returns Ok.
        work(&transport).heartbeat("t1").await.unwrap();
    }

    #[tokio::test]
    async fn test_spawn_heartbeat_noop() {
        let transport = LocalTransport::with_defaults();
        // LocalTransport spawn_heartbeat returns a non-cancelled token (no-op loop).
        let token = work(&transport).spawn_heartbeat("t1", std::time::Duration::from_secs(10));
        assert!(!token.is_cancelled());
    }
}
