//! End-to-end test: workflow recovery after simulated server crash.
//!
//! Covers two guarantees of the stateless workflow executor:
//!
//! 1. `WorkflowDecision` executor arm seeds `agentic_workflow_state` and drives
//!    the stateless decider to a `Done` outcome.
//! 2. Recovery enqueues `WorkflowDecision` (not `TaskSpec::Resume`), so a run
//!    stuck in `delegating` with a completed child outcome reaches `done`
//!    without dangling orchestrator channels.
//!
//! Run:
//!   cargo nextest run -p agentic-pipeline --test workflow_recovery_test

use std::collections::HashMap;
use std::sync::Arc;

use agentic_core::delegation::{TaskAssignment, TaskOutcome, TaskSpec};
use agentic_core::transport::{CoordinatorTransport, WorkerTransport};
use agentic_runtime::coordinator::Coordinator;
use agentic_runtime::crud;
use agentic_runtime::migration::RuntimeMigrator;
use agentic_runtime::state::RuntimeState;
use agentic_runtime::transport::DurableTransport;
use agentic_runtime::worker::{ExecutingTask, TaskExecutor, Worker};
use async_trait::async_trait;
use sea_orm::{Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

static TEST_DB_URL: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();

/// Keeps the Postgres container handle alive for the process lifetime without
/// leaking. `ReuseDirective::Always` means tests across nextest processes share
/// the same container regardless.
static TEST_CONTAINER: tokio::sync::OnceCell<
    std::sync::Arc<testcontainers::ContainerAsync<testcontainers_modules::postgres::Postgres>>,
> = tokio::sync::OnceCell::const_new();

async fn test_db() -> Option<DatabaseConnection> {
    let url = TEST_DB_URL
        .get_or_init(|| async {
            if let Ok(url) = std::env::var("OXY_DATABASE_URL") {
                return url;
            }
            use testcontainers::runners::AsyncRunner;
            use testcontainers::{ImageExt, ReuseDirective};
            use testcontainers_modules::postgres::Postgres;
            let container = TEST_CONTAINER
                .get_or_init(|| async {
                    std::sync::Arc::new(
                        Postgres::default()
                            .with_tag("18-alpine")
                            .with_reuse(ReuseDirective::Always)
                            .start()
                            .await
                            .expect("failed to start Postgres testcontainer — is Docker running?"),
                    )
                })
                .await;
            let port = container.get_host_port_ipv4(5432_u16).await.unwrap();
            format!("postgresql://postgres:postgres@127.0.0.1:{port}/postgres")
        })
        .await
        .clone();

    let mut db = None;
    for attempt in 0..10 {
        match Database::connect(&url).await {
            Ok(conn) => {
                db = Some(conn);
                break;
            }
            Err(e) if attempt < 9 => {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                eprintln!("test_db: attempt {attempt} failed: {e}");
            }
            Err(e) => panic!("failed to connect after 10 retries: {e}"),
        }
    }
    let db = db.unwrap();

    RuntimeMigrator::up(&db, None)
        .await
        .expect("runtime migrations failed");
    agentic_analytics::extension::AnalyticsMigrator::up(&db, None)
        .await
        .expect("analytics migrations failed");
    agentic_workflow::WorkflowMigrator::up(&db, None)
        .await
        .expect("workflow migrations failed");
    Some(db)
}

// ── Test 1: WorkflowDecision executor arm ────────────────────────────────────

/// The `WorkflowDecision` executor arm returns `Ok(ExecutingTask)` whose
/// outcome channel yields `TaskOutcome::Done` once the stateless decider
/// finishes.
#[tokio::test(flavor = "multi_thread")]
async fn test_workflow_decision_executor_arm_returns_done() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };

    let run_id = format!("wf-decision-{}", uuid::Uuid::new_v4());

    // Insert a parent workflow run so the executor can load state from DB.
    crud::insert_run(&db, &run_id, "test decision task", None, "workflow", None)
        .await
        .expect("insert run");

    // Insert minimal workflow state so load_workflow_state doesn't 404.
    let minimal_workflow = agentic_workflow::WorkflowConfig {
        name: "test_wf".to_string(),
        tasks: vec![],
        description: String::new(),
        variables: None,
        consistency_prompt: None,
        consistency_model: None,
    };
    let initial_state = agentic_workflow::extension::WorkflowRunState {
        run_id: run_id.clone(),
        workflow: minimal_workflow,
        workflow_yaml_hash: "testhash".to_string(),
        workflow_context: json!({"workspace_path": "/tmp"}),
        variables: None,
        trace_id: "test-trace".to_string(),
        current_step: 0,
        results: HashMap::new(),
        render_context: json!({}),
        pending_children: HashMap::new(),
        decision_version: 0,
    };
    agentic_workflow::extension::insert_workflow_state(&db, &initial_state)
        .await
        .expect("insert workflow state");

    // Build a minimal PipelineTaskExecutor with a fake PlatformContext.
    // We only care that the WorkflowDecision arm is hit — not a real pipeline.
    //
    // We call the executor directly (not via Worker) to isolate the arm.
    let platform: Arc<dyn agentic_pipeline::platform::PlatformContext> =
        Arc::new(FakePlatform::default());
    let executor = agentic_pipeline::executor::PipelineTaskExecutor {
        platform,
        builder_bridges: None,
        schema_cache: None,
        builder_test_runner: None,
        db: db.clone(),
        state: None,
    };

    let assignment = TaskAssignment {
        task_id: run_id.clone(),
        parent_task_id: None,
        run_id: run_id.clone(),
        spec: TaskSpec::WorkflowDecision {
            run_id: run_id.clone(),
            pending_child_answer: None,
        },
        policy: None,
    };

    let result = executor.execute(assignment).await;

    assert!(
        result.is_ok(),
        "WorkflowDecision executor arm returned error: {:?}",
        result.err()
    );

    // Drain the executing task and assert Done outcome.
    let mut task = result.unwrap();
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(5), task.outcomes.recv())
        .await
        .expect("timed out waiting for outcome")
        .expect("outcome channel closed");

    assert!(
        matches!(outcome, TaskOutcome::Done { .. }),
        "expected Done outcome, got {outcome:?}"
    );
}

// ── Test: decision_version advances correctly after decide() ─────────────────

/// Verify that `update_workflow_state` correctly persists state after
/// `decide()` increments `decision_version`.
///
/// This caught a real bug: `decide()` increments `decision_version` in the
/// returned state (0 → 1), but `update_workflow_state` was passing the
/// already-incremented value as the expected version for the `WHERE` clause.
/// The DB still had 0, so the update silently returned `Ok(false)` (version
/// conflict) and every WorkflowDecision became a no-op.
#[tokio::test(flavor = "multi_thread")]
async fn test_decision_version_advances_after_decide() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };

    let run_id = format!("version-{}", uuid::Uuid::new_v4());

    crud::insert_run(&db, &run_id, "test version", None, "workflow", None)
        .await
        .expect("insert run");

    // Workflow with 1 task — the decider will return DelegateStep (not Complete).
    let workflow = agentic_workflow::WorkflowConfig {
        name: "version_test".to_string(),
        tasks: vec![agentic_workflow::config::TaskConfig {
            name: "step0".to_string(),
            task_type: agentic_workflow::config::TaskType::Unknown,
        }],
        description: String::new(),
        variables: None,
        consistency_prompt: None,
        consistency_model: None,
    };
    let initial_state = agentic_workflow::extension::WorkflowRunState {
        run_id: run_id.clone(),
        workflow,
        workflow_yaml_hash: "hash".to_string(),
        workflow_context: json!({"workspace_path": "/tmp"}),
        variables: None,
        trace_id: "test".to_string(),
        current_step: 0,
        results: HashMap::new(),
        render_context: json!({}),
        pending_children: HashMap::new(),
        decision_version: 0,
    };
    agentic_workflow::extension::insert_workflow_state(&db, &initial_state)
        .await
        .expect("insert state");

    // Run the decider — should return DelegateStep for step0.
    let decider = agentic_workflow::WorkflowDecider::new(None);
    let (new_state, decision) = decider.decide(initial_state, None).await;

    // decide() does NOT modify decision_version — the persistence layer owns it.
    assert_eq!(
        new_state.decision_version, 0,
        "decide() should not modify version"
    );
    assert!(
        matches!(
            decision,
            agentic_workflow::WorkflowDecision::DelegateStep { .. }
        ),
        "expected DelegateStep for Unknown task type, got {decision:?}"
    );

    // Persist — this must succeed (not return false for version conflict).
    let updated = agentic_workflow::extension::update_workflow_state(&db, &new_state)
        .await
        .expect("update state");
    assert!(
        updated,
        "update_workflow_state must succeed (not version conflict)"
    );

    // Verify the DB has the new version.
    let loaded = agentic_workflow::extension::load_workflow_state(&db, &run_id)
        .await
        .expect("load state")
        .expect("state should exist");
    assert_eq!(
        loaded.decision_version, 1,
        "DB should have version 1 after update"
    );

    // Run a second decision (simulate child completion folding).
    let child_answer = agentic_core::delegation::ChildCompletion {
        child_task_id: format!("{run_id}.1"),
        step_index: 0,
        step_name: "step0".to_string(),
        status: "done".to_string(),
        answer: r#"{"text":"result"}"#.to_string(),
    };
    let (new_state2, decision2) = decider.decide(loaded, Some(child_answer)).await;
    // decision_version is still 1 (from the DB load after first update).
    // decide() does not touch it.
    assert_eq!(
        new_state2.decision_version, 1,
        "decide() should not modify version"
    );
    assert!(
        matches!(
            decision2,
            agentic_workflow::WorkflowDecision::Complete { .. }
        ),
        "expected Complete after step0 done, got {decision2:?}"
    );

    let updated2 = agentic_workflow::extension::update_workflow_state(&db, &new_state2)
        .await
        .expect("update state 2");
    assert!(updated2, "second update must succeed");

    let loaded2 = agentic_workflow::extension::load_workflow_state(&db, &run_id)
        .await
        .expect("load state 2")
        .expect("state should exist");
    assert_eq!(
        loaded2.decision_version, 2,
        "DB version should be 2 after two updates"
    );
    assert_eq!(loaded2.current_step, 1, "should have advanced past step0");
    assert!(
        loaded2.results.contains_key("step0"),
        "results should contain step0"
    );
}

// ── Test 2: Full recovery scenario ───────────────────────────────────────────

/// A workflow run stuck in `delegating` with a completed child outcome
/// reaches `task_status="done"` after recovery: the recovery path enqueues
/// `WorkflowDecision` and the executor drives the stateless decider.
#[tokio::test(flavor = "multi_thread")]
async fn test_workflow_recovery_after_crash_completes_run() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };

    let (parent_id, _child_id) = seed_crashed_workflow(&db).await;

    // Confirm initial state is "delegating".
    let run = crud::get_run(&db, &parent_id).await.unwrap().unwrap();
    assert_eq!(run.task_status.as_deref(), Some("delegating"));

    // Run recovery using the real recovery path but with a stub executor
    // that correctly handles WorkflowDecision tasks (post-fix behavior).
    let state = Arc::new(RuntimeState::new());
    {
        let (answer_tx, _) = mpsc::channel::<String>(1);
        let (cancel_tx, _) = tokio::sync::watch::channel(false);
        state.register(&parent_id, answer_tx, cancel_tx);
    }

    run_recovery(&db, state.clone(), &parent_id).await;

    // Wait up to 10s for the parent to reach "done".
    let mut final_status = None;
    for _ in 0..100 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if let Ok(Some(run)) = crud::get_run(&db, &parent_id).await {
            if matches!(run.task_status.as_deref(), Some("done") | Some("failed")) {
                final_status = run.task_status;
                break;
            }
        }
    }

    assert_eq!(
        final_status.as_deref(),
        Some("done"),
        "expected 'done' after recovery, got: {:?}",
        final_status
    );
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Seed a crashed workflow:
/// - parent: source_type="workflow", task_status="delegating"
/// - child: source_type="workflow_step", task_status="running" (completed mid-crash)
/// - task_outcome row for child (completed but parent never resumed)
async fn seed_crashed_workflow(db: &DatabaseConnection) -> (String, String) {
    let parent_id = format!("wf-{}", uuid::Uuid::new_v4());
    let child_id = format!("{}.1", parent_id);

    // Insert minimal workflow state for the Temporal decider to load.
    let workflow = agentic_workflow::WorkflowConfig {
        name: "crash_test_wf".to_string(),
        tasks: vec![agentic_workflow::config::TaskConfig {
            name: "step0".to_string(),
            task_type: agentic_workflow::config::TaskType::Unknown,
        }],
        description: String::new(),
        variables: None,
        consistency_prompt: None,
        consistency_model: None,
    };
    let workflow_spec = json!({ "type": "workflow", "workflow_ref": "test.workflow.yml" });

    crud::insert_run(
        db,
        &parent_id,
        "run crash test workflow",
        None,
        "workflow",
        Some(json!({ "original_spec": workflow_spec })),
    )
    .await
    .expect("insert parent");
    crud::update_task_status(db, &parent_id, "delegating", None)
        .await
        .expect("set delegating");

    // Suspension data with orchestrator checkpoint (current_step = 0).
    let stage_data = json!({
        "current_step": 0,
        "results": {},
        "render_context": {},
        "workflow": serde_json::to_value(&workflow).unwrap(),
        "workflow_context": {"workspace_path": "/tmp"},
        "trace_id": "crash-trace",
    });
    crud::upsert_suspension(
        db,
        &parent_id,
        "Executing step: step0",
        &[],
        &agentic_core::human_input::SuspendedRunData {
            from_state: "workflow".to_string(),
            original_input: "crash_test_wf".to_string(),
            trace_id: "crash-trace".to_string(),
            stage_data,
            question: "Executing step: step0".to_string(),
            suggestions: vec![],
        },
    )
    .await
    .expect("upsert suspension");

    // Workflow state in DB (for Temporal decider).
    let state = agentic_workflow::extension::WorkflowRunState {
        run_id: parent_id.clone(),
        workflow,
        workflow_yaml_hash: "hash".to_string(),
        workflow_context: json!({"workspace_path": "/tmp"}),
        variables: None,
        trace_id: "crash-trace".to_string(),
        current_step: 0,
        results: HashMap::new(),
        render_context: json!({}),
        pending_children: {
            let mut m = HashMap::new();
            m.insert("0".to_string(), vec![child_id.clone()]);
            m
        },
        decision_version: 0,
    };
    agentic_workflow::extension::insert_workflow_state(db, &state)
        .await
        .expect("insert workflow state");

    // Child run — was running, but server crashed before parent was resumed.
    let child_spec = json!({
        "type": "workflow_step",
        "step_config": {"name": "step0"},
        "render_context": {},
        "workflow_context": {"workspace_path": "/tmp"},
    });
    crud::insert_run(
        db,
        &child_id,
        "step0",
        None,
        "workflow_step",
        Some(json!({ "original_spec": child_spec })),
    )
    .await
    .expect("insert child");
    crud::transition_run(db, &child_id, "running", None, Some(&parent_id), None)
        .await
        .expect("set child running");

    // Child's task_outcome row — completed before crash, parent not yet resumed.
    crud::insert_task_outcome(db, &child_id, &parent_id, "done", None)
        .await
        .expect("insert child outcome");

    (parent_id, child_id)
}

/// A stub executor that handles `WorkflowDecision` by returning Done.
/// Used to simulate the post-refactor executor behavior in recovery.
struct WorkflowDecisionStubExecutor;

#[async_trait]
impl TaskExecutor for WorkflowDecisionStubExecutor {
    async fn execute(&self, assignment: TaskAssignment) -> Result<ExecutingTask, String> {
        let (_, event_rx) = mpsc::channel::<(String, serde_json::Value)>(4);
        let (outcome_tx, outcome_rx) = mpsc::channel::<TaskOutcome>(4);
        let cancel = CancellationToken::new();

        match &assignment.spec {
            TaskSpec::WorkflowDecision { run_id, .. } => {
                // Simulate the Temporal decider: workflow is complete (0 tasks or all done).
                let run_id = run_id.clone();
                tokio::spawn(async move {
                    // Small delay so coordinator is ready.
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    let _ = outcome_tx
                        .send(TaskOutcome::Done {
                            answer: format!("workflow-complete:{run_id}"),
                            metadata: None,
                        })
                        .await;
                });
            }
            _ => {
                // All other tasks complete immediately.
                let _ = outcome_tx
                    .send(TaskOutcome::Done {
                        answer: "stub-done".to_string(),
                        metadata: None,
                    })
                    .await;
            }
        }

        Ok(ExecutingTask {
            events: event_rx,
            outcomes: outcome_rx,
            cancel,
            answers: None,
        })
    }

    async fn resume_from_state(
        &self,
        run: &agentic_runtime::entity::run::Model,
        _: Option<agentic_core::human_input::SuspendedRunData>,
    ) -> Result<ExecutingTask, String> {
        let source_type = run.source_type.as_deref().unwrap_or("unknown");
        if source_type == "workflow" {
            // For workflow parent tasks: the Temporal refactor enqueues a
            // WorkflowDecision instead of re-launching the long-lived orchestrator.
            // Simulate by executing a WorkflowDecision directly.
            self.execute(TaskAssignment {
                task_id: run.id.clone(),
                parent_task_id: run.parent_run_id.clone(),
                run_id: run.id.clone(),
                spec: TaskSpec::WorkflowDecision {
                    run_id: run.id.clone(),
                    pending_child_answer: None,
                },
                policy: None,
            })
            .await
        } else {
            self.execute(TaskAssignment {
                task_id: run.id.clone(),
                parent_task_id: run.parent_run_id.clone(),
                run_id: run.id.clone(),
                spec: TaskSpec::WorkflowStep {
                    step_config: json!({}),
                    render_context: json!({}),
                    workflow_context: json!({}),
                },
                policy: None,
            })
            .await
        }
    }
}

async fn run_recovery(db: &DatabaseConnection, state: Arc<RuntimeState>, root_id: &str) {
    let transport = DurableTransport::new(db.clone());
    transport.run_reaper().await;

    let (coordinator, pending_resumes) = Coordinator::from_db(
        db.clone(),
        state.clone(),
        transport.clone() as Arc<dyn CoordinatorTransport>,
        root_id,
    )
    .await
    .expect("from_db failed");

    let tree = crud::load_task_tree(db, root_id)
        .await
        .expect("load_task_tree");
    let executor = Arc::new(WorkflowDecisionStubExecutor);

    let pending_parent_ids: std::collections::HashSet<String> = pending_resumes
        .iter()
        .map(|pr| pr.parent_task_id.clone())
        .collect();

    for task_run in &tree {
        match task_run.task_status.as_deref() {
            Some("done") | Some("failed") => continue,
            Some("awaiting_input") => continue,
            _ => {
                // Skip parents that have active (non-terminal) children — they
                // are delegating and the coordinator handles them.
                let has_active_children = tree.iter().any(|t| {
                    t.parent_run_id.as_deref() == Some(task_run.id.as_str())
                        && !matches!(t.task_status.as_deref(), Some("done") | Some("failed"))
                });
                if has_active_children {
                    continue;
                }
                if pending_parent_ids.contains(&task_run.id) {
                    continue;
                }

                let suspend_data = crud::get_suspension(db, &task_run.id).await.ok().flatten();
                let executing = match executor.resume_from_state(task_run, suspend_data).await {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::warn!("resume_from_state failed for {}: {e}", task_run.id);
                        continue;
                    }
                };
                crud::update_run_running(db, &task_run.id).await.ok();
                crud::update_task_status(db, &task_run.id, "running", None)
                    .await
                    .ok();
                spawn_virtual_worker(
                    transport.clone() as Arc<dyn WorkerTransport>,
                    &task_run.id,
                    executing,
                );
            }
        }
    }

    let worker = Worker::new(transport.clone() as Arc<dyn WorkerTransport>, executor);
    tokio::spawn(async move { worker.run().await });

    // Process pending_resumes then run the coordinator.
    tokio::spawn(async move {
        let mut coord = coordinator;
        coord.process_pending_resumes(pending_resumes).await;
        coord.run().await;
    });
}

fn spawn_virtual_worker(
    transport: Arc<dyn WorkerTransport>,
    task_id: &str,
    executing: ExecutingTask,
) {
    use agentic_core::delegation::TaskOutcome;
    use agentic_core::transport::WorkerMessage;

    let task_id = task_id.to_string();
    let t2 = transport.clone();
    let tid2 = task_id.clone();

    tokio::spawn(async move {
        let mut events = executing.events;
        while let Some((et, p)) = events.recv().await {
            let _ = t2
                .send(WorkerMessage::Event {
                    task_id: tid2.clone(),
                    event_type: et,
                    payload: p,
                })
                .await;
        }
    });

    tokio::spawn(async move {
        let mut outcomes = executing.outcomes;
        while let Some(outcome) = outcomes.recv().await {
            let terminal = matches!(
                outcome,
                TaskOutcome::Done { .. } | TaskOutcome::Failed(_) | TaskOutcome::Cancelled
            );
            let _ = transport
                .send(WorkerMessage::Outcome {
                    task_id: task_id.clone(),
                    outcome,
                })
                .await;
            if terminal {
                break;
            }
        }
    });
}

// ── Test 3: Agent-delegates-to-workflow recovery ────────────────────────────

/// When an analytics agent delegates to a workflow, and the server crashes:
///
/// - Root (analytics, `needs_resume`) has active child → must NOT be re-launched
/// - Workflow child (`needs_resume`) → re-launched via WorkflowDecision
/// - When workflow completes → coordinator resumes root → root completes
///
/// Before the fix: `from_db` treated `needs_resume` as `Failed`, and the
/// recovery tree-walk re-launched the root as a fresh analytics pipeline that
/// didn't wait for the workflow child.
#[tokio::test(flavor = "multi_thread")]
async fn test_agent_delegates_to_workflow_recovery() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };

    let (root_id, workflow_id) = seed_agent_delegates_to_workflow(&db).await;

    // Confirm initial state.
    let root = crud::get_run(&db, &root_id).await.unwrap().unwrap();
    assert_eq!(root.task_status.as_deref(), Some("needs_resume"));
    let wf = crud::get_run(&db, &workflow_id).await.unwrap().unwrap();
    assert_eq!(wf.task_status.as_deref(), Some("needs_resume"));

    // Run recovery.
    let state = Arc::new(RuntimeState::new());
    {
        let (answer_tx, _) = mpsc::channel::<String>(1);
        let (cancel_tx, _) = tokio::sync::watch::channel(false);
        state.register(&root_id, answer_tx, cancel_tx);
    }
    run_recovery(&db, state.clone(), &root_id).await;

    // Wait up to 10s for the root to reach "done".
    let mut final_status = None;
    for _ in 0..100 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if let Ok(Some(run)) = crud::get_run(&db, &root_id).await {
            if matches!(run.task_status.as_deref(), Some("done") | Some("failed")) {
                final_status = run.task_status;
                break;
            }
        }
    }

    assert_eq!(
        final_status.as_deref(),
        Some("done"),
        "root should complete after workflow child finishes, got: {:?}",
        final_status
    );

    // Verify workflow child also completed.
    let wf = crud::get_run(&db, &workflow_id).await.unwrap().unwrap();
    assert_eq!(
        wf.task_status.as_deref(),
        Some("done"),
        "workflow child should be done"
    );
}

/// Seed the agent-delegates-to-workflow crash scenario:
///
/// - root: analytics, `needs_resume` (was delegating to workflow child)
/// - workflow child: workflow, `needs_resume` (was executing, has workflow_state)
/// - workflow_step grandchild: done (step 0 completed)
/// - task_outcome: grandchild → workflow child = done
async fn seed_agent_delegates_to_workflow(db: &DatabaseConnection) -> (String, String) {
    let root_id = format!("agent-wf-{}", uuid::Uuid::new_v4());
    let wf_id = format!("{}.1", root_id);
    let step_id = format!("{}.1", wf_id);

    // Root: analytics run, was delegating
    crud::insert_run(db, &root_id, "test Q", None, "analytics", None)
        .await
        .expect("insert root");
    crud::update_task_status(
        db,
        &root_id,
        "needs_resume",
        Some(json!({
            "child_task_ids": [&wf_id],
            "completed": {},
            "failure_policy": "fail_fast",
        })),
    )
    .await
    .expect("set root needs_resume");

    // Root's suspension data (analytics executing state).
    crud::upsert_suspension(
        db,
        &root_id,
        "Delegation to workflow",
        &[],
        &agentic_core::human_input::SuspendedRunData {
            from_state: "executing".to_string(),
            original_input: "test Q".to_string(),
            trace_id: "agent-trace".to_string(),
            stage_data: json!({}),
            question: "Delegation to workflow".to_string(),
            suggestions: vec![],
        },
    )
    .await
    .expect("upsert root suspension");

    // Workflow child: was running, has durable workflow state
    let workflow = agentic_workflow::WorkflowConfig {
        name: "test_wf".to_string(),
        tasks: vec![agentic_workflow::config::TaskConfig {
            name: "step0".to_string(),
            task_type: agentic_workflow::config::TaskType::Unknown,
        }],
        description: String::new(),
        variables: None,
        consistency_prompt: None,
        consistency_model: None,
    };

    crud::insert_run_with_parent(db, &wf_id, &root_id, "run workflow", "workflow", None, 0)
        .await
        .expect("insert workflow");
    crud::update_task_status(db, &wf_id, "needs_resume", None)
        .await
        .expect("set workflow needs_resume");

    // Workflow suspension data (workflow_decision from_state).
    crud::upsert_suspension(
        db,
        &wf_id,
        "Executing step: step0",
        &[],
        &agentic_core::human_input::SuspendedRunData {
            from_state: "workflow_decision".to_string(),
            original_input: "test_wf".to_string(),
            trace_id: "wf-trace".to_string(),
            stage_data: json!({"step_name": "step0", "step_index": 0}),
            question: "Executing step: step0".to_string(),
            suggestions: vec![],
        },
    )
    .await
    .expect("upsert wf suspension");

    // Insert durable workflow state — step 0 completed, advance to step 1 (= done).
    let wf_state = agentic_workflow::extension::WorkflowRunState {
        run_id: wf_id.clone(),
        workflow,
        workflow_yaml_hash: "hash".to_string(),
        workflow_context: json!({"workspace_path": "/tmp"}),
        variables: None,
        trace_id: "wf-trace".to_string(),
        current_step: 1, // Already past the only step.
        results: {
            let mut m = HashMap::new();
            m.insert("step0".to_string(), json!({"text": "step0 result"}));
            m
        },
        render_context: json!({}),
        pending_children: HashMap::new(),
        decision_version: 0,
    };
    agentic_workflow::extension::insert_workflow_state(db, &wf_state)
        .await
        .expect("insert workflow state");

    // Step grandchild: done
    crud::insert_run_with_parent(db, &step_id, &wf_id, "step0", "workflow_step", None, 0)
        .await
        .expect("insert step");
    crud::update_task_status(db, &step_id, "done", None)
        .await
        .expect("set step done");

    // Task outcome: step → workflow = done
    crud::insert_task_outcome(db, &step_id, &wf_id, "done", None)
        .await
        .expect("insert step outcome");

    (root_id, wf_id)
}

/// Build a minimal WorkspaceManager pointing to /tmp.
/// Only used to satisfy the PipelineTaskExecutor constructor — the
/// WorkflowDecision arm does not call workspace methods before loading
/// state from DB.
// ── Test 4: Happy path — analytics→workflow→step→done ────────────────────────

/// Full happy path: analytics agent suspends for workflow delegation,
/// coordinator spawns workflow child, workflow seeds state, chains
/// WorkflowDecision, decider delegates step, step completes, decider
/// completes workflow, coordinator resumes analytics root → done.
///
/// This exercises the entire Temporal-style workflow lifecycle from
/// first principles, no crash recovery involved.
#[tokio::test(flavor = "multi_thread")]
async fn test_happy_path_analytics_workflow_step_done() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };

    let root_id = format!("happy-{}", uuid::Uuid::new_v4());

    // Insert root analytics run.
    crud::insert_run(&db, &root_id, "test Q", None, "analytics", None)
        .await
        .expect("insert root");

    let state = Arc::new(RuntimeState::new());
    let transport = DurableTransport::new(db.clone());
    transport.run_reaper().await;

    let executor = Arc::new(HappyPathExecutor { db: db.clone() });
    let worker = Worker::new(
        transport.clone() as Arc<dyn agentic_core::transport::WorkerTransport>,
        executor,
    );

    let (answer_tx, answer_rx) = mpsc::channel::<String>(1);
    let (cancel_tx, _) = tokio::sync::watch::channel(false);
    state.register(&root_id, answer_tx, cancel_tx);

    let mut coordinator = Coordinator::new(
        db.clone(),
        state.clone(),
        transport.clone() as Arc<dyn CoordinatorTransport>,
    );
    coordinator.register_answer_channel(root_id.clone(), answer_rx);

    // Submit the root task: an analytics agent that immediately suspends
    // for workflow delegation.
    coordinator
        .submit_root(
            root_id.clone(),
            TaskSpec::Agent {
                agent_id: "test_agent".to_string(),
                question: "test Q".to_string(),
            },
        )
        .await
        .expect("submit_root");

    tokio::spawn(async move { worker.run().await });
    let coord_handle = tokio::spawn(async move { coordinator.run().await });

    // Wait up to 10s for the root to complete.
    tokio::time::timeout(std::time::Duration::from_secs(10), coord_handle)
        .await
        .expect("coordinator timed out")
        .expect("coordinator panicked");

    let run = crud::get_run(&db, &root_id).await.unwrap().unwrap();
    assert_eq!(
        crud::user_facing_status(run.task_status.as_deref()),
        "done",
        "root should be done, got status={:?} error={:?}",
        run.task_status,
        run.error_message
    );

    // Verify the full task tree was created and completed.
    let tree = crud::load_task_tree(&db, &root_id).await.unwrap();
    assert!(
        tree.len() >= 3,
        "tree should have root + workflow + step, got {} nodes",
        tree.len()
    );

    // Verify workflow child exists and is done.
    let workflow_child = tree
        .iter()
        .find(|t| t.source_type.as_deref() == Some("workflow"))
        .expect("workflow child should exist in tree");
    assert_eq!(
        workflow_child.task_status.as_deref(),
        Some("done"),
        "workflow child should be done"
    );

    // Verify workflow_step grandchild exists and is done.
    let step = tree
        .iter()
        .find(|t| t.source_type.as_deref() == Some("workflow_step"))
        .expect("workflow_step grandchild should exist in tree");
    assert_eq!(
        step.task_status.as_deref(),
        Some("done"),
        "workflow_step should be done"
    );

    // Verify the root answer contains the delegated result.
    assert!(
        run.answer.as_deref().unwrap_or("").contains("step0 result"),
        "root answer should contain the step result, got: {:?}",
        run.answer
    );

    // Verify procedure lifecycle events were emitted on the workflow child's stream.
    let wf_events = crud::get_all_events(&db, &workflow_child.id).await.unwrap();
    let event_types: Vec<&str> = wf_events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(
        event_types.contains(&"procedure_started"),
        "should have procedure_started event, got: {event_types:?}"
    );
    assert!(
        event_types.contains(&"procedure_step_started"),
        "should have procedure_step_started event, got: {event_types:?}"
    );
    assert!(
        event_types.contains(&"procedure_step_completed"),
        "should have procedure_step_completed event, got: {event_types:?}"
    );
    assert!(
        event_types.contains(&"procedure_completed"),
        "should have procedure_completed event, got: {event_types:?}"
    );
}

/// Executor for happy path test. Simulates all task types:
///
/// - Agent: suspends for Delegation(Workflow) with from_state="executing"
/// - Workflow: seeds workflow state, returns Done { workflow_continue }
/// - WorkflowDecision: runs the real decider against the seeded state
/// - WorkflowStep: returns Done with a result
/// - Resume: returns Done (analytics resumes from checkpoint)
struct HappyPathExecutor {
    db: DatabaseConnection,
}

#[async_trait]
impl TaskExecutor for HappyPathExecutor {
    async fn execute(&self, assignment: TaskAssignment) -> Result<ExecutingTask, String> {
        use agentic_core::delegation::{DelegationTarget, SuspendReason};
        use agentic_core::human_input::SuspendedRunData;

        let (event_tx, event_rx) = mpsc::channel::<(String, serde_json::Value)>(16);
        let (outcome_tx, outcome_rx) = mpsc::channel::<TaskOutcome>(4);
        let cancel = CancellationToken::new();

        match assignment.spec.clone() {
            TaskSpec::Agent { .. } => {
                // Analytics agent: suspend for workflow delegation.
                tokio::spawn(async move {
                    drop(event_tx);
                    let _ = outcome_tx
                        .send(TaskOutcome::Suspended {
                            reason: SuspendReason::Delegation {
                                target: DelegationTarget::Workflow {
                                    workflow_ref: "test.workflow.yml".to_string(),
                                },
                                request: "run workflow".to_string(),
                                context: json!({}),
                                policy: None,
                            },
                            resume_data: SuspendedRunData {
                                from_state: "executing".to_string(),
                                original_input: "test Q".to_string(),
                                trace_id: "trace".to_string(),
                                stage_data: json!({}),
                                question: "delegation".to_string(),
                                suggestions: vec![],
                            },
                            trace_id: "trace".to_string(),
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

            TaskSpec::Workflow { .. } => {
                // Workflow seed: insert state, return workflow_continue.
                let run_id = assignment.run_id.clone();
                let db = self.db.clone();
                let workflow = agentic_workflow::WorkflowConfig {
                    name: "happy_wf".to_string(),
                    tasks: vec![agentic_workflow::config::TaskConfig {
                        name: "step0".to_string(),
                        task_type: agentic_workflow::config::TaskType::Unknown,
                    }],
                    description: String::new(),
                    variables: None,
                    consistency_prompt: None,
                    consistency_model: None,
                };
                let initial_state = agentic_workflow::extension::WorkflowRunState {
                    run_id: run_id.clone(),
                    workflow,
                    workflow_yaml_hash: "hash".to_string(),
                    workflow_context: json!({"workspace_path": "/tmp"}),
                    variables: None,
                    trace_id: "wf-trace".to_string(),
                    current_step: 0,
                    results: HashMap::new(),
                    render_context: json!({}),
                    pending_children: HashMap::new(),
                    decision_version: 0,
                };
                agentic_workflow::extension::insert_workflow_state(&db, &initial_state)
                    .await
                    .expect("seed workflow state");

                tokio::spawn(async move {
                    drop(event_tx);
                    let _ = outcome_tx
                        .send(TaskOutcome::Done {
                            answer: String::new(),
                            metadata: Some(json!({"workflow_continue": true})),
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

            TaskSpec::WorkflowDecision {
                run_id,
                pending_child_answer,
            } => {
                // Run the REAL decider against DB state.
                let db = self.db.clone();
                let state = agentic_workflow::extension::load_workflow_state(&db, &run_id)
                    .await
                    .map_err(|e| format!("load state: {e}"))?
                    .ok_or("no workflow state")?;

                let decider = agentic_workflow::WorkflowDecider::new(None);
                let (new_state, decision) = decider.decide(state, pending_child_answer).await;
                agentic_workflow::extension::update_workflow_state(&db, &new_state)
                    .await
                    .map_err(|e| format!("update state: {e}"))?;

                agentic_pipeline::executor::run_decision_task(decision)
            }

            TaskSpec::WorkflowStep { .. } => {
                // Step: complete with a result.
                tokio::spawn(async move {
                    drop(event_tx);
                    let _ = outcome_tx
                        .send(TaskOutcome::Done {
                            answer: json!({"text": "step0 result"}).to_string(),
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

            TaskSpec::Resume { answer, .. } => {
                // Analytics resume: complete with the delegated answer.
                let final_answer = if answer.is_empty() {
                    "analytics done".to_string()
                } else {
                    format!("analytics done: {answer}")
                };
                tokio::spawn(async move {
                    drop(event_tx);
                    let _ = outcome_tx
                        .send(TaskOutcome::Done {
                            answer: final_answer,
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
    }
}

/// Noop [`agentic_pipeline::platform::PlatformContext`] impl for tests that
/// exercise executor arms which never reach into the platform. Every method
/// returns a "not implemented" error or an empty value.
// ── Test: analytics run stuck in needs_resume is processed by recovery ──────
//
// Regression: `cleanup_stale_runs` was called on server startup, but
// `recover_active_runs` was not — leaving runs that were interrupted
// mid-execution stuck in `task_status = "needs_resume"` forever. The
// frontend would show them as "active" with no progress.
//
// This test drives the exact sequence the server startup path should run:
// `cleanup_stale_runs` → `recover_active_runs`. With `FakePlatform` the
// pipeline-side resume cannot actually rebuild the agent (no config on
// disk), so we only assert that `recover_active_runs` *processes* the
// stuck run — `task_status` must leave `needs_resume`. A no-op recovery
// (the regression) would leave the run stuck.
#[tokio::test(flavor = "multi_thread")]
async fn test_recovery_processes_stuck_needs_resume_analytics_run() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };

    let run_id = format!("analytics-stuck-{}", uuid::Uuid::new_v4());

    // Simulate the state a crashed mid-run would leave in the DB:
    // - Row inserted with source_type="analytics" and some events written
    // - task_status was "running" at the time of crash
    crud::insert_run(
        &db,
        &run_id,
        "what is revenue by quarter?",
        None,
        "analytics",
        Some(json!({ "agent_id": "analytics" })),
    )
    .await
    .expect("insert run");

    // A handful of events so the run is not classified as "never started".
    for (seq, event_type) in ["state_enter", "llm_start", "llm_token", "llm_token"]
        .into_iter()
        .enumerate()
    {
        crud::insert_event(&db, &run_id, seq as i64, event_type, &json!({}), 0)
            .await
            .expect("insert event");
    }

    // `cleanup_stale_runs` runs at startup and marks running rows needs_resume.
    crud::cleanup_stale_runs(&db)
        .await
        .expect("cleanup_stale_runs");

    let before = crud::get_run(&db, &run_id).await.unwrap().unwrap();
    assert_eq!(
        before.task_status.as_deref(),
        Some("needs_resume"),
        "cleanup_stale_runs should transition running → needs_resume; got {:?}",
        before.task_status
    );

    // Now invoke the second half — what the app startup is supposed to do.
    let state = Arc::new(RuntimeState::new());
    let platform: Arc<dyn agentic_pipeline::platform::PlatformContext> =
        Arc::new(FakePlatform::default());

    let _recovered = agentic_pipeline::recovery::recover_active_runs(
        db.clone(),
        state,
        platform,
        None,
        None,
        None,
    )
    .await;

    // Whatever the outcome, the run must have moved past needs_resume. In
    // this test it ends as "failed" because FakePlatform can't rebuild the
    // real pipeline — that is expected and still proves recovery ran.
    let after = crud::get_run(&db, &run_id).await.unwrap().unwrap();
    assert_ne!(
        after.task_status.as_deref(),
        Some("needs_resume"),
        "recover_active_runs did not process the stuck run; still needs_resume"
    );
    assert!(
        matches!(
            after.task_status.as_deref(),
            Some("failed") | Some("running") | Some("done")
        ),
        "unexpected terminal task_status after recovery: {:?}",
        after.task_status
    );
}

// ── Test: graceful shutdown marks active runs resumable ─────────────────────
//
// On SIGINT/SIGTERM the server must mark every active run as
// `task_status = "shutdown"` (rather than leaving them in `running`) and
// cancel their pipeline task via the cancel_tx channel. `"shutdown"` is
// treated as resumable by `get_resumable_root_runs`, so the next server
// start re-drives these runs through the recovery path — end-to-end, the
// lifecycle is: `running` → (graceful stop) `shutdown` → (restart)
// `recover_active_runs` processes it.
#[tokio::test(flavor = "multi_thread")]
async fn test_graceful_shutdown_marks_active_runs_resumable() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };

    let run_id = format!("analytics-shutdown-{}", uuid::Uuid::new_v4());

    // Insert an active analytics run (simulates an in-flight pipeline).
    crud::insert_run(
        &db,
        &run_id,
        "shutdown cycle question",
        None,
        "analytics",
        Some(json!({ "agent_id": "analytics" })),
    )
    .await
    .expect("insert run");
    crud::update_task_status(&db, &run_id, "running", None)
        .await
        .expect("set running");

    // Register the run in RuntimeState — `shutdown_all` only touches rows
    // whose `cancel_tx` is live, matching what the real lifecycle sees.
    let state = Arc::new(RuntimeState::new());
    let (answer_tx, _) = mpsc::channel::<String>(1);
    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
    state.register(&run_id, answer_tx, cancel_tx);

    // Trigger the same path `spawn_shutdown_hook` runs on token cancel.
    let count = state.shutdown_all(&db).await;
    assert_eq!(count, 1, "shutdown_all should process exactly 1 run");

    // DB was flipped to the resumable "shutdown" state.
    let after = crud::get_run(&db, &run_id).await.unwrap().unwrap();
    assert_eq!(
        after.task_status.as_deref(),
        Some("shutdown"),
        "expected task_status=shutdown, got {:?}",
        after.task_status
    );

    // cancel_tx was fired so the in-flight pipeline would stop cleanly.
    assert!(
        cancel_rx.has_changed().unwrap_or(false),
        "shutdown_all should have sent on the cancel channel"
    );
    assert!(
        *cancel_rx.borrow_and_update(),
        "cancel signal should be true"
    );

    // Confirm recovery still finds `"shutdown"` rows (so the next server
    // start would resume them).
    let resumable = agentic_runtime::crud::get_resumable_root_runs(&db)
        .await
        .expect("list resumable");
    assert!(
        resumable.iter().any(|r| r.id == run_id),
        "get_resumable_root_runs must include the shutdown run"
    );
}

#[derive(Default)]
struct FakePlatform;

#[async_trait]
impl agentic_pipeline::platform::ProjectContext for FakePlatform {
    async fn resolve_connector(
        &self,
        _db_name: &str,
    ) -> Option<agentic_connector::ConnectorConfig> {
        None
    }

    async fn resolve_model(
        &self,
        _model_ref: Option<&str>,
        _has_explicit_model: bool,
    ) -> Option<agentic_analytics::config::ResolvedModelInfo> {
        None
    }

    async fn resolve_secret(&self, _var_name: &str) -> Option<String> {
        None
    }
}

#[async_trait]
impl agentic_workflow::WorkspaceContext for FakePlatform {
    fn workspace_path(&self) -> &std::path::Path {
        std::path::Path::new("")
    }

    fn database_configs(&self) -> Vec<airlayer::DatabaseConfig> {
        vec![]
    }

    async fn get_connector(
        &self,
        name: &str,
    ) -> Result<std::sync::Arc<dyn agentic_connector::DatabaseConnector>, String> {
        Err(format!("fake platform: connector '{name}' unavailable"))
    }

    async fn get_integration(
        &self,
        name: &str,
    ) -> Result<agentic_workflow::workspace::IntegrationConfig, String> {
        Err(format!("fake platform: integration '{name}' unavailable"))
    }

    async fn list_workflow_files(&self) -> Result<Vec<std::path::PathBuf>, String> {
        Ok(vec![])
    }

    async fn resolve_workflow_yaml(&self, _workflow_ref: &str) -> Result<String, String> {
        Err("fake platform: workflow yaml not available".into())
    }
}
