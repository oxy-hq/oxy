//! Facade integration test for `start_airway_run`.
//!
//! The worker path is covered by `agentic-airway`'s
//! `worker_integration`. This pins the *seeding* half: that
//! `start_airway_run` resolves + renders + validates the `.airway.yml`,
//! and atomically lands the three rows the coordinator needs —
//! `agentic_runs` (source_type=airway), `airway_run_extensions`, and
//! a queued `TaskSpec::Airway` in `agentic_task_queue`.
//!
//! Requires Docker (or `OXY_DATABASE_URL`); self-skips otherwise.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentic_airway::AirwayMigrator;
use agentic_airway::extension::run_extension;
use agentic_core::delegation::TaskSpec;
use agentic_pipeline::airway_run::{StartAirwayRequest, start_airway_run};
use agentic_runtime::crud;
use agentic_runtime::migration::RuntimeMigrator;
use async_trait::async_trait;
use sea_orm::{Database, DatabaseConnection};
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
    // RuntimeMigrator first — `airway_run_extensions.run_id` and the
    // queue row both FK / key off `agentic_runs`.
    RuntimeMigrator::up(&db, None)
        .await
        .expect("runtime migrations");
    AirwayMigrator::up(&db, None)
        .await
        .expect("airway migrations");
    Some(db)
}

/// Minimal `WorkspaceContext` whose `workspace_path` points at a real
/// temp dir so `start_airway_run` can read the `.airway.yml`. Every
/// other method is unreachable on this path.
struct TmpWorkspace {
    root: PathBuf,
}

#[async_trait]
impl agentic_pipeline::platform::ProjectContext for TmpWorkspace {
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
impl agentic_workflow::WorkspaceContext for TmpWorkspace {
    fn workspace_path(&self) -> &Path {
        &self.root
    }
    fn database_configs(&self) -> Vec<airlayer::DatabaseConfig> {
        vec![]
    }
    async fn get_connector(
        &self,
        name: &str,
    ) -> Result<Arc<dyn agentic_connector::DatabaseConnector>, String> {
        Err(format!("tmp workspace: connector '{name}' unavailable"))
    }
    async fn get_integration(
        &self,
        name: &str,
    ) -> Result<agentic_workflow::workspace::IntegrationConfig, String> {
        Err(format!("tmp workspace: integration '{name}' unavailable"))
    }
    async fn list_workflow_files(&self) -> Result<Vec<PathBuf>, String> {
        Ok(vec![])
    }
    async fn resolve_workflow_yaml(&self, _workflow_ref: &str) -> Result<String, String> {
        Err("tmp workspace: not available".into())
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn start_airway_run_seeds_run_extension_and_queue() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };

    let dir = tempfile::tempdir().expect("tempdir");
    let pipeline_name = format!("facade_{}", uuid::Uuid::new_v4().simple());

    // `{{ env }}` exercises templating *through the facade* — proves
    // `start_airway_run` renders before persisting.
    let yaml = format!(
        r#"
name: {pipeline_name}_{{{{ env }}}}
source:
  kind: filesystem
  config:
    base_path: /tmp/airway-facade
    pattern: "*.jsonl"
    format: jsonl
    table_name: users
destination:
  kind: memory
  config:
    dataset_name: scratch
concurrency: 3
resources:
  - users
"#
    );
    std::fs::write(dir.path().join("p.airway.yml"), yaml).expect("write spec");

    let ws = TmpWorkspace {
        root: dir.path().to_path_buf(),
    };
    let request = StartAirwayRequest {
        pipeline_ref: "p.airway.yml".to_string(),
        variables: Some(serde_json::json!({ "env": "prod" })),
        thread_id: None,
    };

    let run_id = start_airway_run(&db, &ws, request)
        .await
        .expect("start_airway_run");

    let rendered_name = format!("{pipeline_name}_prod");

    // ── agentic_runs row ─────────────────────────────────────────────────
    let run = crud::get_run(&db, &run_id)
        .await
        .expect("query run")
        .expect("agentic_runs row exists");
    assert_eq!(run.source_type.as_deref(), Some("airway"));
    assert_eq!(run.question, format!("airway: {rendered_name}"));

    // ── airway_run_extensions row ────────────────────────────────────────
    let ext = run_extension::get_run_extension(&db, &run_id)
        .await
        .expect("query extension")
        .expect("airway_run_extensions row exists");
    assert_eq!(ext.pipeline_name, rendered_name);
    assert_eq!(ext.pipeline_ref.as_deref(), Some("p.airway.yml"));
    assert_eq!(ext.concurrency, 3);
    assert_eq!(
        ext.resources,
        serde_json::json!(["users"]),
        "selected resources persisted verbatim"
    );

    // ── queued TaskSpec::Airway ──────────────────────────────────────────
    let entry = crud::get_queue_entry(&db, &run_id)
        .await
        .expect("query queue")
        .expect("queue row exists");
    let spec: TaskSpec = serde_json::from_value(entry.spec).expect("deserialize spec");
    match spec {
        TaskSpec::Airway {
            pipeline_ref,
            variables,
        } => {
            assert_eq!(pipeline_ref, "p.airway.yml");
            assert_eq!(
                variables.and_then(|v| v.get("env").cloned()),
                Some(serde_json::json!("prod")),
            );
        }
        other => panic!("expected TaskSpec::Airway, got {other:?}"),
    }

    // ── run-history list surfaces this run ───────────────────────────────
    let runs = agentic_pipeline::airway_run::list_airway_runs(&db, "p.airway.yml", 50)
        .await
        .expect("list_airway_runs");
    assert!(
        runs.iter().any(|r| r.run_id == run_id),
        "started run must appear in the pipeline's run history"
    );
    // A different pipeline_ref must not match.
    let other = agentic_pipeline::airway_run::list_airway_runs(&db, "nope.airway.yml", 50)
        .await
        .expect("list_airway_runs other");
    assert!(
        !other.iter().any(|r| r.run_id == run_id),
        "run must not leak into an unrelated pipeline's history"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn start_airway_run_rejects_missing_pipeline_file() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let ws = TmpWorkspace {
        root: dir.path().to_path_buf(),
    };
    let request = StartAirwayRequest {
        pipeline_ref: "does-not-exist.airway.yml".to_string(),
        variables: None,
        thread_id: None,
    };
    let err = start_airway_run(&db, &ws, request)
        .await
        .expect_err("missing file must error before any DB write");
    assert!(
        err.to_string().contains("does-not-exist.airway.yml"),
        "got: {err}"
    );
}
