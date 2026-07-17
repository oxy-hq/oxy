//! Example customer-app seed — deploys the checked-in `oxy-starter` bundle so a
//! fresh `oxy seed` lands on a launcher with a real, clickable app instead of an
//! empty grid.
//!
//! **It deploys through the real publish path**, not a shortcut: bytes go to the
//! build store, then an `app_builds` row, then the channel pointers — the same
//! order (and the same code) `POST /api/customer-apps/publish` uses. What it
//! skips is only the transport: no tarball, no HTTP, no bundle validation. That
//! matters because it means the seeded app exercises the serve path a real
//! `oxy publish` produces, so a test against it tests the shipping code.
//!
//! The bundle is checked into the repo (`examples/customer_apps/oxy-starter/`)
//! rather than generated here: it's reviewable in a diff, and `oxy seed` stays a
//! tool that reads the workspace instead of writing to it.
//!
//! No S3, no Node. The build store falls back to the filesystem when
//! `OXY_CUSTOMER_APPS_S3_BUCKET` is unset, and the bundle is hand-written HTML
//! with no build step — so this works on a fresh clone with no toolchain.
//!
//! **Superseded builds are left behind on purpose.** Editing the bundle changes
//! its content hash, so a re-seed writes a new build (row + bytes) and repoints
//! the channels at it; the previous one stays until `oxy seed --clear`. That's
//! not a leak to fix here — it's how a rollback stays possible, it mirrors what
//! the real publish pipeline keeps (which has `gc_builds` for the hosted case),
//! and the cost is a few KB per edit on a dev machine.

use std::path::{Path, PathBuf};

use chrono::Utc;
use entity::prelude::{AppBuilds, Apps, Organizations, Workspaces};
use entity::{app_builds, apps, organizations, workspaces};
use oxy::theme::StyledText;
use oxy_shared::errors::OxyError;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
    QueryOrder,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::server::api::customer_apps_build_store as store;

type Conn = sea_orm::DatabaseConnection;

/// Bundle directory, relative to the seeded workspace path.
const BUNDLE_REL_PATH: &str = "customer_apps/oxy-starter";

/// The app slug, shared by every deployment. `apps` is unique on
/// `(org_id, slug)`, not on slug alone — so the same bundle can serve
/// `local/oxy-starter` and `acme/oxy-starter` as two independent deployments,
/// which is exactly the multi-tenant shape the platform is built for.
///
/// Deliberately NOT `hello-oxy`: that slug belongs to the canonical worked
/// example in the customer-apps repo. Sharing it would mean a developer who
/// publishes the real one locally collides with the seed on `(org_id, slug)` —
/// and `oxy seed --clear` would then delete their work.
const APP_SLUG: &str = "oxy-starter";

/// Files that document the example but aren't part of the deployed bundle.
/// A real `oxy publish` ships a build output directory, which wouldn't
/// contain these.
const NOT_BUNDLE_FILES: &[&str] = &["README.md"];

/// One deployment of the bundle: an org and the workspace whose launcher
/// should show it.
pub(crate) struct AppTarget {
    pub org_id: Uuid,
    pub org_slug: String,
    /// `apps.project_id` — the column is named `project_id` but holds a
    /// `workspaces.id`. The launcher filters on it verbatim.
    pub workspace_id: Uuid,
}

/// Deterministic per-org app id, so re-seeding updates one row rather than
/// racing the `(org_id, slug)` unique index.
fn app_id_for(org_id: Uuid) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_DNS,
        format!("oxy.app-seed.{org_id}.{APP_SLUG}").as_bytes(),
    )
}

/// Content-addressed build id: hash the bundle, use the first 16 hex chars.
///
/// A stable id would let edited bytes land on the prefix a running server has
/// already cached, and the reader would keep serving the old page
/// (`customer_apps_bundle_cache` keys on `(app_id, build_id, path)`). Hashing
/// means changed bytes are a different build, so an edit can't be shadowed by
/// a warm cache. It also mirrors what a real publish does with a commit sha.
fn build_id_for(files: &[(String, Vec<u8>)]) -> String {
    let mut hasher = Sha256::new();
    for (rel, bytes) in files {
        hasher.update(rel.as_bytes());
        hasher.update([0]);
        hasher.update(bytes);
    }
    hex::encode(hasher.finalize())[..16].to_string()
}

/// Read the bundle into `(relative_path, bytes)` pairs, sorted so the content
/// hash is stable across filesystems (readdir order is not).
async fn read_bundle(dir: &Path) -> Result<Vec<(String, Vec<u8>)>, OxyError> {
    let mut files = Vec::new();
    collect_files(dir, dir, &mut files).await?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    if !files.iter().any(|(rel, _)| rel == "index.html") {
        return Err(OxyError::RuntimeError(format!(
            "example app bundle at {} has no index.html",
            dir.display()
        )));
    }
    Ok(files)
}

/// Recursive walk. Boxed because `async fn` recursion needs an indirection.
fn collect_files<'a>(
    root: &'a Path,
    dir: &'a Path,
    out: &'a mut Vec<(String, Vec<u8>)>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), OxyError>> + Send + 'a>> {
    Box::pin(async move {
        let mut entries = tokio::fs::read_dir(dir)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("read {}: {e}", dir.display())))?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("walk {}: {e}", dir.display())))?
        {
            let path = entry.path();
            if path.is_dir() {
                collect_files(root, &path, out).await?;
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .map_err(|e| OxyError::RuntimeError(format!("relativize {path:?}: {e}")))?
                .to_string_lossy()
                .replace('\\', "/");
            if NOT_BUNDLE_FILES.contains(&rel.as_str()) {
                continue;
            }
            let bytes = tokio::fs::read(&path)
                .await
                .map_err(|e| OxyError::RuntimeError(format!("read {}: {e}", path.display())))?;
            out.push((rel, bytes));
        }
        Ok(())
    })
}

/// The bundle's own `oxy-app.json`, for the `app_builds.manifest_json` column
/// the launcher reads card metadata from.
fn manifest_of(files: &[(String, Vec<u8>)]) -> Option<serde_json::Value> {
    let (_, bytes) = files.iter().find(|(rel, _)| rel == "oxy-app.json")?;
    serde_json::from_slice(bytes).ok()
}

/// Where the bundle lives for a given seeded workspace path.
fn bundle_dir(workspace_path: &str) -> PathBuf {
    Path::new(workspace_path).join(BUNDLE_REL_PATH)
}

/// Deploy the example bundle into every target org.
///
/// Returns the number of apps deployed. Skips (returns `Ok(0)`) when the bundle
/// isn't present — `--workspace-path` can point at any directory, and a seed
/// shouldn't fail because the developer's own workspace has no example app in it.
pub(crate) async fn seed_example_apps(
    conn: &Conn,
    workspace_path: &str,
    targets: &[AppTarget],
) -> Result<usize, OxyError> {
    let dir = bundle_dir(workspace_path);
    if !dir.is_dir() {
        return Ok(0);
    }
    let files = read_bundle(&dir).await?;
    let build_id = build_id_for(&files);
    let manifest = manifest_of(&files);

    println!(
        "{} deploying example app ({} file{}, build {build_id})",
        "📦".info(),
        files.len(),
        if files.len() == 1 { "" } else { "s" }
    );

    let mut deployed = 0;
    for target in targets {
        deploy(conn, target, &files, &build_id, manifest.clone()).await?;
        println!(
            "  {} /customer-apps/{}/{APP_SLUG}/",
            "✓".success(),
            target.org_slug
        );
        deployed += 1;
    }
    Ok(deployed)
}

/// One deployment, in the same order `publish()` uses:
///
/// 1. the `apps` row — `app_builds.app_id` carries a foreign key to it, so a
///    build row can't exist first;
/// 2. the bytes;
/// 3. the `app_builds` row;
/// 4. the channel pointers.
///
/// Pointers last is what makes the sequence safe to interrupt: a row only ever
/// names a build whose bytes are already stored, so a half-finished seed leaves
/// the previous build live rather than a 404.
async fn deploy(
    conn: &Conn,
    target: &AppTarget,
    files: &[(String, Vec<u8>)],
    build_id: &str,
    manifest: Option<serde_json::Value>,
) -> Result<(), OxyError> {
    let app_id = app_id_for(target.org_id);
    ensure_app(conn, target, app_id).await?;

    // Unconditional, even when the rows already exist: this is what heals a
    // wiped state dir (`oxy clean`, a new machine, a pruned Docker volume).
    // The DB would still name the build, but the bytes would be gone and the
    // app would 404 — so re-running `oxy seed` is the repair.
    let prefix = store::put_build(app_id, build_id, files.to_vec())
        .await
        .map_err(|e| OxyError::RuntimeError(format!("store example app bundle: {e}")))?;

    let build_pk = upsert_build(conn, app_id, build_id, &prefix, manifest).await?;
    point_app_at(conn, app_id, build_pk).await?;
    Ok(())
}

/// The `app_builds` row. Keyed on `(app_id, build_id)` like the real publish,
/// which the schema enforces as unique.
async fn upsert_build(
    conn: &Conn,
    app_id: Uuid,
    build_id: &str,
    prefix: &str,
    manifest: Option<serde_json::Value>,
) -> Result<Uuid, OxyError> {
    let existing = AppBuilds::find()
        .filter(app_builds::Column::AppId.eq(app_id))
        .filter(app_builds::Column::BuildId.eq(build_id))
        .one(conn)
        .await
        .map_err(|e| OxyError::DBError(format!("query app_build {build_id}: {e}")))?;
    if let Some(row) = existing {
        return Ok(row.id);
    }
    let id = Uuid::new_v5(
        &Uuid::NAMESPACE_DNS,
        format!("oxy.app-seed.build.{app_id}.{build_id}").as_bytes(),
    );
    app_builds::ActiveModel {
        id: ActiveValue::Set(id),
        app_id: ActiveValue::Set(app_id),
        build_id: ActiveValue::Set(build_id.to_string()),
        s3_prefix: ActiveValue::Set(prefix.to_string()),
        manifest_json: ActiveValue::Set(manifest),
        created_at: ActiveValue::Set(Utc::now().fixed_offset()),
        published_by: ActiveValue::Set(None),
        source_repo: ActiveValue::Set(Some("oxy-hq/oxygen".to_string())),
        commit_sha: ActiveValue::Set(None),
        source_branch: ActiveValue::Set(Some("main".to_string())),
        // The promote gate refuses to make a build live unless this is
        // "passed" — the seed bypasses the validator, so it must assert the
        // verdict the validator would have reached for a bundle this simple.
        validation_status: ActiveValue::Set("passed".to_string()),
        validation_detail: ActiveValue::Set(None),
    }
    .insert(conn)
    .await
    .map_err(|e| OxyError::DBError(format!("insert app_build {build_id}: {e}")))?;
    Ok(id)
}

/// The `apps` row, without channel pointers — those are set by
/// [`point_app_at`] once the build they name actually exists.
async fn ensure_app(conn: &Conn, target: &AppTarget, app_id: Uuid) -> Result<(), OxyError> {
    let now = Utc::now().fixed_offset();
    let existing = Apps::find_by_id(app_id)
        .one(conn)
        .await
        .map_err(|e| OxyError::DBError(format!("query app {app_id}: {e}")))?;

    if let Some(row) = existing {
        // Re-point an existing row at the workspace, in case the demo
        // workspace was re-created with a different id.
        if row.project_id != target.workspace_id {
            let mut active = row.into_active_model();
            active.project_id = ActiveValue::Set(target.workspace_id);
            active.updated_at = ActiveValue::Set(now);
            active
                .update(conn)
                .await
                .map_err(|e| OxyError::DBError(format!("re-point app {app_id}: {e}")))?;
        }
        return Ok(());
    }

    apps::ActiveModel {
        id: ActiveValue::Set(app_id),
        slug: ActiveValue::Set(APP_SLUG.to_string()),
        name: ActiveValue::Set("Oxy Starter".to_string()),
        org_id: ActiveValue::Set(target.org_id),
        project_id: ActiveValue::Set(target.workspace_id),
        branch: ActiveValue::Set("main".to_string()),
        source_repo: ActiveValue::Set("oxy-hq/oxygen".to_string()),
        status: ActiveValue::Set("created".to_string()),
        // "s3" is the tag for the build-store pipeline; the store itself picks
        // S3 or the filesystem from OXY_CUSTOMER_APPS_S3_BUCKET. "local" would
        // mean something else entirely — serve the dev's directory directly,
        // bypassing builds and channels — and would not exercise the real path.
        source_type: ActiveValue::Set("s3".to_string()),
        // NOT NULL with no default: NotSet fails the insert.
        source_config: ActiveValue::Set(serde_json::json!({})),
        last_synced_at: ActiveValue::Set(None),
        manifest_override: ActiveValue::Set(None),
        bootstrap_pr_url: ActiveValue::Set(None),
        // Pointers and publication are set by point_app_at() once the build
        // exists. Publishing here would name a build_pk that doesn't exist yet.
        published_at: ActiveValue::Set(None),
        repo_path: ActiveValue::Set(Some(format!("examples/{APP_SLUG}"))),
        draft_build_id: ActiveValue::Set(None),
        published_build_id: ActiveValue::Set(None),
        last_promoted_by: ActiveValue::Set(None),
        last_promoted_at: ActiveValue::Set(None),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
    }
    .insert(conn)
    .await
    .map_err(|e| OxyError::DBError(format!("insert app {app_id}: {e}")))?;
    Ok(())
}

/// Make `build_pk` live on both channels and publish the app.
///
/// `published_at` is what the launcher filters on. Without it the app still
/// serves on a direct URL — `resolve_channel` falls back to the draft channel —
/// but never appears on the home grid. That half-state reads as "the launcher is
/// broken" rather than "the app isn't published", so the seed always sets it.
async fn point_app_at(conn: &Conn, app_id: Uuid, build_pk: Uuid) -> Result<(), OxyError> {
    let now = Utc::now().fixed_offset();
    let row = Apps::find_by_id(app_id)
        .one(conn)
        .await
        .map_err(|e| OxyError::DBError(format!("query app {app_id}: {e}")))?
        .ok_or_else(|| OxyError::RuntimeError(format!("app {app_id} vanished mid-deploy")))?;

    let mut active = row.into_active_model();
    active.draft_build_id = ActiveValue::Set(Some(build_pk));
    active.published_build_id = ActiveValue::Set(Some(build_pk));
    active.published_at = ActiveValue::Set(Some(now));
    active.last_promoted_at = ActiveValue::Set(Some(now));
    active.updated_at = ActiveValue::Set(now);
    active
        .update(conn)
        .await
        .map_err(|e| OxyError::DBError(format!("point app {app_id} at build: {e}")))?;
    Ok(())
}

/// Tear down every app this seed deployed — the stored bytes as well as the
/// rows.
///
/// Matches on the deterministic id, not just the slug: `id == app_id_for(org_id)`
/// proves the seed created the row, so a developer's own app that happens to be
/// called `hello-oxy` is left alone. `apps.project_id` has no foreign key, so
/// dropping the workspace would otherwise leave these rows dangling — and the
/// bytes on disk have no owner at all.
pub(crate) async fn clear_example_apps(conn: &Conn) -> Result<u64, OxyError> {
    let rows = Apps::find()
        .filter(apps::Column::Slug.eq(APP_SLUG))
        .all(conn)
        .await
        .map_err(|e| OxyError::DBError(format!("query seeded apps: {e}")))?;

    let mut removed = 0;
    for row in rows {
        if row.id != app_id_for(row.org_id) {
            continue;
        }
        // Bytes first: a failure here leaves the row in place, so a re-run
        // still knows what to clean. The reverse would orphan the files.
        store::delete_app(row.id)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("delete app bundle {}: {e}", row.id)))?;
        removed += Apps::delete_by_id(row.id)
            .exec(conn)
            .await
            .map_err(|e| OxyError::DBError(format!("delete app {}: {e}", row.id)))?
            .rows_affected;
    }
    Ok(removed)
}

/// An org's first workspace by name, for orgs the partner seed created (whose
/// ids it derives internally). `None` when the org or its workspace is absent —
/// the partner seed skips on a non-local DB, so callers must tolerate that.
pub(crate) async fn first_workspace_of(
    conn: &Conn,
    org_slug: &str,
) -> Result<Option<AppTarget>, OxyError> {
    let Some(org) = Organizations::find()
        .filter(organizations::Column::Slug.eq(org_slug))
        .one(conn)
        .await
        .map_err(|e| OxyError::DBError(format!("lookup org {org_slug}: {e}")))?
    else {
        return Ok(None);
    };
    let Some(ws) = Workspaces::find()
        .filter(workspaces::Column::OrgId.eq(org.id))
        .order_by_asc(workspaces::Column::Name)
        .one(conn)
        .await
        .map_err(|e| OxyError::DBError(format!("lookup workspace for {org_slug}: {e}")))?
    else {
        return Ok(None);
    };
    Ok(Some(AppTarget {
        org_id: org.id,
        org_slug: org.slug,
        workspace_id: ws.id,
    }))
}

#[cfg(test)]
#[path = "seed_apps_tests.rs"]
mod tests;
