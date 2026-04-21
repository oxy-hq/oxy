//! Atomicity tests for [`agentic_workflow::extension::commit_decision`].
//!
//! `commit_decision` is the single atomic write-point for a workflow decision
//! boundary. It replaces the ad-hoc sequence `update_workflow_state` → emit
//! events → mark queue completed, which was observably non-atomic: a silent
//! failure anywhere in that sequence stranded a workflow in a half-committed
//! state (state advanced, events/queue never written).
//!
//! Run:
//!   cargo nextest run -p agentic-pipeline --test commit_decision_test

use std::collections::HashMap;

use agentic_runtime::crud;
use agentic_runtime::migration::RuntimeMigrator;
use agentic_workflow::extension::{
    CommitOutcome, DecisionCommit, DecisionTerminal, WorkflowRunState, commit_decision,
    insert_workflow_state, load_workflow_state,
};
use sea_orm::{Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use uuid::Uuid;

static TEST_DB_URL: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
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
                            .expect("failed to start Postgres testcontainer"),
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

// ── Fixture helpers ──────────────────────────────────────────────────────────

/// Build a two-step workflow with a delegated SQL step followed by an inline
/// formatter. Mirrors the real-world shape where a non-atomic commit stranded
/// the workflow between the two steps.
fn two_step_workflow() -> agentic_workflow::WorkflowConfig {
    agentic_workflow::WorkflowConfig {
        name: "atomic_test".to_string(),
        tasks: vec![
            agentic_workflow::config::TaskConfig {
                name: "step_sql".to_string(),
                task_type: agentic_workflow::config::TaskType::Unknown,
            },
            agentic_workflow::config::TaskConfig {
                name: "step_fmt".to_string(),
                task_type: agentic_workflow::config::TaskType::Unknown,
            },
        ],
        description: String::new(),
        variables: None,
        consistency_prompt: None,
        consistency_model: None,
    }
}

async fn seed_run(db: &DatabaseConnection) -> (String, WorkflowRunState) {
    let run_id = format!("wf-commit-{}", Uuid::new_v4());
    crud::insert_run(db, &run_id, "test commit_decision", None, "workflow", None)
        .await
        .expect("insert run");

    let state = WorkflowRunState {
        run_id: run_id.clone(),
        workflow: two_step_workflow(),
        workflow_yaml_hash: "hash".to_string(),
        workflow_context: json!({"workspace_path": "/tmp"}),
        variables: None,
        trace_id: "trace".to_string(),
        current_step: 0,
        results: HashMap::new(),
        render_context: json!({}),
        pending_children: HashMap::new(),
        decision_version: 0,
    };
    insert_workflow_state(db, &state)
        .await
        .expect("insert workflow state");
    (run_id, state)
}

/// Enqueue a decision task row so tests can observe it being marked terminal.
async fn seed_queue_row(db: &DatabaseConnection, task_id: &str, run_id: &str) {
    use agentic_core::delegation::TaskSpec;
    crud::enqueue_task(
        db,
        task_id,
        run_id,
        None,
        &TaskSpec::WorkflowDecision {
            run_id: run_id.to_string(),
            pending_child_answer: None,
        },
        None,
    )
    .await
    .expect("enqueue decision task");
}

async fn event_types(db: &DatabaseConnection, run_id: &str) -> Vec<String> {
    crud::get_all_events(db, run_id)
        .await
        .expect("read events")
        .into_iter()
        .map(|e| e.event_type)
        .collect()
}

async fn run_task_status(db: &DatabaseConnection, run_id: &str) -> Option<String> {
    crud::get_run(db, run_id)
        .await
        .unwrap()
        .unwrap()
        .task_status
}

async fn queue_status(db: &DatabaseConnection, task_id: &str) -> Option<String> {
    crud::get_queue_entry(db, task_id)
        .await
        .unwrap()
        .map(|m| m.queue_status)
}

// ── Tests ────────────────────────────────────────────────────────────────────

/// Continuing terminal: state CAS'd, events persisted with monotonic seq, run
/// + queue rows untouched. This models the common case of "decider advanced
/// the workflow to the next suspended step".
#[tokio::test(flavor = "multi_thread")]
async fn continuing_commits_state_and_events_atomically() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };
    let (run_id, mut state) = seed_run(&db).await;
    let decision_task_id = run_id.clone();
    seed_queue_row(&db, &decision_task_id, &run_id).await;

    // Decider-produced patch: advance past step 0, store its result.
    state.current_step = 1;
    state.results.insert("step_sql".into(), json!({"rows": 3}));

    let events: Vec<(String, Value)> = vec![
        (
            "procedure_step_completed".into(),
            json!({"step": "step_sql", "success": true}),
        ),
        ("procedure_step_started".into(), json!({"step": "step_fmt"})),
    ];

    let outcome = commit_decision(
        &db,
        DecisionCommit {
            run_id: run_id.clone(),
            decision_task_id: decision_task_id.clone(),
            expected_version: 0,
            new_state: state,
            events,
            attempt: 0,
            terminal: DecisionTerminal::Continuing,
        },
    )
    .await
    .expect("commit_decision");

    assert!(matches!(outcome, CommitOutcome::Committed));

    // State CAS'd: version advanced to 1, current_step == 1.
    let loaded = load_workflow_state(&db, &run_id).await.unwrap().unwrap();
    assert_eq!(loaded.decision_version, 1);
    assert_eq!(loaded.current_step, 1);
    assert!(loaded.results.contains_key("step_sql"));

    // Events persisted in order with monotonic seq starting at 0.
    let events = crud::get_all_events(&db, &run_id).await.unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].seq, 0);
    assert_eq!(events[0].event_type, "procedure_step_completed");
    assert_eq!(events[1].seq, 1);
    assert_eq!(events[1].event_type, "procedure_step_started");

    // Continuing terminal leaves queue & run untouched.
    assert_eq!(
        queue_status(&db, &decision_task_id).await.as_deref(),
        Some("queued")
    );
    assert_eq!(
        run_task_status(&db, &run_id).await.as_deref(),
        Some("running")
    );
}

/// CompleteWorkflow terminal: run row flipped to `done` and decision queue
/// row flipped to `completed` inside the same transaction.
#[tokio::test(flavor = "multi_thread")]
async fn complete_workflow_terminal_atomically_finalizes_run_and_queue() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };
    let (run_id, mut state) = seed_run(&db).await;
    let decision_task_id = run_id.clone();
    seed_queue_row(&db, &decision_task_id, &run_id).await;

    // Fold through both steps so we satisfy the completion invariant.
    state.current_step = 2;
    state.results.insert("step_sql".into(), json!({"rows": 3}));
    state
        .results
        .insert("step_fmt".into(), json!({"text": "ok"}));

    let events = vec![(
        "procedure_completed".into(),
        json!({"procedure_name": "atomic_test", "success": true}),
    )];

    let outcome = commit_decision(
        &db,
        DecisionCommit {
            run_id: run_id.clone(),
            decision_task_id: decision_task_id.clone(),
            expected_version: 0,
            new_state: state,
            events,
            attempt: 0,
            terminal: DecisionTerminal::CompleteWorkflow {
                final_answer: "[\"done\"]".into(),
            },
        },
    )
    .await
    .expect("commit_decision");

    assert!(matches!(outcome, CommitOutcome::Committed));
    assert_eq!(run_task_status(&db, &run_id).await.as_deref(), Some("done"));
    assert_eq!(
        queue_status(&db, &decision_task_id).await.as_deref(),
        Some("completed")
    );
    let run = crud::get_run(&db, &run_id).await.unwrap().unwrap();
    assert_eq!(run.answer.as_deref(), Some("[\"done\"]"));
    assert_eq!(event_types(&db, &run_id).await, vec!["procedure_completed"]);
}

/// FailWorkflow terminal: run row flipped to `failed` and decision queue
/// row flipped to `failed` atomically, with the error surfaced on both.
#[tokio::test(flavor = "multi_thread")]
async fn fail_workflow_terminal_atomically_marks_run_and_queue_failed() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };
    let (run_id, state) = seed_run(&db).await;
    let decision_task_id = run_id.clone();
    seed_queue_row(&db, &decision_task_id, &run_id).await;

    let outcome = commit_decision(
        &db,
        DecisionCommit {
            run_id: run_id.clone(),
            decision_task_id: decision_task_id.clone(),
            expected_version: 0,
            new_state: state,
            events: vec![(
                "procedure_completed".into(),
                json!({"success": false, "error": "boom"}),
            )],
            attempt: 0,
            terminal: DecisionTerminal::FailWorkflow {
                error: "boom".into(),
            },
        },
    )
    .await
    .expect("commit_decision");

    assert!(matches!(outcome, CommitOutcome::Committed));
    assert_eq!(
        run_task_status(&db, &run_id).await.as_deref(),
        Some("failed")
    );
    assert_eq!(
        queue_status(&db, &decision_task_id).await.as_deref(),
        Some("failed")
    );
    let run = crud::get_run(&db, &run_id).await.unwrap().unwrap();
    assert_eq!(run.error_message.as_deref(), Some("boom"));
}

/// Version conflict rolls back EVERYTHING: no event insert, no run/queue
/// transition. This is the invariant that makes decision tasks idempotent —
/// a retry that raced a prior worker must leave the DB untouched.
#[tokio::test(flavor = "multi_thread")]
async fn version_conflict_rolls_back_all_writes() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };
    let (run_id, mut state) = seed_run(&db).await;
    let decision_task_id = run_id.clone();
    seed_queue_row(&db, &decision_task_id, &run_id).await;

    // Simulate another worker racing ahead by bumping the decision_version to
    // 1 via a committed update.
    state.current_step = 1;
    state.results.insert("step_sql".into(), json!({"rows": 1}));
    assert!(
        agentic_workflow::extension::update_workflow_state(&db, &state)
            .await
            .unwrap()
    );
    let after_race = load_workflow_state(&db, &run_id).await.unwrap().unwrap();
    assert_eq!(after_race.decision_version, 1);

    // Our worker still holds expected_version = 0. Commit should rollback.
    state.current_step = 2;
    state
        .results
        .insert("step_fmt".into(), json!({"text": "x"}));
    let outcome = commit_decision(
        &db,
        DecisionCommit {
            run_id: run_id.clone(),
            decision_task_id: decision_task_id.clone(),
            expected_version: 0,
            new_state: state,
            events: vec![("procedure_completed".into(), json!({"success": true}))],
            attempt: 0,
            terminal: DecisionTerminal::CompleteWorkflow {
                final_answer: "stale".into(),
            },
        },
    )
    .await
    .expect("commit_decision");

    assert!(matches!(outcome, CommitOutcome::VersionConflict));

    // DB reflects the racing worker's state, NOT our conflicting commit.
    let final_state = load_workflow_state(&db, &run_id).await.unwrap().unwrap();
    assert_eq!(final_state.decision_version, 1);
    assert_eq!(final_state.current_step, 1);
    assert!(!final_state.results.contains_key("step_fmt"));

    // No event inserted; queue + run untouched.
    assert!(event_types(&db, &run_id).await.is_empty());
    assert_eq!(
        run_task_status(&db, &run_id).await.as_deref(),
        Some("running")
    );
    assert_eq!(
        queue_status(&db, &decision_task_id).await.as_deref(),
        Some("queued")
    );
}

/// Event inserts start at `max_seq + 1` — sharing a run with prior events
/// (from coordinator-emitted `delegation_completed`/`input_resolved`) must
/// not collide on `(run_id, seq)`.
#[tokio::test(flavor = "multi_thread")]
async fn events_append_after_existing_coordinator_events() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };
    let (run_id, mut state) = seed_run(&db).await;
    let decision_task_id = run_id.clone();
    seed_queue_row(&db, &decision_task_id, &run_id).await;

    // Coordinator wrote these before this decision boundary.
    crud::insert_event(&db, &run_id, 0, "delegation_completed", &json!({}), 0)
        .await
        .unwrap();
    crud::insert_event(&db, &run_id, 1, "input_resolved", &json!({}), 0)
        .await
        .unwrap();

    state.current_step = 1;
    let outcome = commit_decision(
        &db,
        DecisionCommit {
            run_id: run_id.clone(),
            decision_task_id,
            expected_version: 0,
            new_state: state,
            events: vec![
                (
                    "procedure_step_completed".into(),
                    json!({"step": "step_sql"}),
                ),
                ("procedure_step_started".into(), json!({"step": "step_fmt"})),
            ],
            attempt: 0,
            terminal: DecisionTerminal::Continuing,
        },
    )
    .await
    .expect("commit_decision");
    assert!(matches!(outcome, CommitOutcome::Committed));

    let events = crud::get_all_events(&db, &run_id).await.unwrap();
    let seqs: Vec<i64> = events.iter().map(|e| e.seq).collect();
    assert_eq!(
        seqs,
        vec![0, 1, 2, 3],
        "seq must be monotonic across writers"
    );
    let types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert_eq!(
        types,
        vec![
            "delegation_completed",
            "input_resolved",
            "procedure_step_completed",
            "procedure_step_started",
        ]
    );
}

/// Completion invariant: CompleteWorkflow with `current_step < tasks.len()`
/// is rejected as a bug and rolls back. Without this, a decider with an
/// off-by-one would strand the workflow with pending steps never running.
#[tokio::test(flavor = "multi_thread")]
async fn complete_workflow_with_pending_steps_is_rejected() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };
    let (run_id, mut state) = seed_run(&db).await;
    let decision_task_id = run_id.clone();
    seed_queue_row(&db, &decision_task_id, &run_id).await;

    // Only step 0 folded — step 1 still pending. This must not be marked complete.
    state.current_step = 1;
    state.results.insert("step_sql".into(), json!({"rows": 1}));

    let err = commit_decision(
        &db,
        DecisionCommit {
            run_id: run_id.clone(),
            decision_task_id: decision_task_id.clone(),
            expected_version: 0,
            new_state: state,
            events: vec![],
            attempt: 0,
            terminal: DecisionTerminal::CompleteWorkflow {
                final_answer: "premature".into(),
            },
        },
    )
    .await
    .expect_err("expected invariant violation");
    let msg = format!("{err:?}");
    assert!(
        msg.to_lowercase().contains("current_step"),
        "unexpected: {msg}"
    );

    // Nothing committed.
    let loaded = load_workflow_state(&db, &run_id).await.unwrap().unwrap();
    assert_eq!(loaded.decision_version, 0);
    assert_eq!(
        run_task_status(&db, &run_id).await.as_deref(),
        Some("running")
    );
    assert_eq!(
        queue_status(&db, &decision_task_id).await.as_deref(),
        Some("queued")
    );
}
