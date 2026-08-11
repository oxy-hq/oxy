//! Non-airhouse control flow of the reset-schema helpers
//! ([`agentic_airway::reset`]) — the parts cheaply testable against the oxy DB
//! without a live destination:
//! - `clear_pipeline_state` is idempotent (deleting an absent row is a no-op),
//! - `stored_schema_table_names` yields `[]` when no state row exists — the
//!   empty-tables early-return the executor relies on to skip destination
//!   resolution.
//!
//! Requires Docker (or `OXY_DATABASE_URL`).

use std::sync::Arc;
use std::time::Duration;

use agentic_airway::extension::AirwayMigrator;
use agentic_runtime::migration::RuntimeMigrator;
use sea_orm::{Database, DatabaseConnection};

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

/// Deleting the state row for a pipeline that has none is a no-op, not an error
/// — the executor calls this unconditionally, including on the
/// never-provisioned path, so it must tolerate an absent row (and repeats).
#[tokio::test(flavor = "multi_thread")]
async fn clear_pipeline_state_is_idempotent_on_absent_row() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };
    let name = format!("reset-absent-{}", uuid::Uuid::new_v4());
    // No row was ever written for `name`.
    agentic_airway::reset::clear_pipeline_state(&db, &name)
        .await
        .expect("clearing an absent state row must be a no-op");
    // A second call is still a no-op.
    agentic_airway::reset::clear_pipeline_state(&db, &name)
        .await
        .expect("second clear is still a no-op");
}

/// A pipeline with no `airway_pipeline_state` row has no stored schema, so a
/// reset has nothing to drop — this drives the executor's empty-tables
/// early-return (clear state, skip destination resolution).
#[tokio::test(flavor = "multi_thread")]
async fn stored_schema_table_names_empty_when_no_row() {
    let Some(db) = test_db().await else {
        eprintln!("skipping: no DB available");
        return;
    };
    let name = format!("reset-noschema-{}", uuid::Uuid::new_v4());
    let tables = agentic_airway::reset::stored_schema_table_names(&db, &name)
        .await
        .expect("an absent state row loads a default (empty) snapshot, not an error");
    assert!(
        tables.is_empty(),
        "no state row ⇒ no stored schema ⇒ nothing to drop, got: {tables:?}"
    );
}
