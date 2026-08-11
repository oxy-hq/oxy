//! Regression test for the airway run path on the compile boundary.
//!
//! `execute_airway` / `reset_airway_schema` claim a queued `TaskSpec::Airway`
//! on the **durable worker fleet**, which is stateless and owns no working
//! copy. Resolving the pipeline's `.airway.yml` by walking the workspace
//! filesystem there is the instance-affinity failure mode: the read fails with
//! "workspace directory not found" / a spurious missing-pipeline error on a
//! replica, while it works on the node that happens to hold the checkout.
//!
//! `.airway.yml` IS compiled — one `airway_pipelines` row per file, keyed by
//! `revision_id`. These tests drive the real production path end-to-end:
//!
//!   `pipeline_ref::load_pipeline_yaml`  (containment guard, agentic-pipeline)
//!     → `WorkspaceContext::resolve_pipeline_yaml`   (port)
//!       → `OxyProjectContext`                        (host adapter, oxy-app)
//!         → `compiled_reader::resolve_pipeline`      (open_compiled_revision)
//!
//! with the workspace directory **absent from disk entirely**, which is what
//! makes them fail before the change and pass after.
//!
//! Database-backed through [`common::fresh_db`] — own database per test, so
//! this module belongs to `db-per-test` in `.config/nextest.toml`.

use std::path::PathBuf;

use entity::workspaces::WorkspaceStatus;
use entity::{airway_pipelines, organizations, revisions, workspaces};
use oxy::adapters::workspace::builder::WorkspaceBuilder;
use oxy_app::agentic_wiring::OxyProjectContext;
use oxy_app::server::api::compiled_reader::resolve_pipeline;
use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseConnection, EntityTrait};
use serde_json::json;
use uuid::Uuid;

/// Per-test database, migrated and wired so that `establish_connection()`
/// (used inside `compiled_reader`) points at it.
///
/// [`common::fresh_db`] rather than a hand-rolled harness, exactly as
/// `toast_webhook_compile_boundary` does: it names the database with the
/// `oxytest_` prefix and the `NEXTEST_RUN_ID` tag that `drop_stale_databases`
/// needs in order to tell a live sibling from a stray, and it asserts
/// process-per-test before the `set_var` below — which is only sound under that
/// isolation. The chain also runs `Migrator::up` once per `cargo nextest run`
/// into a template this clones, instead of once per test.
async fn setup_db() -> DatabaseConnection {
    let (db, test_url) = crate::common::fresh_db(crate::common::Schema::Central).await;
    // SAFETY: single-threaded test setup before any other env access. nextest
    // isolates each test in its own process, so pointing the process-wide
    // OnceCell at the per-test DB here is safe.
    unsafe {
        std::env::set_var("OXY_DATABASE_URL", &test_url);
        std::env::remove_var("OXY_DATABASE_AUTH_MODE");
    }
    db
}

/// Seed an org + a **promoted** workspace (a `revisions` row set as
/// `current_revision_id`) with no pipelines yet. Returns `(workspace_id,
/// revision_id)`.
async fn seed_promoted_workspace(db: &DatabaseConnection) -> (Uuid, Uuid) {
    let now = chrono::Utc::now().fixed_offset();

    let org_id = Uuid::new_v4();
    organizations::ActiveModel {
        id: ActiveValue::Set(org_id),
        name: ActiveValue::Set("acb-org".into()),
        slug: ActiveValue::Set(format!("acb-{}", org_id.simple())),
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
        name: ActiveValue::Set("acb-ws".into()),
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
        file_count_seen: ActiveValue::Set(1),
        file_count_compiled: ActiveValue::Set(1),
        file_count_failed: ActiveValue::Set(0),
        error_summary: ActiveValue::Set(None),
    }
    .insert(db)
    .await
    .expect("seed revision");

    // Promote (the FK requires the revision to exist first).
    let mut ws: workspaces::ActiveModel = workspaces::Entity::find_by_id(ws_id)
        .one(db)
        .await
        .expect("load workspace")
        .expect("workspace exists")
        .into();
    ws.current_revision_id = ActiveValue::Set(Some(rev_id));
    ws.update(db).await.expect("promote revision");

    (ws_id, rev_id)
}

/// Insert one compiled `.airway.yml` row. `name` is the YAML `name:` field and
/// deliberately differs from `file_path` — the mismatch that makes the choice
/// of lookup column observable.
async fn seed_pipeline(db: &DatabaseConnection, rev_id: Uuid, name: &str, file_path: &str) {
    airway_pipelines::ActiveModel {
        revision_id: ActiveValue::Set(rev_id),
        name: ActiveValue::Set(name.into()),
        file_path: ActiveValue::Set(file_path.into()),
        // Shape mirrors what `oxy-compile` writes: the whole parsed YAML,
        // untyped. It must satisfy the strict (`deny_unknown_fields`)
        // `AirwayPipelineSpec` on the way back out — that round-trip is the
        // point of the test, so the fixture is a realistic authored document
        // (a `destination:` *reference*, which is what users write).
        definition: ActiveValue::Set(json!({
            "name": name,
            "source": {
                "kind": "filesystem",
                "config": {
                    "base_path": "/tmp/acb",
                    "pattern": "*.jsonl",
                    "format": "jsonl",
                    "table_name": "orders",
                },
            },
            "destination": { "database": "warehouse", "dataset_name": "raw" },
            "resources": ["orders"],
        })),
    }
    .insert(db)
    .await
    .expect("seed airway pipeline");
}

/// An `OxyProjectContext` whose workspace path points at a directory that does
/// NOT exist — a stateless durable worker that never cloned the repo.
async fn worker_context_without_working_copy(ws_id: Uuid) -> (OxyProjectContext, PathBuf) {
    // Make this process look like a worker replica, so the host adapter takes
    // the "no working copy → no branch hint" arm.
    // SAFETY: single-threaded test setup; nextest gives each test its own process.
    unsafe {
        std::env::set_var("OXY_ROLE", "worker");
    }
    oxy_app::server::role_manifest::init_process_role_from_env();

    let absent = PathBuf::from(format!("/nonexistent-oxy-workspace/{}", Uuid::new_v4()));
    assert!(!absent.exists(), "precondition: no working copy on disk");

    let wm = WorkspaceBuilder::new(ws_id)
        .with_workspace_path_and_fallback_config(&absent)
        .await
        .expect("workspace builder")
        .build()
        .await
        .expect("workspace manager");
    (OxyProjectContext::new(wm), absent)
}

/// THE regression: the run path resolves a pipeline with no workspace
/// directory anywhere on disk. Before the compile-boundary port this could
/// only fail — `resolve_pipeline_ref` canonicalises the workspace root first,
/// so an absent working copy is an immediate "workspace root is not
/// accessible".
#[tokio::test]
async fn airway_pipeline_yaml_resolves_with_no_workspace_directory() {
    let db = setup_db().await;
    let (ws_id, rev_id) = seed_promoted_workspace(&db).await;
    seed_pipeline(&db, rev_id, "toast_orders", "pipelines/toast.airway.yml").await;

    let (ctx, absent_root) = worker_context_without_working_copy(ws_id).await;

    // The FS path is genuinely impossible here — pin that, so a future change
    // that quietly re-creates a working copy can't make this test vacuous.
    assert!(
        agentic_pipeline::pipeline_ref::resolve_pipeline_ref(
            &absent_root,
            "pipelines/toast.airway.yml"
        )
        .is_err(),
        "the filesystem path must be unavailable for this test to mean anything"
    );

    // Full production path: guard → port → host adapter → compiled_reader.
    let yaml =
        agentic_pipeline::pipeline_ref::load_pipeline_yaml(&ctx, "pipelines/toast.airway.yml")
            .await
            .expect("compiled row must satisfy the read with no working copy");

    // And the body the worker gets is one the airway parser accepts — the
    // JSONB → YAML round-trip has to survive its actual consumer, not just be
    // a non-empty string.
    let spec = agentic_airway::AirwayPipelineSpec::from_yaml_with_vars(&yaml, None)
        .expect("compiled definition must round-trip into an AirwayPipelineSpec");
    assert_eq!(spec.name, "toast_orders");
    assert_eq!(spec.source.kind, "filesystem");

    // Variables still render at run time: the worker re-renders the same
    // document with the run's `variables`, so templating is not collapsed by
    // serving from the boundary.
    let rendered = agentic_airway::AirwayPipelineSpec::from_yaml_with_vars(
        &yaml,
        Some(&json!({ "unused": "x" })),
    )
    .expect("rendering with variables must still work on a compiled body");
    assert_eq!(rendered.name, "toast_orders");
}

/// The containment guard still holds on the compiled path: a traversal ref is
/// rejected before either backend is consulted, and the error quotes only the
/// caller-supplied ref.
#[tokio::test]
async fn airway_pipeline_ref_containment_holds_without_a_working_copy() {
    let db = setup_db().await;
    let (ws_id, rev_id) = seed_promoted_workspace(&db).await;
    seed_pipeline(&db, rev_id, "toast_orders", "pipelines/toast.airway.yml").await;

    let (ctx, _absent) = worker_context_without_working_copy(ws_id).await;

    for bad in ["", "   ", "/etc/passwd", "../../etc/passwd", "a/../../b"] {
        let err = agentic_pipeline::pipeline_ref::load_pipeline_yaml(&ctx, bad)
            .await
            .expect_err("traversal / empty refs must be rejected");
        assert!(
            !err.to_string().contains("nonexistent-oxy-workspace"),
            "errors must quote only the ref, never a resolved path: {err}"
        );
    }
}

/// `pipeline_ref` is a workspace-relative PATH, so the reader must key on
/// `file_path` — not the `(revision_id, name)` primary key, whose `name` is the
/// YAML `name:` field. Keying by `name` misses for every pipeline whose name
/// differs from its path, silently falling back to a filesystem a stateless
/// replica doesn't have.
#[tokio::test]
async fn airway_compiled_reader_keys_by_file_path_not_name() {
    let db = setup_db().await;
    let (ws_id, rev_id) = seed_promoted_workspace(&db).await;
    seed_pipeline(&db, rev_id, "toast_orders", "pipelines/toast.airway.yml").await;

    let row = resolve_pipeline(ws_id, None, "pipelines/toast.airway.yml")
        .await
        .expect("query")
        .expect("must resolve by workspace-relative file_path");
    assert_eq!(row.name, "toast_orders");
    assert_eq!(row.file_path, "pipelines/toast.airway.yml");

    assert!(
        resolve_pipeline(ws_id, None, "toast_orders")
            .await
            .expect("query-by-name")
            .is_none(),
        "the YAML name must not resolve a row — the reader keys by file_path"
    );

    // An uncompiled / unknown path is a clean miss so the caller can fall
    // back, not an error.
    assert!(
        resolve_pipeline(ws_id, None, "pipelines/missing.airway.yml")
            .await
            .expect("missing-path query")
            .is_none()
    );
}

/// Multi-tenant containment on the DB path: a `pipeline_ref` naming another
/// workspace's pipeline must not resolve. The row set is scoped by the
/// caller's own promoted `revision_id`, which belongs to exactly one workspace.
#[tokio::test]
async fn airway_pipeline_ref_cannot_reach_another_workspace() {
    let db = setup_db().await;
    let (ws_a, rev_a) = seed_promoted_workspace(&db).await;
    let (ws_b, rev_b) = seed_promoted_workspace(&db).await;
    seed_pipeline(&db, rev_a, "a_pipeline", "pipelines/a.airway.yml").await;
    seed_pipeline(&db, rev_b, "b_pipeline", "pipelines/secret_b.airway.yml").await;

    assert!(
        resolve_pipeline(ws_a, None, "pipelines/secret_b.airway.yml")
            .await
            .expect("cross-workspace query")
            .is_none(),
        "workspace A must not resolve workspace B's pipeline"
    );
    assert_eq!(
        resolve_pipeline(ws_b, None, "pipelines/secret_b.airway.yml")
            .await
            .expect("own query")
            .expect("B resolves its own pipeline")
            .name,
        "b_pipeline"
    );
}

/// A workspace with no promoted revision is a clean `None` — the caller falls
/// through to the filesystem, exactly as before. This is `open_compiled_revision`'s
/// contract; pinned here so the airway reader can't drift from it.
#[tokio::test]
async fn airway_unpromoted_workspace_falls_through_to_fs() {
    let db = setup_db().await;
    let (ws_id, rev_id) = seed_promoted_workspace(&db).await;
    seed_pipeline(&db, rev_id, "toast_orders", "pipelines/toast.airway.yml").await;

    // Un-promote.
    let mut ws: workspaces::ActiveModel = workspaces::Entity::find_by_id(ws_id)
        .one(&db)
        .await
        .expect("load workspace")
        .expect("workspace exists")
        .into();
    ws.current_revision_id = ActiveValue::Set(None);
    ws.update(&db).await.expect("un-promote");

    assert!(
        resolve_pipeline(ws_id, None, "pipelines/toast.airway.yml")
            .await
            .expect("query")
            .is_none(),
        "an unpromoted workspace must read the FS, not a stale revision"
    );
}
