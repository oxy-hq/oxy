//! The delivery task: one `notification_deliver` run per `ctx.notify` call.
//!
//! Queued in the same transaction as the send and its signals
//! ([`super::send`]), drained by whichever pod takes it — serve or worker, both
//! register the push transport — and it outlives the function invocation that
//! queued it. What it does is [`super::delivery::deliver_send`]; this file is
//! only the queue plumbing, after `anomaly_notify`.
//!
//! **Retries resend only what failed.** A channel the provider refused is
//! recorded `failed` and the task fails, so the queue's retry policy runs it
//! again; the next attempt re-claims the `failed` rows and nothing else. A
//! delivery left `claimed` (the pod died mid-send) is never retried — it may
//! have gone, and a second push or email is worse than a missed one when the
//! inbox already holds the notification.
//!
//! Rollout: the kind and the enqueue ship together. A pod that predates it
//! fails the task as an unknown kind, and the retry lands it on one that knows.

use agentic_core::delegation::{
    BackoffStrategy, RetryPolicy, TaskAssignment, TaskOutcome, TaskPolicy, TaskSpec,
};
use agentic_runtime::crud::{TaskScope, enqueue_task, insert_run};
use agentic_runtime::worker::{ExecutingTask, TaskExecutor};
use async_trait::async_trait;
use futures::future::FutureExt;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// The `kind` of the delivery task, and the `source_type` of its run.
pub const NOTIFICATION_DELIVER_KIND: &str = "notification_deliver";

/// What the run is called in the Orchestrator Dashboard.
const RUN_NAME: &str = "Notification delivery";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliverPayload {
    pub send_id: Uuid,
}

/// A provider that refused is usually throttling or rotating credentials; a
/// few spaced retries cover that without resending what already went.
fn policy() -> TaskPolicy {
    TaskPolicy {
        retry: Some(RetryPolicy {
            max_retries: 3,
            backoff: BackoffStrategy::Exponential {
                initial_delay_ms: 30_000,
                max_delay_ms: 600_000,
            },
            retry_on: Vec::new(),
        }),
        fallback_targets: Vec::new(),
    }
}

/// Queue the delivery of send `send_id`, inside the caller's transaction. The
/// task id is the run id, as the global-run driver expects of a root task.
pub async fn enqueue(
    txn: &impl ConnectionTrait,
    run_id: &str,
    send_id: Uuid,
    workspace_id: Uuid,
) -> Result<(), DbErr> {
    let payload = DeliverPayload { send_id };
    let spec = TaskSpec::Custom {
        kind: NOTIFICATION_DELIVER_KIND.to_string(),
        payload: serde_json::to_value(&payload).map_err(|e| DbErr::Custom(e.to_string()))?,
    };
    let metadata = serde_json::json!({ "trigger": "notify", "send_id": send_id });
    insert_run(
        txn,
        run_id,
        RUN_NAME,
        None,
        NOTIFICATION_DELIVER_KIND,
        Some(metadata),
        workspace_id,
    )
    .await?;
    enqueue_task(
        txn,
        run_id,
        run_id,
        None,
        &spec,
        Some(&policy()),
        TaskScope::Global,
    )
    .await
}

pub struct NotificationDeliverExecutor {
    pub db: DatabaseConnection,
}

#[async_trait]
impl TaskExecutor for NotificationDeliverExecutor {
    async fn execute(&self, assignment: TaskAssignment) -> Result<ExecutingTask, String> {
        let TaskSpec::Custom { kind, payload } = &assignment.spec else {
            return Err(format!(
                "unexpected spec for NotificationDeliverExecutor: {:?}",
                assignment.spec
            ));
        };
        if kind != NOTIFICATION_DELIVER_KIND {
            return Err(format!("unknown notification-delivery kind: {kind}"));
        }
        let payload: DeliverPayload = serde_json::from_value(payload.clone())
            .map_err(|e| format!("bad notification-delivery payload: {e}"))?;

        let (event_tx, event_rx) = mpsc::channel(16);
        let (outcome_tx, outcome_rx) = mpsc::channel(4);
        let db = self.db.clone();
        tokio::spawn(async move {
            let _ = event_tx
                .send((
                    "notification_deliver_started".into(),
                    serde_json::json!({ "send_id": payload.send_id }),
                ))
                .await;
            let _ = outcome_tx
                .send(outcome_of(&db, payload.send_id).await)
                .await;
        });
        Ok(ExecutingTask {
            events: event_rx,
            outcomes: outcome_rx,
            cancel: CancellationToken::new(),
            answers: None,
        })
    }
}

/// Deliver and turn whatever happened into a terminal outcome — a panic
/// included, or the run would sit `running` with nothing left to end it.
async fn outcome_of(db: &DatabaseConnection, send_id: Uuid) -> TaskOutcome {
    let result = std::panic::AssertUnwindSafe(super::delivery::deliver_send(db, send_id))
        .catch_unwind()
        .await;
    match result {
        Ok(report) if report.failed == 0 => TaskOutcome::Done {
            answer: report.summary(),
            metadata: Some(serde_json::json!({ "send_id": send_id })),
        },
        // Failing the run is what schedules the retry, and the retry resends
        // only the `failed` rows.
        Ok(report) => TaskOutcome::Failed(report.summary()),
        Err(_) => {
            tracing::error!(%send_id, "notification delivery panicked");
            TaskOutcome::Failed("notification delivery panicked".to_string())
        }
    }
}
