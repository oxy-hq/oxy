//! How a dead-letter reaches the coordinator and the run.
//!
//! A dead-lettered queue row is terminal, and the task behind it produces no
//! outcome of its own: a deferral is not one, and a reaped claim's execution
//! is either gone or about to be cancelled by its heartbeat. Before this, the
//! coordinator driving the run never heard anything, so it waited for ever
//! and kept its scoped worker polling the queue until the process exited.
//! Every dead-letter now becomes one `Failed` outcome through the ordinary
//! outcome path, so a child fails or retries per its parent's policy and a
//! root fails its run.

use agentic_core::delegation::{ClaimEpoch, TaskOutcome};
use agentic_core::transport::WorkerMessage;

use super::DurableTransport;
use crate::crud;
use crate::orchestrator::entity::task_queue;

impl DurableTransport {
    /// A deferral reached its ceiling and [`crud::defer_task`] moved the row
    /// to `dead`. Tell the coordinator, then fail the run if nothing holds it.
    pub(super) async fn on_deferral_dead_lettered(
        &self,
        task_id: &str,
        claim: ClaimEpoch,
        max_wait_secs: u64,
        reason: &str,
    ) {
        // Loud on purpose: a pipeline blocked this long is a failure someone
        // has to see.
        tracing::error!(
            target: "transport",
            %task_id, max_wait_secs, %reason,
            "task dead-lettered: waited past its ceiling without ever being able to run"
        );
        let error = crud::deferral_dead_letter_message(reason);
        let msg = WorkerMessage::Outcome {
            task_id: task_id.to_string(),
            outcome: TaskOutcome::Failed(error.clone()),
            claim_epoch: claim,
        };
        if self.message_tx.send(msg).await.is_err() {
            tracing::warn!(
                target: "transport",
                %task_id,
                "dead-letter outcome not delivered: the coordinator channel is closed"
            );
        }
        self.fail_run_if_unheld(task_id, &error).await;
    }

    /// The backstop for a run no live driver will end. The driver this
    /// process runs holds the lease, so in production this is a no-op beside
    /// the outcome above; it matters where nothing is driving the run.
    /// Best-effort: the row is already `dead`, so a failure here leaves the
    /// run as it was, never half-written.
    async fn fail_run_if_unheld(&self, task_id: &str, error: &str) {
        match crud::fail_dead_lettered_run(&self.db, task_id, error).await {
            Ok(crud::DeadLetterRun::Failed { run_id }) => tracing::error!(
                target: "transport", %task_id, %run_id,
                "run failed: its task was dead-lettered by the queue"
            ),
            Ok(outcome) => tracing::debug!(
                target: "transport", %task_id, ?outcome,
                "dead-lettered task's run left to its driver"
            ),
            Err(e) => tracing::error!(
                target: "transport", %task_id, error = %e,
                "dead-lettered task: could not end its run; the coordinator still has \
                 the outcome"
            ),
        }
    }
}

/// What a stale execution reports once the reaper dead-lettered its row.
///
/// Its heartbeat read the claim as lost and cancelled it, so it reports
/// `Cancelled` — which the coordinator would record as a user's Stop. The run
/// ended because the queue gave up on the task, so it is reported as that.
/// `Done` and `Failed` are the task's own result and pass through.
pub(super) fn reported_for_dead_row(row: &task_queue::Model, outcome: TaskOutcome) -> TaskOutcome {
    match outcome {
        TaskOutcome::Cancelled if row.queue_status == "dead" => {
            TaskOutcome::Failed(crud::reaper_dead_letter_message(row.claim_count))
        }
        other => other,
    }
}
