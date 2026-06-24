//! Phase 2 scheduler backend tests (P6).
//!
//! Real Postgres via testcontainers. Covers: CRUD round-trip +
//! validation, CAS exactly-once under concurrent ticks, misfire
//! run-once-then-resume, and run-now (Global seed, cadence untouched).
//!
//! All fire-path tests use a `workflow` target: `start_automation_run`
//! validates the ref + seeds + enqueues at seed time and never touches
//! the workspace (YAML loads at drive time), so the `FakeWorkspace` stub
//! below is never actually invoked.
//!
//!   cargo nextest run -p agentic-pipeline --test scheduler_test

use std::sync::Arc;

use agentic_pipeline::scheduler::{
    ScheduleError, ScheduleInput, create_schedule, delete_schedule, get_schedule, list_schedules,
    run_schedule_now, tick_schedules, update_schedule,
};
use agentic_runtime::migration::RuntimeMigrator;
use async_trait::async_trait;
use sea_orm::{
    ConnectionTrait, Database, DatabaseConnection, DbBackend, FromQueryResult, Statement,
};
use sea_orm_migration::MigratorTrait;

static TEST_DB_URL: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
static TEST_CONTAINER: tokio::sync::OnceCell<
    Arc<testcontainers::ContainerAsync<testcontainers_modules::postgres::Postgres>>,
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
                    Arc::new(
                        Postgres::default()
                            .with_tag("18-alpine")
                            .with_reuse(ReuseDirective::Always)
                            .start()
                            .await
                            .expect("start Postgres testcontainer — is Docker running?"),
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
            Err(e) => panic!("connect after 10 retries: {e}"),
        }
    }
    let db = db?;
    RuntimeMigrator::up(&db, None)
        .await
        .expect("runtime migrations");
    Some(db)
}

/// Never actually called for `workflow` targets (see module docs); only
/// satisfies the `tick_schedules` / `run_schedule_now` signature.
struct FakeWorkspace;

#[async_trait]
impl agentic_automation::WorkspaceContext for FakeWorkspace {
    fn workspace_path(&self) -> &std::path::Path {
        std::path::Path::new("")
    }
    fn database_configs(&self) -> Vec<airlayer::DatabaseConfig> {
        vec![]
    }
    async fn get_connector(
        &self,
        _name: &str,
    ) -> Result<Arc<dyn agentic_connector::DatabaseConnector>, String> {
        Err("unused".into())
    }
    async fn get_integration(
        &self,
        _name: &str,
    ) -> Result<agentic_automation::workspace::IntegrationConfig, String> {
        Err("unused".into())
    }
    async fn list_automation_files(&self) -> Result<Vec<std::path::PathBuf>, String> {
        Ok(vec![])
    }
    async fn resolve_automation_yaml(&self, _r: &str) -> Result<String, String> {
        Err("unused".into())
    }
}

fn input(name: &str, target_ref: &str, cron_expr: &str) -> ScheduleInput {
    ScheduleInput {
        name: name.to_string(),
        target_kind: "workflow".to_string(),
        target_ref: target_ref.to_string(),
        question: None,
        variables: None,
        cron_expr: cron_expr.to_string(),
        timezone: "UTC".to_string(),
        enabled: true,
    }
}

/// Unique workflow ref so fire-path assertions can count exactly the runs
/// this test seeded (the testcontainer is shared/reused across tests).
fn uniq_ref() -> String {
    format!("workflows/sched-{}.automation.yml", uuid::Uuid::new_v4())
}

async fn force_due(db: &DatabaseConnection, id: &str, secs_ago: i64) {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE agentic_schedules SET next_run_at = now() - make_interval(secs => $1) WHERE id = $2",
        [secs_ago.into(), id.into()],
    ))
    .await
    .unwrap();
}

async fn run_count_for(db: &DatabaseConnection, workflow_ref: &str) -> i64 {
    #[derive(sea_orm::FromQueryResult)]
    struct C {
        c: i64,
    }
    C::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT count(*)::int8 AS c FROM agentic_runs WHERE question = $1",
        [format!("workflow: {workflow_ref}").into()],
    ))
    .one(db)
    .await
    .unwrap()
    .map(|r| r.c)
    .unwrap_or(0)
}

#[tokio::test]
async fn crud_round_trip_and_validation() {
    let Some(db) = test_db().await else { return };
    let r = uniq_ref();
    let ws = uuid::Uuid::new_v4();

    // Validation: bad cron, bad kind, empty name → Invalid.
    assert!(matches!(
        create_schedule(&db, ws, input("n", &r, "not a cron")).await,
        Err(ScheduleError::Invalid(_))
    ));
    let mut bad_kind = input("n", &r, "0 9 * * *");
    bad_kind.target_kind = "bogus".into();
    assert!(matches!(
        create_schedule(&db, ws, bad_kind).await,
        Err(ScheduleError::Invalid(_))
    ));
    assert!(matches!(
        create_schedule(&db, ws, input("  ", &r, "0 9 * * *")).await,
        Err(ScheduleError::Invalid(_))
    ));

    // Create → get → list.
    let created = create_schedule(&db, ws, input("daily", &r, "0 9 * * *"))
        .await
        .unwrap();
    assert!(created.next_run_at > agentic_runtime::crud::now());
    let got = get_schedule(&db, ws, &created.id).await.unwrap();
    assert_eq!(got.name, "daily");
    assert!(
        list_schedules(&db, ws)
            .await
            .unwrap()
            .iter()
            .any(|s| s.id == created.id)
    );

    // Update without cadence change → next_run_at preserved.
    let mut rename = input("renamed", &r, "0 9 * * *");
    rename.enabled = false;
    let updated = update_schedule(&db, ws, &created.id, rename).await.unwrap();
    assert_eq!(updated.name, "renamed");
    assert!(!updated.enabled);
    assert_eq!(
        updated.next_run_at, created.next_run_at,
        "rename must keep the slot"
    );

    // Update WITH cadence change → next_run_at recomputed.
    let updated2 = update_schedule(&db, ws, &created.id, input("renamed", &r, "0 * * * *"))
        .await
        .unwrap();
    assert_ne!(
        updated2.next_run_at, created.next_run_at,
        "cron change recomputes"
    );

    // Delete → gone, second delete is NotFound.
    delete_schedule(&db, ws, &created.id).await.unwrap();
    assert!(matches!(
        get_schedule(&db, ws, &created.id).await,
        Err(ScheduleError::NotFound)
    ));
    assert!(matches!(
        delete_schedule(&db, ws, &created.id).await,
        Err(ScheduleError::NotFound)
    ));
}

#[tokio::test]
async fn tick_cas_fires_exactly_once_under_concurrency() {
    let Some(db) = test_db().await else { return };
    let r = uniq_ref();
    let ws = uuid::Uuid::new_v4();
    let s = create_schedule(&db, ws, input("conc", &r, "0 9 * * *"))
        .await
        .unwrap();
    force_due(&db, &s.id, 3600).await;

    // Two concurrent ticks (simulating two replicas / overlapping cycles)
    // both see the due row; the CAS on next_run_at lets exactly one fire.
    let (db1, db2) = (db.clone(), db.clone());
    let (a, b) = tokio::join!(
        async move { tick_schedules(&db1, ws, &FakeWorkspace).await },
        async move { tick_schedules(&db2, ws, &FakeWorkspace).await },
    );

    assert_eq!(
        run_count_for(&db, &r).await,
        1,
        "exactly one run seeded for this schedule despite two ticks (a={a}, b={b})"
    );
    let after = get_schedule(&db, ws, &s.id).await.unwrap();
    assert!(after.next_run_at > agentic_runtime::crud::now());
    assert!(after.last_fired_at.is_some());
    assert!(after.last_run_id.is_some());
}

#[tokio::test]
async fn misfire_runs_once_then_resumes() {
    let Some(db) = test_db().await else { return };
    let r = uniq_ref();
    let ws = uuid::Uuid::new_v4();
    let s = create_schedule(&db, ws, input("misfire", &r, "0 9 * * *"))
        .await
        .unwrap();
    // A week overdue (many missed daily slots).
    force_due(&db, &s.id, 7 * 24 * 3600).await;

    tick_schedules(&db, ws, &FakeWorkspace).await;

    assert_eq!(
        run_count_for(&db, &r).await,
        1,
        "missed slots collapse to a single catch-up run"
    );
    let after = get_schedule(&db, ws, &s.id).await.unwrap();
    assert!(
        after.next_run_at > agentic_runtime::crud::now(),
        "next_run_at jumps to the next future slot, not the next missed one"
    );

    // A second immediate tick must NOT re-fire (already advanced).
    tick_schedules(&db, ws, &FakeWorkspace).await;
    assert_eq!(run_count_for(&db, &r).await, 1, "no double fire");
}

#[tokio::test]
async fn run_now_seeds_global_without_advancing() {
    let Some(db) = test_db().await else { return };
    let r = uniq_ref();
    let ws = uuid::Uuid::new_v4();
    let s = create_schedule(&db, ws, input("runnow", &r, "0 9 * * *"))
        .await
        .unwrap();
    let before_next = s.next_run_at;

    let run_id = run_schedule_now(&db, ws, &FakeWorkspace, &s.id)
        .await
        .unwrap();
    assert_eq!(run_count_for(&db, &r).await, 1);

    // The seeded run is Global (scope_owned=false), driverless.
    #[derive(sea_orm::FromQueryResult)]
    struct Row {
        scope_owned: bool,
        queue_status: String,
    }
    let q = Row::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT scope_owned, queue_status FROM agentic_task_queue WHERE task_id = $1",
        [run_id.clone().into()],
    ))
    .one(&db)
    .await
    .unwrap()
    .expect("queue row for the seeded run");
    assert!(!q.scope_owned, "run-now seeds a Global task");
    assert_eq!(q.queue_status, "queued");

    // Cadence untouched: run-now is out-of-band.
    let after = get_schedule(&db, ws, &s.id).await.unwrap();
    assert_eq!(
        after.next_run_at, before_next,
        "run-now must not advance next_run_at"
    );
    assert_eq!(after.last_run_id.as_deref(), Some(run_id.as_str()));
}

/// §12 FU4b: a tick for workspace A must fire only A's due schedule and
/// leave B's untouched, and vice versa. Asserts CRUD + tick scoping is
/// truly per-workspace.
#[tokio::test]
async fn multi_tenant_tick_isolation() {
    let Some(db) = test_db().await else { return };
    let ws_a = uuid::Uuid::new_v4();
    let ws_b = uuid::Uuid::new_v4();
    let r_a = uniq_ref();
    let r_b = uniq_ref();

    let s_a = create_schedule(&db, ws_a, input("A", &r_a, "0 9 * * *"))
        .await
        .unwrap();
    let s_b = create_schedule(&db, ws_b, input("B", &r_b, "0 9 * * *"))
        .await
        .unwrap();
    force_due(&db, &s_a.id, 3600).await;
    force_due(&db, &s_b.id, 3600).await;

    // Tick A only.
    let fired_a = tick_schedules(&db, ws_a, &FakeWorkspace).await;

    // A fired exactly once for r_a; r_b untouched.
    assert_eq!(run_count_for(&db, &r_a).await, 1);
    assert_eq!(run_count_for(&db, &r_b).await, 0);
    assert!(fired_a >= 1);
    let after_a = get_schedule(&db, ws_a, &s_a.id).await.unwrap();
    let after_b_pre = get_schedule(&db, ws_b, &s_b.id).await.unwrap();
    assert!(after_a.last_fired_at.is_some(), "A's tick fired A");
    assert!(
        after_b_pre.last_fired_at.is_none(),
        "A's tick must not touch B"
    );

    // Cross-workspace CRUD: get/update/delete of B from ws_a is NotFound,
    // never leaks the existence of another workspace's schedule.
    assert!(matches!(
        get_schedule(&db, ws_a, &s_b.id).await,
        Err(ScheduleError::NotFound)
    ));
    assert!(matches!(
        delete_schedule(&db, ws_a, &s_b.id).await,
        Err(ScheduleError::NotFound)
    ));
    assert!(matches!(
        update_schedule(&db, ws_a, &s_b.id, input("hijack", &r_b, "0 9 * * *")).await,
        Err(ScheduleError::NotFound)
    ));

    // Now tick B → fires B; A's count unchanged.
    let fired_b = tick_schedules(&db, ws_b, &FakeWorkspace).await;
    assert_eq!(run_count_for(&db, &r_b).await, 1);
    assert_eq!(run_count_for(&db, &r_a).await, 1, "A unchanged");
    assert!(fired_b >= 1);
    let after_b = get_schedule(&db, ws_b, &s_b.id).await.unwrap();
    assert!(after_b.last_fired_at.is_some());

    // list_schedules is scoped: each workspace only sees its own row.
    let list_a = list_schedules(&db, ws_a).await.unwrap();
    let list_b = list_schedules(&db, ws_b).await.unwrap();
    assert!(list_a.iter().all(|s| s.workspace_id == ws_a));
    assert!(list_b.iter().all(|s| s.workspace_id == ws_b));
    assert!(list_a.iter().any(|s| s.id == s_a.id));
    assert!(!list_a.iter().any(|s| s.id == s_b.id));
    assert!(list_b.iter().any(|s| s.id == s_b.id));
}

// ── Agent target kind ────────────────────────────────────────────────────────

fn agent_input(name: &str, agent_ref: &str, question: &str, cron_expr: &str) -> ScheduleInput {
    ScheduleInput {
        name: name.to_string(),
        target_kind: "agent".to_string(),
        target_ref: agent_ref.to_string(),
        question: Some(question.to_string()),
        variables: None,
        cron_expr: cron_expr.to_string(),
        timezone: "UTC".to_string(),
        enabled: true,
    }
}

/// `target_kind="agent"` requires a non-empty `question`. Empty / missing
/// surfaces as `Invalid` from `create_schedule` before any DB write.
#[tokio::test]
async fn agent_schedule_requires_question() {
    let Some(db) = test_db().await else { return };
    let ws = uuid::Uuid::new_v4();

    // Missing question entirely.
    let mut no_q = agent_input("nq", "agents/foo", "", "0 9 * * *");
    no_q.question = None;
    assert!(matches!(
        create_schedule(&db, ws, no_q).await,
        Err(ScheduleError::Invalid(_))
    ));

    // Whitespace-only question.
    assert!(matches!(
        create_schedule(&db, ws, agent_input("ws", "agents/foo", "   ", "0 9 * * *")).await,
        Err(ScheduleError::Invalid(_))
    ));

    // Workflow / airway schedules MAY omit `question`.
    let mut wf = input("wf", &uniq_ref(), "0 9 * * *");
    wf.question = None;
    create_schedule(&db, ws, wf)
        .await
        .expect("workflow schedule must not require question");
}

/// `run_schedule_now` for an `agent` schedule seeds an analytics run:
/// `agentic_runs` row with `source_type="analytics"`, `schedule_id`
/// linked back to the schedule, metadata stamped with `agent_id` +
/// `question`, and an `analytics_run_extensions` row with the
/// `agent_id`. The queue row is `Global` (driverless) like the
/// workflow/airway paths.
#[tokio::test]
async fn agent_run_now_seeds_analytics_run() {
    let Some(db) = test_db().await else { return };
    let ws = uuid::Uuid::new_v4();
    // Use a unique agent_id so we can isolate this run from sibling tests
    // running against the shared testcontainer.
    let agent_id = format!("agents/sched-{}.agentic.yml", uuid::Uuid::new_v4());
    let question = "What is yesterday's revenue?";

    let s = create_schedule(
        &db,
        ws,
        agent_input("agent-nightly", &agent_id, question, "0 9 * * *"),
    )
    .await
    .unwrap();
    let before_next = s.next_run_at;

    let run_id = run_schedule_now(&db, ws, &FakeWorkspace, &s.id)
        .await
        .expect("run_now succeeds");

    // The agentic_runs row: analytics source, linked to the schedule,
    // question stored verbatim, metadata.agent_id populated.
    #[derive(FromQueryResult)]
    struct RunRow {
        source_type: Option<String>,
        schedule_id: Option<String>,
        question: String,
        metadata: Option<serde_json::Value>,
    }
    let row = RunRow::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT source_type, schedule_id, question, metadata \
         FROM agentic_runs WHERE id = $1",
        [run_id.clone().into()],
    ))
    .one(&db)
    .await
    .unwrap()
    .expect("run row exists for the seeded agent run");
    assert_eq!(row.source_type.as_deref(), Some("analytics"));
    assert_eq!(row.schedule_id.as_deref(), Some(s.id.as_str()));
    assert_eq!(row.question, question);
    let meta = row.metadata.expect("metadata stamped");
    assert_eq!(
        meta.get("agent_id").and_then(|v| v.as_str()),
        Some(agent_id.as_str())
    );
    assert_eq!(
        meta.get("question").and_then(|v| v.as_str()),
        Some(question)
    );
    // `run_now` stamps trigger="manual" (vs "scheduled" / "backfill").
    assert_eq!(meta.get("trigger").and_then(|v| v.as_str()), Some("manual"));

    // analytics_run_extensions row: agent_id matches what start_agent_run
    // wrote, no spec_hint yet (the run hasn't executed).
    #[derive(FromQueryResult)]
    struct Ext {
        agent_id: String,
    }
    let ext = Ext::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT agent_id FROM analytics_run_extensions WHERE run_id = $1",
        [run_id.clone().into()],
    ))
    .one(&db)
    .await
    .unwrap()
    .expect("analytics extension row exists");
    assert_eq!(ext.agent_id, agent_id);

    // Queue row: Global (scope_owned=false), queued — driven by the
    // standalone consumer, not a co-located coordinator.
    #[derive(FromQueryResult)]
    struct Q {
        scope_owned: bool,
        queue_status: String,
    }
    let q = Q::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT scope_owned, queue_status FROM agentic_task_queue WHERE task_id = $1",
        [run_id.clone().into()],
    ))
    .one(&db)
    .await
    .unwrap()
    .expect("queue row for the seeded run");
    assert!(!q.scope_owned, "agent run-now seeds a Global task");
    assert_eq!(q.queue_status, "queued");

    // Cadence is untouched — `run_now` is out-of-band — and `last_run_id`
    // points at the freshly seeded run.
    let after = get_schedule(&db, ws, &s.id).await.unwrap();
    assert_eq!(
        after.next_run_at, before_next,
        "agent run-now must not advance next_run_at"
    );
    assert_eq!(after.last_run_id.as_deref(), Some(run_id.as_str()));
}

/// Scheduler tick for a due agent schedule fires `start_agent_run` and
/// links the seeded run back via `schedule_id`. Confirms the agent arm
/// of `fire_schedule` participates in the CAS / cadence-advance flow.
#[tokio::test]
async fn agent_schedule_tick_fires_once() {
    let Some(db) = test_db().await else { return };
    let ws = uuid::Uuid::new_v4();
    let agent_id = format!("agents/tick-{}.agentic.yml", uuid::Uuid::new_v4());

    let s = create_schedule(
        &db,
        ws,
        agent_input("agent-tick", &agent_id, "Daily standup", "0 9 * * *"),
    )
    .await
    .unwrap();
    force_due(&db, &s.id, 3600).await;

    let fired = tick_schedules(&db, ws, &FakeWorkspace).await;
    assert!(fired >= 1, "tick fired at least one schedule");

    // Exactly one analytics run linked to this schedule.
    #[derive(FromQueryResult)]
    struct Row {
        id: String,
        source_type: Option<String>,
        trigger: Option<String>,
    }
    let runs = Row::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT id, source_type, metadata->>'trigger' AS trigger \
         FROM agentic_runs WHERE schedule_id = $1",
        [s.id.clone().into()],
    ))
    .all(&db)
    .await
    .unwrap();
    assert_eq!(runs.len(), 1, "exactly one run seeded for this schedule");
    assert_eq!(runs[0].source_type.as_deref(), Some("analytics"));
    // Tick stamps trigger="scheduled" (vs "manual" / "backfill").
    assert_eq!(runs[0].trigger.as_deref(), Some("scheduled"));

    // Cadence advanced past now.
    let after = get_schedule(&db, ws, &s.id).await.unwrap();
    assert!(after.next_run_at > agentic_runtime::crud::now());
    assert!(after.last_fired_at.is_some());
    assert_eq!(after.last_run_id.as_deref(), Some(runs[0].id.as_str()));
}
