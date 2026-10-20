//! A child result only counts toward a fan-out its parent is still waiting on,
//! and recovery reads a child's real terminal status.
//!
//! Orchestrator hardening items 3 and 5 (`internal-docs/2026-10-13-orchestrator-hardening-plan.md`).
//!
//! Item 3: `record_child_result` used to take a `_ =>` "resume directly" arm for
//! any parent not `WaitingOnChildren`. So a child finishing after its parent
//! timed out revived the parent (`timed_out` -> `running`), a stale sibling's
//! result became a `SuspendedHuman` parent's "answer", and a stale child id
//! was counted into a parent's *next* fan-out.
//!
//! Item 5: recovery rebuilt a child whose status was `cancelled` / `timed_out`
//! as a live task and replayed every recovered fan-out to the parent as `done`.
//!
//! Durations are >= 1s: this machine is loaded and shorter ones flake.
//!
//! Run:
//!   cargo nextest run -p agentic-runtime --test integration -E 'test(late_child_and_recovery_status_test)'

use std::sync::{Arc, Mutex};
use std::time::Duration;

use agentic_core::delegation::{
    DelegationItem, DelegationTarget, FanoutFailurePolicy, SuspendReason, TaskAssignment,
    TaskOutcome, TaskSpec,
};
use agentic_core::events::HumanInputQuestion;
use agentic_core::human_input::SuspendedRunData;
use agentic_core::transport::{CoordinatorTransport, WorkerTransport};
use agentic_runtime::coordinator::Coordinator;
use agentic_runtime::crud;
use agentic_runtime::state::RuntimeState;
use agentic_runtime::transport::LocalTransport;
use agentic_runtime::worker::{ExecutingTask, TaskExecutor, Worker};
use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::integration_tests::test_db;

fn checkpoint() -> SuspendedRunData {
    SuspendedRunData {
        from_state: "executing".into(),
        original_input: "root".into(),
        trace_id: "trace-1".into(),
        stage_data: json!({}),
        question: "delegate".into(),
        suggestions: vec![],
    }
}

fn delegate_to(question: &str) -> DelegationItem {
    DelegationItem {
        target: DelegationTarget::Agent {
            agent_id: "child".into(),
        },
        request: question.into(),
        context: json!({}),
    }
}

fn suspend(reason: SuspendReason) -> TaskOutcome {
    TaskOutcome::Suspended {
        reason,
        resume_data: checkpoint(),
        trace_id: "trace-1".into(),
    }
}

/// What the root does on its first run and on its first `Resume`.
#[derive(Clone, Copy)]
enum Root {
    /// Delegate to one `slow` child; every `Resume` finishes.
    DelegateOnce,
    /// Fan out to `fail` + `slow` (fail-fast); the first `Resume` asks the
    /// human a question, any later one finishes.
    FanOutThenAsk,
    /// Fan out to `fail` + `slow` (fail-fast); the first `Resume` delegates to
    /// `third`, any later one finishes.
    FanOutThenDelegate,
}

/// Scripted tree. Children are told apart by their question: `fail` fails at
/// once, `slow` finishes after `slow_ms` ignoring its cancel token (a child
/// whose cancel loses the race with its own completion), `third` after 3.5s.
struct Script {
    root: Root,
    slow_ms: u64,
    /// Every `Resume` the root receives, by answer.
    resumes: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl TaskExecutor for Script {
    async fn execute(&self, assignment: TaskAssignment) -> Result<ExecutingTask, String> {
        let (_event_tx, events) = mpsc::channel(1);
        let (outcome_tx, outcomes) = mpsc::channel(1);
        let root = self.root;
        let slow_ms = self.slow_ms;
        let resumes = self.resumes.clone();
        let spec = assignment.spec.clone();
        tokio::spawn(async move {
            let outcome = match spec {
                TaskSpec::Agent { question, .. } if question == "fail" => {
                    TaskOutcome::Failed("boom".into())
                }
                TaskSpec::Agent { question, .. } if question == "slow" => {
                    tokio::time::sleep(Duration::from_millis(slow_ms)).await;
                    TaskOutcome::Done {
                        answer: "slow child answer".into(),
                        metadata: None,
                    }
                }
                TaskSpec::Agent { question, .. } if question == "third" => {
                    tokio::time::sleep(Duration::from_millis(3500)).await;
                    TaskOutcome::Done {
                        answer: "third child answer".into(),
                        metadata: None,
                    }
                }
                TaskSpec::Agent { .. } => suspend(match root {
                    Root::DelegateOnce => SuspendReason::Delegation {
                        target: delegate_to("slow").target,
                        request: "slow".into(),
                        context: json!({}),
                        policy: None,
                    },
                    Root::FanOutThenAsk | Root::FanOutThenDelegate => {
                        SuspendReason::ParallelDelegation {
                            targets: vec![delegate_to("fail"), delegate_to("slow")],
                            failure_policy: FanoutFailurePolicy::FailFast,
                        }
                    }
                }),
                TaskSpec::Resume { answer, .. } => {
                    let nth = {
                        let mut seen = resumes.lock().unwrap();
                        seen.push(answer);
                        seen.len()
                    };
                    match (root, nth) {
                        (Root::FanOutThenAsk, 1) => suspend(SuspendReason::HumanInput {
                            questions: vec![HumanInputQuestion {
                                prompt: "which store?".into(),
                                suggestions: vec![],
                            }],
                        }),
                        (Root::FanOutThenDelegate, 1) => suspend(SuspendReason::Delegation {
                            target: delegate_to("third").target,
                            request: "third".into(),
                            context: json!({}),
                            policy: None,
                        }),
                        _ => TaskOutcome::Done {
                            answer: "root done".into(),
                            metadata: None,
                        },
                    }
                }
                other => panic!("unexpected spec: {other:?}"),
            };
            let _ = outcome_tx.send(outcome).await;
        });
        Ok(ExecutingTask {
            events,
            outcomes,
            cancel: CancellationToken::new(),
            answers: None,
        })
    }
}

/// Drive `root` to the end of the coordinator's loop. `human` is sent on the
/// run's answer channel after the given delay.
async fn drive(
    db: &DatabaseConnection,
    script: Script,
    suspend_timeout: Option<Duration>,
    human: Option<(Duration, &'static str)>,
) -> String {
    let run_id = format!("late-child-{}", uuid::Uuid::new_v4());
    crud::insert_run(
        db,
        &run_id,
        "root",
        None,
        "workflow",
        None,
        uuid::Uuid::nil(),
    )
    .await
    .unwrap();
    let transport = LocalTransport::with_defaults();
    let worker = Worker::new(
        transport.clone() as Arc<dyn WorkerTransport>,
        Arc::new(script),
    );
    let (answer_tx, answer_rx) = mpsc::channel::<String>(1);
    let mut coordinator = Coordinator::new(
        db.clone(),
        Arc::new(RuntimeState::new()),
        transport.clone() as Arc<dyn CoordinatorTransport>,
    );
    if let Some(timeout) = suspend_timeout {
        coordinator = coordinator.with_suspend_timeout(timeout);
    }
    coordinator.register_answer_channel(run_id.clone(), answer_rx);
    coordinator
        .submit_root(
            run_id.clone(),
            TaskSpec::Agent {
                agent_id: "root".into(),
                question: "root".into(),
                extra: None,
            },
        )
        .await
        .unwrap();
    if let Some((after, text)) = human {
        tokio::spawn(async move {
            tokio::time::sleep(after).await;
            let _ = answer_tx.send(text.into()).await;
        });
    } else {
        // Keep the channel open for the run's lifetime.
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(60)).await;
            drop(answer_tx);
        });
    }
    tokio::spawn(async move { worker.run().await });
    let driven = tokio::spawn(async move { coordinator.run().await });
    tokio::time::timeout(Duration::from_secs(40), driven)
        .await
        .expect("the coordinator never finished")
        .expect("coordinator panicked");
    run_id
}

async fn status_of(db: &DatabaseConnection, run_id: &str) -> Option<String> {
    crud::get_run(db, run_id)
        .await
        .unwrap()
        .unwrap()
        .task_status
}

/// The reported case: the parent timed out waiting, the child finishes later.
/// The late result must not turn `timed_out` back into `running` and hand the
/// parent a `Resume`.
#[tokio::test]
async fn a_child_finishing_after_its_parent_timed_out_does_not_revive_it() {
    let Some(db) = test_db().await else {
        return;
    };
    let resumes = Arc::new(Mutex::new(vec![]));
    let script = Script {
        root: Root::DelegateOnce,
        slow_ms: 3000,
        resumes: resumes.clone(),
    };
    let run_id = drive(&db, script, Some(Duration::from_secs(1)), None).await;

    assert_eq!(
        status_of(&db, &run_id).await.as_deref(),
        Some("timed_out"),
        "the late child result revived the timed-out parent"
    );
    assert!(
        resumes.lock().unwrap().is_empty(),
        "a timed-out parent was resumed with {:?}",
        resumes.lock().unwrap()
    );
}

/// A fail-fast sibling that lands after the parent moved on is not the answer
/// to the parent's later question to the human.
#[tokio::test]
async fn a_stale_sibling_result_is_not_a_suspended_humans_answer() {
    let Some(db) = test_db().await else {
        return;
    };
    let resumes = Arc::new(Mutex::new(vec![]));
    let script = Script {
        root: Root::FanOutThenAsk,
        slow_ms: 1500,
        resumes: resumes.clone(),
    };
    let human = Some((Duration::from_secs(4), "store 7"));
    let run_id = drive(&db, script, None, human).await;

    let seen = resumes.lock().unwrap().clone();
    assert_eq!(
        seen.len(),
        2,
        "fan-out resume, then the human's answer: {seen:?}"
    );
    assert_eq!(
        seen[1], "store 7",
        "the parent's answer to its question came from a stale child: {seen:?}"
    );
    assert_eq!(status_of(&db, &run_id).await.as_deref(), Some("done"));
}

/// A child of an earlier fan-out is not a member of the parent's next one.
#[tokio::test]
async fn a_stale_child_id_is_not_counted_into_a_new_fan_out() {
    let Some(db) = test_db().await else {
        return;
    };
    let resumes = Arc::new(Mutex::new(vec![]));
    let script = Script {
        root: Root::FanOutThenDelegate,
        slow_ms: 1500,
        resumes: resumes.clone(),
    };
    let run_id = drive(&db, script, None, None).await;

    let seen = resumes.lock().unwrap().clone();
    assert_eq!(
        seen.len(),
        2,
        "fan-out resume, then the third child's: {seen:?}"
    );
    assert_eq!(
        seen[1], "third child answer",
        "the second delegation completed on a stale child's result: {seen:?}"
    );
    assert_eq!(status_of(&db, &run_id).await.as_deref(), Some("done"));
}

// ── Item 5: recovery ────────────────────────────────────────────────────────

/// A root parked at a delegation to children `1` and `2`, as the coordinator
/// writes it, both of which ended with `statuses`.
async fn parent_with_ended_children(db: &DatabaseConnection, statuses: [&str; 2]) -> String {
    let parent = format!("recover-{}", uuid::Uuid::new_v4());
    crud::insert_run(db, &parent, "Q", None, "analytics", None, uuid::Uuid::nil())
        .await
        .unwrap();
    let ids: Vec<String> = ["1", "2"].iter().map(|n| format!("{parent}.{n}")).collect();
    crud::suspend_with_data_txn(
        db,
        &parent,
        "delegating",
        Some(json!({
            "child_task_ids": ids,
            "completed": {},
            "failure_policy": "best_effort",
        })),
        "delegate",
        &[],
        &checkpoint(),
    )
    .await
    .unwrap();
    for (child, status) in ids.iter().zip(statuses) {
        crud::insert_child_run(db, child, &parent, "child Q", "analytics", 0, None)
            .await
            .unwrap();
        crud::complete_child_failed_txn(db, child, child, &parent, status, "stopped early")
            .await
            .unwrap();
    }
    parent
}

async fn rebuild(
    db: &DatabaseConnection,
    root: &str,
) -> (
    Coordinator,
    Vec<agentic_runtime::coordinator::PendingResume>,
    Arc<LocalTransport>,
) {
    let transport = LocalTransport::with_defaults();
    let (coordinator, pending) = Coordinator::from_db(
        db.clone(),
        Arc::new(RuntimeState::new()),
        transport.clone() as Arc<dyn CoordinatorTransport>,
        root,
    )
    .await
    .unwrap();
    (coordinator, pending, transport)
}

/// Both children were stopped, so the parent resumes on a failed fan-out, not
/// a `done` one, and each child is reported as failed.
#[tokio::test]
async fn recovery_reports_a_cancelled_and_timed_out_fan_out_as_failed() {
    let Some(db) = test_db().await else {
        return;
    };
    let parent = parent_with_ended_children(&db, ["cancelled", "timed_out"]).await;
    let (_coordinator, pending, _t) = rebuild(&db, &parent).await;

    assert_eq!(pending.len(), 1, "every child is in");
    assert_eq!(pending[0].child_status, "failed");
    let answer: serde_json::Value = serde_json::from_str(&pending[0].answer).unwrap();
    for n in ["1", "2"] {
        assert_eq!(answer[format!("{parent}.{n}")]["status"], "failed");
    }
}

/// A child that ended `cancelled` / `timed_out` is terminal, not live: once
/// the parent resumes and finishes, the coordinator's loop ends.
#[tokio::test]
async fn recovery_does_not_rebuild_a_cancelled_or_timed_out_child_as_live() {
    let Some(db) = test_db().await else {
        return;
    };
    let parent = parent_with_ended_children(&db, ["cancelled", "timed_out"]).await;
    let (mut coordinator, pending, transport) = rebuild(&db, &parent).await;
    coordinator.process_pending_resumes(pending).await;

    let script = Script {
        root: Root::DelegateOnce,
        slow_ms: 1000,
        resumes: Arc::new(Mutex::new(vec![])),
    };
    let worker = Worker::new(
        transport.clone() as Arc<dyn WorkerTransport>,
        Arc::new(script),
    );
    tokio::spawn(async move { worker.run().await });
    let driven = tokio::spawn(async move { coordinator.run().await });
    let finished = tokio::time::timeout(Duration::from_secs(15), driven).await;
    assert!(
        finished.is_ok(),
        "the loop never ended: a cancelled/timed_out child was rebuilt as live"
    );
    assert_eq!(status_of(&db, &parent).await.as_deref(), Some("done"));
}
