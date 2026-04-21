//! Transport traits for the coordinator-worker architecture.
//!
//! The coordinator publishes [`TaskAssignment`]s and receives
//! [`WorkerMessage`]s. Workers pull assignments and send messages back.
//! The transport layer is swappable: in-process channels for day 1,
//! gRPC or message queues for distributed deployment later.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::delegation::{TaskAssignment, TaskOutcome};

// ── WorkerMessage ────────────────────────────────────────────────────────────

/// A message from a worker to the coordinator.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerMessage {
    /// A serialized event from the running task.
    Event {
        task_id: String,
        event_type: String,
        payload: Value,
    },
    /// The task produced a final outcome.
    Outcome {
        task_id: String,
        outcome: TaskOutcome,
    },
}

// ── TransportError ───────────────────────────────────────────────────────────

/// Errors that can occur during transport operations.
#[derive(Debug)]
pub enum TransportError {
    /// The underlying channel has been closed.
    ChannelClosed,
    /// The referenced task was not found.
    NotFound,
    /// Any other transport-level error.
    Other(String),
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::ChannelClosed => write!(f, "transport channel closed"),
            TransportError::NotFound => write!(f, "task not found"),
            TransportError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for TransportError {}

// ── CoordinatorTransport ─────────────────────────────────────────────────────

/// Coordinator-side transport: sends assignments to workers, receives messages.
#[async_trait]
pub trait CoordinatorTransport: Send + Sync + 'static {
    /// Send a task assignment for a worker to pick up.
    async fn assign(&self, assignment: TaskAssignment) -> Result<(), TransportError>;

    /// Receive the next message from any worker.
    ///
    /// Returns `None` when all worker-side senders have been dropped.
    async fn recv(&self) -> Option<WorkerMessage>;

    /// Signal cancellation of a specific task.
    async fn cancel(&self, task_id: &str) -> Result<(), TransportError>;
}

// ── WorkerTransport ──────────────────────────────────────────────────────────

/// Worker-side transport: pulls assignments, sends messages back.
#[async_trait]
pub trait WorkerTransport: Send + Sync + 'static {
    /// Pull the next task assignment from the coordinator.
    ///
    /// Returns `None` when the coordinator-side sender has been dropped.
    async fn recv_assignment(&self) -> Option<TaskAssignment>;

    /// Send a message (event or outcome) back to the coordinator.
    async fn send(&self, msg: WorkerMessage) -> Result<(), TransportError>;

    /// Get a cancellation token for a specific task.
    ///
    /// The token is cancelled when the coordinator calls `cancel(task_id)`.
    fn cancellation_token(&self, task_id: &str) -> CancellationToken;

    /// Send a heartbeat for a running task.
    ///
    /// Durable transports update a heartbeat timestamp in the database;
    /// in-memory transports no-op. Workers call this periodically to prevent
    /// the reaper from reclaiming their tasks.
    async fn heartbeat(&self, _task_id: &str) -> Result<(), TransportError> {
        Ok(())
    }

    /// Spawn a background heartbeat loop for a task.
    ///
    /// Returns a [`CancellationToken`] — cancel it when the task completes.
    /// Durable transports periodically update the heartbeat timestamp;
    /// in-memory transports return a no-op token.
    fn spawn_heartbeat(&self, _task_id: &str, _interval: std::time::Duration) -> CancellationToken {
        CancellationToken::new()
    }
}
