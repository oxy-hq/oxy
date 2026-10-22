//! Starting a claimed task: the stretch between the claim and a running
//! [`ExecutingTask`].
//!
//! [`TaskExecutor::execute`] is not instant — it can wait on a compile, open a
//! connector, resolve a spec through the compile boundary — and three things
//! used to go wrong inside it:
//!
//! - **No heartbeat.** The ticker was spawned only after `execute` returned,
//!   so a setup longer than the visibility timeout (60 s) read as a dead
//!   worker: the reaper re-queued a live task, and a second copy started.
//!   `Worker::handle_task` now spawns the ticker before calling this.
//! - **A panic ended the worker's task with no outcome.** The coordinator
//!   waited for ever and the run read `running`. It is caught here and
//!   reported as `Failed` with the panic message.
//! - **A Stop could not reach it.** The cancel forwarder is wired to the
//!   [`ExecutingTask`]'s own token, which does not exist until `execute`
//!   returns. A Stop (or a heartbeat finding the claim lost) now abandons the
//!   setup and reports `Cancelled`.
//!
//! **No timeout.** Nothing in config bounds a task's setup, and none is
//! invented here: a legitimate setup ranges from milliseconds to however long
//! a compile or a connector handshake takes, and any fixed number would fail
//! a slow-but-healthy start on a busy deployment. What bounds a *hung* setup
//! is the claim's own token: a user's Stop, a peer's cancel and a lost claim
//! all fire it. The cost, stated so it is not rediscovered: a setup that hangs
//! with nobody pressing Stop now holds its claim (heartbeating) instead of
//! being reaped and run again — which was the duplicate execution above.

use std::any::Any;
use std::panic::AssertUnwindSafe;

use agentic_core::delegation::{TaskAssignment, TaskOutcome};
use futures::FutureExt;
use tokio_util::sync::CancellationToken;

use super::{ExecutingTask, TaskExecutor};

/// How a start ended.
pub(super) enum Started {
    /// The task is running; forward its events and outcomes.
    Running(ExecutingTask),
    /// The task ended before it ran: report this outcome and stop.
    Ended(Box<TaskOutcome>),
}

/// Run `execute`, turning a refusal, a panic or a cancel into an outcome.
pub(super) async fn start_task(
    executor: &dyn TaskExecutor,
    assignment: TaskAssignment,
    cancel: &CancellationToken,
) -> Started {
    let execute = AssertUnwindSafe(executor.execute(assignment)).catch_unwind();
    tokio::select! {
        // Cancel first: a Stop that landed before the claim stops the task
        // without starting it.
        biased;
        _ = cancel.cancelled() => ended(TaskOutcome::Cancelled),
        started = execute => match started {
            Ok(Ok(task)) => Started::Running(task),
            Ok(Err(msg)) => ended(TaskOutcome::Failed(msg)),
            Err(panic) => ended(TaskOutcome::Failed(format!(
                "task panicked while starting: {}",
                panic_message(panic.as_ref())
            ))),
        },
    }
}

fn ended(outcome: TaskOutcome) -> Started {
    Started::Ended(Box::new(outcome))
}

/// The text of a panic payload: `panic!` with a literal yields `&str`, with
/// formatting `String`; anything else has no message to show.
fn panic_message(payload: &(dyn Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("non-string panic payload")
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentic_core::delegation::TaskSpec;
    use async_trait::async_trait;

    struct Panics;
    #[async_trait]
    impl TaskExecutor for Panics {
        async fn execute(&self, _a: TaskAssignment) -> Result<ExecutingTask, String> {
            panic!("formatted {}", 7);
        }
    }

    struct Hangs;
    #[async_trait]
    impl TaskExecutor for Hangs {
        async fn execute(&self, _a: TaskAssignment) -> Result<ExecutingTask, String> {
            std::future::pending().await
        }
    }

    fn assignment() -> TaskAssignment {
        TaskAssignment {
            task_id: "t".into(),
            parent_task_id: None,
            run_id: "t".into(),
            spec: TaskSpec::Agent {
                agent_id: "a".into(),
                question: "q".into(),
                extra: None,
            },
            policy: None,
            claim_epoch: agentic_core::NO_CLAIM,
        }
    }

    #[tokio::test]
    async fn a_panic_becomes_a_failed_outcome_with_its_message() {
        let Started::Ended(outcome) =
            start_task(&Panics, assignment(), &CancellationToken::new()).await
        else {
            panic!("a panic must end the start");
        };
        match *outcome {
            TaskOutcome::Failed(msg) => assert!(msg.contains("formatted 7"), "{msg}"),
            other => panic!("a panic must end the start as Failed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_cancel_abandons_a_hung_start() {
        let cancel = CancellationToken::new();
        let fire = cancel.clone();
        tokio::spawn(async move { fire.cancel() });
        let started = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            start_task(&Hangs, assignment(), &cancel),
        )
        .await
        .expect("a cancel must end a hung start");
        assert!(matches!(started, Started::Ended(o) if matches!(*o, TaskOutcome::Cancelled)));
    }
}
