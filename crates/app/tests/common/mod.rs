//! Shared harness for seed-driven integration tests.
//!
//! Testcontainers, falling back to `OXY_DATABASE_URL` when it's set (CI's
//! service container). Testcontainers rather than an env-gated skip because a
//! skip means a broken seed passes on a laptop and only fails after push.
#![allow(dead_code)] // Each test binary uses a different subset.

use std::path::{Path, PathBuf};

use migration::{Migrator, MigratorTrait};
use sea_orm::{Database, DatabaseConnection};
use uuid::Uuid;

/// Keeps the state dir alive for the whole test binary. Dropping it would
/// delete the bundle bytes mid-test.
static STATE_DIR: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
static TEST_DB_URL: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
static TEST_CONTAINER: tokio::sync::OnceCell<
    std::sync::Arc<testcontainers::ContainerAsync<testcontainers_modules::postgres::Postgres>>,
> = tokio::sync::OnceCell::const_new();

/// The demo project the seed points workspaces at.
pub fn examples_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// The demo workspace id `seed_demo` derives — UUID v5 of "demo.oxy.local" in
/// the DNS namespace. Re-derived rather than imported because the seed's helper
/// is private; if that derivation ever changes, this fails loudly, which is the
/// point (the id is documented as stable so saved IDE state stays valid).
pub fn demo_workspace_id() -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_DNS, b"demo.oxy.local")
}

pub const APP_SLUG: &str = "oxy-starter";

/// A migrated, per-test database, with the process pointed at it.
pub async fn test_db() -> DatabaseConnection {
    let admin_url = TEST_DB_URL
        .get_or_init(|| async {
            if let Ok(url) = std::env::var("OXY_DATABASE_URL") {
                return url; // CI: reuse the service container.
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
                            .expect("start postgres testcontainer (is Docker running?)"),
                    )
                })
                .await;
            let port = container
                .get_host_port_ipv4(5432_u16)
                .await
                .expect("get postgres port");
            format!("postgresql://postgres:postgres@127.0.0.1:{port}/postgres")
        })
        .await
        .clone();

    let admin = Database::connect(&admin_url)
        .await
        .expect("connect to admin database");
    let db_name = format!("seed_app_{}", Uuid::new_v4().simple());
    sea_orm::ConnectionTrait::execute_unprepared(&admin, &format!("CREATE DATABASE \"{db_name}\""))
        .await
        .expect("create per-test database");
    let test_url = match admin_url.rfind('/') {
        Some(pos) => format!("{}/{db_name}", &admin_url[..pos]),
        None => panic!("admin_url missing path"),
    };

    let state_dir = STATE_DIR.get_or_init(|| tempfile::tempdir().expect("state dir"));

    // SAFETY: single-threaded setup, before anything else touches the env.
    // `establish_connection()` is a process-wide OnceCell that reads
    // OXY_DATABASE_URL once, and nextest gives each test binary its own
    // process — so this is what points the seed's own connection at our DB.
    unsafe {
        std::env::set_var("OXY_DATABASE_URL", &test_url);
        // The seed writes real bundle bytes. Without this it would write them
        // into the developer's actual state dir.
        std::env::set_var("OXY_STATE_DIR", state_dir.path());
        // establish_connection branches on auth mode; an inherited IAM setting
        // connects to nothing.
        std::env::remove_var("OXY_DATABASE_AUTH_MODE");
        // The build store refuses a filesystem write unless the role is `all`,
        // and picks S3 whenever a bucket is set. Either would fail the seed for
        // reasons that have nothing to do with the code under test.
        std::env::remove_var("OXY_ROLE");
        std::env::remove_var("OXY_CUSTOMER_APPS_S3_BUCKET");
        // Platform standing is read from these. A developer with either set in
        // their shell (`.env` binds them at seed time) would make the test user
        // STAFF — and the staff path in `user_can_access_app` short-circuits
        // before the org-membership check, so the multi-tenant assertions would
        // pass for the wrong reason, or fail only on someone else's machine.
        std::env::remove_var("OXY_OWNER");
        std::env::remove_var("OXY_GLOBAL_ADMINS");
        std::env::remove_var("OXY_APP_ADMINS");
    }

    let db = Database::connect(&test_url)
        .await
        .expect("connect to per-test database");
    Migrator::up(&db, None).await.expect("run migrations");
    db
}
