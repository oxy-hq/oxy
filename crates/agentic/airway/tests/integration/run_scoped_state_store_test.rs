//! Round-trip test for `AirwayRunScopedStateStore` — the run-scoped cursor store
//! behind mid-window backfill resume. The cursor must persist to
//! `airway_run_extensions.resume_state` and reload from it; an absent
//! resume_state must load as an EMPTY cursor (resume from the window start),
//! never the live pipeline cursor. The schema arg to `save` is ignored (a
//! backfill never writes the live schema).
//!
//! Requires Docker (or `OXY_DATABASE_URL`).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use agentic_airway::AirwayRunScopedStateStore;
use agentic_airway::extension::AirwayMigrator;
use agentic_runtime::crud;
use agentic_runtime::migration::RuntimeMigrator;
use airway::Schema;
use airway::state::{PipelineState, ResourceState, StateStore};
use sea_orm::{ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, Statement};

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
                            // 64 MB (Docker default) is too small: a parallel plan wants a 32 MB
                            // DSM segment and a REUSED container accumulates them.
                            // Must match at every setup site — reuse hashes the config.
                            // See internal-docs/workspace-source.md.
                            .with_shm_size(1024 * 1024 * 1024)
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
                tokio::time::sleep(Duration::from_millis(500)).await;
                eprintln!("test_db: attempt {attempt} failed: {e}, retrying");
            }
            Err(e) => panic!("connect to test DB failed after 10 retries: {e}"),
        }
    }
    let db = db?;
    // Central then runtime (production order — see
    // oxy_test_utils::migration), then AirwayMigrator: airway_run_extensions.run_id
    // FKs to agentic_runs.id, so it must land after runtime.
    oxy_test_utils::migration::migrate_shared_test_db::<RuntimeMigrator>(&url, &db)
        .await
        .expect("shared migrations failed")
        .then::<AirwayMigrator>()
        .await
        .expect("airway migrations failed")
        .finish()
        .await;
    Some(db)
}

/// Seed an `agentic_runs` row + its `airway_run_extensions` row (resume_state NULL).
async fn seed_run_with_extension(db: &DatabaseConnection) -> String {
    let run_id = format!("aw-rss-{}", uuid::Uuid::new_v4());
    crud::insert_run(db, &run_id, "Q", None, "airway", None, uuid::Uuid::nil())
        .await
        .unwrap();
    db.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "INSERT INTO airway_run_extensions \
             (run_id, pipeline_name, concurrency, resources) \
         VALUES ($1, 'p', 1, '[]'::jsonb)",
        [run_id.clone().into()],
    ))
    .await
    .unwrap();
    run_id
}

/// An absent resume_state loads as an empty cursor; a saved cursor round-trips
/// through resume_state on the run extension.
#[tokio::test(flavor = "multi_thread")]
async fn resume_state_round_trips_the_cursor() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };
    let db = Arc::new(db);
    let run_id = seed_run_with_extension(&db).await;
    let store = AirwayRunScopedStateStore::new(Arc::clone(&db), run_id.clone(), "p");

    // Fresh run: no resume_state yet → empty cursor (start from window start).
    let snap = store.load().await.unwrap();
    assert!(
        snap.state.resource_states.is_empty(),
        "absent resume_state must load an empty cursor"
    );

    // Persist a cursor for the `orders` resource.
    let mut state = PipelineState::default();
    state.schema_version_hash = Some("hash-1".to_string());
    state.resource_states.insert(
        "orders".to_string(),
        ResourceState {
            incremental: None,
            custom: HashMap::from([(
                "__connector_state".to_string(),
                serde_json::json!({ "high_water": "2026-06-15" }),
            )]),
        },
    );
    // Schema arg is ignored by the run-scoped store; any Schema works.
    store.save(&state, &Schema::new("p"), 0).await.unwrap();

    // A fresh store reloads the persisted cursor from resume_state.
    let store2 = AirwayRunScopedStateStore::new(Arc::clone(&db), run_id.clone(), "p");
    let snap2 = store2.load().await.unwrap();
    let orders = snap2
        .state
        .resource_states
        .get("orders")
        .expect("orders cursor must persist");
    assert_eq!(
        orders
            .custom
            .get("__connector_state")
            .and_then(|v| v.get("high_water"))
            .and_then(|v| v.as_str()),
        Some("2026-06-15"),
        "the high-water cursor must round-trip"
    );
    assert_eq!(
        snap2.state.schema_version_hash.as_deref(),
        Some("hash-1"),
        "schema_version_hash must round-trip"
    );
}
