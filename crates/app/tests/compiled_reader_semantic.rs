//! Regression test for the semantic-view / semantic-topic compile-boundary
//! readers.
//!
//! `resolve_semantic_view` / `resolve_semantic_topic` must key the compiled row
//! by its workspace-relative `file_path`, NOT the primary-key `name` (the YAML
//! `name:` field). Their only callers (the IDE preview via
//! `materialise_semantic_entity`) pass the file path. Keying by `name` made the
//! lookup miss for every view/topic whose `name:` differs from its path — i.e.
//! all of them — so the read fell through to the working-copy filesystem, which
//! a stateless serve replica doesn't have ("Failed to canonicalize project
//! path: No such file or directory"). See oxygen-internal#2613.
//!
//! Spins up a Postgres testcontainer (or reuses `OXY_DATABASE_URL` in CI),
//! applies the central migrator, seeds a promoted workspace with a view + topic
//! whose `name` ≠ `file_path`, then drives the real readers end-to-end.

use std::sync::Arc;

use entity::workspaces::WorkspaceStatus;
use entity::{organizations, revisions, semantic_topics, semantic_views, workspaces};
use migration::{Migrator, MigratorTrait};
use oxy_app::server::api::compiled_reader::{resolve_semantic_topic, resolve_semantic_view};
use sea_orm::{
    ActiveModelTrait, ActiveValue, ConnectionTrait, Database, DatabaseConnection, EntityTrait,
};
use serde_json::json;
use uuid::Uuid;

static TEST_DB_URL: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
static TEST_CONTAINER: tokio::sync::OnceCell<
    Arc<testcontainers::ContainerAsync<testcontainers_modules::postgres::Postgres>>,
> = tokio::sync::OnceCell::const_new();

/// Per-test database on a shared Postgres testcontainer, migrated and wired so
/// that `establish_connection()` (used by the readers under test) points at it.
async fn setup_db() -> DatabaseConnection {
    let admin_url = TEST_DB_URL
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

    let mut admin = None;
    for attempt in 0..10 {
        match Database::connect(&admin_url).await {
            Ok(c) => {
                admin = Some(c);
                break;
            }
            Err(e) if attempt < 9 => {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                eprintln!("admin connect attempt {attempt} failed: {e}");
            }
            Err(e) => panic!("admin connect: {e}"),
        }
    }
    let admin = admin.unwrap();

    let db_name = format!("csr_{}", Uuid::new_v4().simple());
    admin
        .execute_unprepared(&format!("CREATE DATABASE \"{db_name}\""))
        .await
        .expect("create per-test database");

    // Replace only the trailing /<dbname>, not occurrences inside the userinfo.
    let test_url = match admin_url.rfind('/') {
        Some(pos) => format!("{}/{db_name}", &admin_url[..pos]),
        None => panic!("admin_url missing path: {admin_url}"),
    };

    // The readers resolve their own connection via `establish_connection()`,
    // which reads `OXY_DATABASE_URL` once per process. nextest isolates each
    // test in its own process, so pointing it at the per-test DB here is safe.
    // SAFETY: single-threaded test setup before any other env access.
    unsafe {
        std::env::set_var("OXY_DATABASE_URL", &test_url);
        std::env::remove_var("OXY_DATABASE_AUTH_MODE");
    }

    let db = Database::connect(&test_url)
        .await
        .expect("connect to per-test database");
    Migrator::up(&db, None).await.expect("run migrations");
    db
}

/// Seed an org + a **promoted** workspace (a `revisions` row set as
/// `current_revision_id`) carrying one semantic view and one topic whose
/// `name` deliberately differs from its `file_path`. Returns the workspace id.
async fn seed_promoted_workspace(db: &DatabaseConnection) -> Uuid {
    let now = chrono::Utc::now().fixed_offset();

    let org_id = Uuid::new_v4();
    organizations::ActiveModel {
        id: ActiveValue::Set(org_id),
        name: ActiveValue::Set("csr-org".into()),
        slug: ActiveValue::Set(format!("csr-{}", org_id.simple())),
        logo: ActiveValue::NotSet,
        logo_content_type: ActiveValue::NotSet,
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
    }
    .insert(db)
    .await
    .expect("seed org");

    let ws_id = Uuid::new_v4();
    workspaces::ActiveModel {
        id: ActiveValue::Set(ws_id),
        name: ActiveValue::Set("csr-ws".into()),
        git_namespace_id: ActiveValue::Set(None),
        git_remote_url: ActiveValue::Set(None),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
        path: ActiveValue::Set(None),
        last_opened_at: ActiveValue::Set(None),
        created_by: ActiveValue::Set(None),
        org_id: ActiveValue::Set(Some(org_id)),
        status: ActiveValue::Set(WorkspaceStatus::Ready),
        error: ActiveValue::Set(None),
        monthly_vlm_budget_micros: ActiveValue::Set(None),
        current_revision_id: ActiveValue::Set(None),
    }
    .insert(db)
    .await
    .expect("seed workspace");

    let rev_id = Uuid::new_v4();
    revisions::ActiveModel {
        revision_id: ActiveValue::Set(rev_id),
        workspace_id: ActiveValue::Set(ws_id),
        git_sha: ActiveValue::Set("deadbeef".into()),
        branch: ActiveValue::Set(Some("main".into())),
        schema_version: ActiveValue::Set(1),
        status: ActiveValue::Set("ready".into()),
        kind: ActiveValue::Set("full".into()),
        owner_user_id: ActiveValue::Set(None),
        compiler_version: ActiveValue::Set("test".into()),
        started_at: ActiveValue::Set(now),
        finished_at: ActiveValue::Set(Some(now)),
        file_count_seen: ActiveValue::Set(2),
        file_count_compiled: ActiveValue::Set(2),
        file_count_failed: ActiveValue::Set(0),
        error_summary: ActiveValue::Set(None),
    }
    .insert(db)
    .await
    .expect("seed revision");

    // Promote the revision (FK requires the revision to exist first).
    let mut ws: workspaces::ActiveModel = workspaces::Entity::find_by_id(ws_id)
        .one(db)
        .await
        .expect("load workspace")
        .expect("workspace exists")
        .into();
    ws.current_revision_id = ActiveValue::Set(Some(rev_id));
    ws.update(db).await.expect("promote revision");

    // name = "oxymart", file_path = "semantics/views/oxymart.view.yml" — the
    // mismatch that broke the old PK-keyed lookup.
    semantic_views::ActiveModel {
        revision_id: ActiveValue::Set(rev_id),
        name: ActiveValue::Set("oxymart".into()),
        file_path: ActiveValue::Set("semantics/views/oxymart.view.yml".into()),
        definition: ActiveValue::Set(json!({ "name": "oxymart", "table": "orders" })),
        compiled_sql_blob_key: ActiveValue::Set(None),
    }
    .insert(db)
    .await
    .expect("seed semantic view");

    semantic_topics::ActiveModel {
        revision_id: ActiveValue::Set(rev_id),
        name: ActiveValue::Set("sales".into()),
        file_path: ActiveValue::Set("semantics/topics/sales.topic.yml".into()),
        definition: ActiveValue::Set(json!({ "name": "sales" })),
        compiled_sql_blob_key: ActiveValue::Set(None),
    }
    .insert(db)
    .await
    .expect("seed semantic topic");

    ws_id
}

#[tokio::test]
async fn semantic_view_and_topic_resolve_by_file_path_not_name() {
    let db = setup_db().await;
    let ws = seed_promoted_workspace(&db).await;

    // ── view ────────────────────────────────────────────────────────────────
    // Resolves by its workspace-relative path (what the IDE preview passes).
    let view = resolve_semantic_view(ws, None, "semantics/views/oxymart.view.yml")
        .await
        .expect("view query")
        .expect("view must resolve from the compile boundary by file_path");
    assert_eq!(view.name, "oxymart");
    assert_eq!(view.file_path, "semantics/views/oxymart.view.yml");

    // The YAML `name:` must NOT be a valid lookup key — that was the old,
    // broken `find_by_id((revision, name))` behaviour that always missed.
    assert!(
        resolve_semantic_view(ws, None, "oxymart")
            .await
            .expect("view-by-name query")
            .is_none(),
        "the view name must not resolve a row — the reader keys by file_path"
    );

    // An uncompiled / unknown path is a clean miss (lets the caller fall back),
    // not an error.
    assert!(
        resolve_semantic_view(ws, None, "semantics/views/missing.view.yml")
            .await
            .expect("missing-view query")
            .is_none()
    );

    // ── topic ───────────────────────────────────────────────────────────────
    let topic = resolve_semantic_topic(ws, None, "semantics/topics/sales.topic.yml")
        .await
        .expect("topic query")
        .expect("topic must resolve from the compile boundary by file_path");
    assert_eq!(topic.name, "sales");
    assert_eq!(topic.file_path, "semantics/topics/sales.topic.yml");

    assert!(
        resolve_semantic_topic(ws, None, "sales")
            .await
            .expect("topic-by-name query")
            .is_none(),
        "the topic name must not resolve a row — the reader keys by file_path"
    );
}
