//! `POST /api/customer-apps/publish` — the one-way publish entry point.
//!
//! CI (or a local `oxy publish`) uploads a gzipped tar of the built
//! bundle. The service validates it, stores each file in S3 under a
//! per-build prefix ([`super::customer_apps_build_store`]), records an
//! `app_builds` row, upserts the `apps` row (creating it on first
//! publish), and points the draft channel (and published, with
//! `--promote`) at the new build. Replaces the old
//! `ensure` + `aws s3 sync` + callback-`/sync` dance.
//!
//! Gating: mounted under the app-admin guard, so the caller is trusted
//! oxy staff. On top of that, an Oxy engineer may only publish into a
//! workspace whose org has granted Oxy access (the `workspace_oxy_access`
//! toggle) — org members publishing their own apps are exempt. We also
//! validate that the target project belongs to the named org to catch
//! fat-finger cross-org publishes.

use std::io::Read;
use std::sync::Arc;

use axum::Json;
use axum::extract::Multipart;
use axum::http::StatusCode;
use chrono::Utc;
use entity::{app_builds, app_functions, apps, organizations, workspaces};
use flate2::read::GzDecoder;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, EntityTrait, ModelTrait,
    QueryFilter, QueryOrder,
};
use serde::Serialize;
use tar::Archive;
use uuid::Uuid;

use super::{
    customer_apps_auth, customer_apps_build_store as store, customer_apps_bundle_cache as cache,
};

/// How many builds to retain per app. Older builds (not currently pointed
/// at by either channel) are GC'd from the DB and S3 after each publish.
const KEEP_BUILDS: usize = 10;

pub struct PublishInput {
    /// Org identity — accepts either a slug (`"acme"`) or a UUID
    /// (`"550e8400-e29b-41d4-a716-446655440000"`). UUIDs are useful
    /// when the slug has drifted between envs (e.g. an admin renamed
    /// the org in prod but not staging) and the publisher wants a
    /// stable handle. `resolve_org` looks at both columns.
    pub org_ref: OrgRef,
    pub app_slug: String,
    pub project_id: Uuid,
    pub branch: Option<String>,
    pub build_id: String,
    pub name: Option<String>,
    pub promote: bool,
    pub tarball: Vec<u8>,
    pub manifest: Option<serde_json::Value>,
    /// Authenticated publisher (app-admin). Recorded on the build for the
    /// "who deployed" audit in the admin UI.
    pub published_by: Option<Uuid>,
}

/// How the publisher referred to the target org. Accepting both lets
/// CLI users pass whichever they have at hand: `--org acme` (slug,
/// stable for humans) or `--org 550e8400-...` (UUID, stable across
/// rename/env-drift). The server tries them in order.
#[derive(Debug)]
pub enum OrgRef {
    /// Looked up against `organizations.slug`.
    Slug(String),
    /// Looked up against `organizations.id`. Skips a slug round-trip
    /// when the publisher already knows the row id.
    Id(Uuid),
}

impl OrgRef {
    /// Auto-detect: if the input parses as a UUID, treat as `Id`;
    /// otherwise treat as `Slug`. Lets a single `--org <value>` CLI
    /// arg accept either form without forcing the user to pick a
    /// different flag.
    pub fn from_str_auto(s: &str) -> Self {
        match Uuid::parse_str(s) {
            Ok(id) => OrgRef::Id(id),
            Err(_) => OrgRef::Slug(s.to_string()),
        }
    }

    fn describe(&self) -> String {
        match self {
            OrgRef::Slug(s) => s.clone(),
            OrgRef::Id(id) => id.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PublishResult {
    pub app_id: Uuid,
    pub build_id: String,
    pub url: String,
    pub channel: String,
    /// Canonical org slug the app row landed on, after the server
    /// resolved whatever `OrgRef` the publisher sent. Echoing the
    /// server's view lets the CLI render `Registered new app
    /// acme/store-pulse` even when the engineer passed a UUID (which
    /// would otherwise echo back as `550e8400-…/store-pulse` —
    /// technically correct but jarring).
    pub org_slug: String,
    /// `true` when this publish created the app row, `false` when it
    /// updated an existing row. Lets the CLI tell the engineer whether
    /// they just registered a brand-new app or shipped a new version
    /// of one that was already in the system — surfaces accidental
    /// re-registration vs. intentional re-publish.
    pub is_new_app: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum PublishError {
    #[error("organization {0:?} not found")]
    UnknownOrg(String),
    #[error("project {0} is not part of org {1:?}")]
    UnknownProject(Uuid, String),
    #[error(
        "org {org:?} has not granted Oxy access for workspace {project} — an org owner must enable it (workspace settings → Oxy access)"
    )]
    OxyAccessDenied { org: String, project: Uuid },
    #[error("invalid bundle: {0}")]
    BadTarball(String),
    #[error("database error: {0}")]
    Db(String),
    #[error("storage error: {0}")]
    S3(String),
}

impl From<store::BuildStoreError> for PublishError {
    fn from(e: store::BuildStoreError) -> Self {
        PublishError::S3(e.to_string())
    }
}

impl PublishError {
    fn status(&self) -> StatusCode {
        match self {
            PublishError::UnknownOrg(_) | PublishError::UnknownProject(..) => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            PublishError::OxyAccessDenied { .. } => StatusCode::FORBIDDEN,
            PublishError::BadTarball(_) => StatusCode::UNPROCESSABLE_ENTITY,
            PublishError::Db(_) | PublishError::S3(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// A tar entry path is safe iff it's relative and contains no `..`
/// component — guards against an archive member escaping the build
/// prefix (`../../etc/...`) when its files are later keyed into S3.
fn is_safe_relative_path(path: &std::path::Path) -> bool {
    !path.is_absolute()
        && !path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
}

/// Decompress a gzipped tar into `(relative_path, bytes)` pairs, rejecting
/// absolute paths and `..` traversal. Directories are skipped.
pub fn unpack_tar_gz(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, PublishError> {
    let mut archive = Archive::new(GzDecoder::new(bytes));
    let entries = archive
        .entries()
        .map_err(|e| PublishError::BadTarball(e.to_string()))?;
    let mut out = Vec::new();
    for entry in entries {
        let mut entry = entry.map_err(|e| PublishError::BadTarball(e.to_string()))?;
        let path = entry
            .path()
            .map_err(|e| PublishError::BadTarball(e.to_string()))?;
        if !is_safe_relative_path(&path) {
            return Err(PublishError::BadTarball(format!(
                "unsafe path in tarball: {}",
                path.display()
            )));
        }
        if entry.header().entry_type().is_dir() {
            continue;
        }
        let rel = path.to_string_lossy().trim_start_matches("./").to_string();
        if rel.is_empty() {
            continue;
        }
        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|e| PublishError::BadTarball(e.to_string()))?;
        out.push((rel, buf));
    }
    if !out.iter().any(|(p, _)| p == "index.html") {
        return Err(PublishError::BadTarball(
            "bundle has no index.html at its root".to_string(),
        ));
    }
    Ok(out)
}

async fn resolve_org(
    db: &DatabaseConnection,
    org_ref: &OrgRef,
) -> Result<organizations::Model, PublishError> {
    let row = match org_ref {
        OrgRef::Slug(slug) => {
            organizations::Entity::find()
                .filter(organizations::Column::Slug.eq(slug))
                .one(db)
                .await
        }
        OrgRef::Id(id) => organizations::Entity::find_by_id(*id).one(db).await,
    };
    row.map_err(|e| PublishError::Db(e.to_string()))?
        .ok_or_else(|| PublishError::UnknownOrg(org_ref.describe()))
}

/// Best-effort cross-org guard: if the project id resolves to a workspace,
/// its org must match. Unknown ids are rejected so a typo can't silently
/// create a row pointing at nothing.
async fn validate_project(
    db: &DatabaseConnection,
    project_id: Uuid,
    org_id: Uuid,
    org_slug: &str,
) -> Result<(), PublishError> {
    let ws = workspaces::Entity::find_by_id(project_id)
        .one(db)
        .await
        .map_err(|e| PublishError::Db(e.to_string()))?;
    match ws {
        Some(w) if w.org_id == Some(org_id) => Ok(()),
        _ => Err(PublishError::UnknownProject(
            project_id,
            org_slug.to_string(),
        )),
    }
}

/// Beyond the app-admin route guard: an Oxy engineer may only publish into
/// a workspace whose org has granted Oxy access (`workspace_oxy_access`).
/// Org members are exempt — they own the app, so a workspace owner (and a
/// local-mode operator on their own org) can always publish regardless of
/// the toggle. Mirrors the two-path model in
/// [`customer_apps_auth::user_can_access_app`].
async fn authorize_publish(
    db: &DatabaseConnection,
    org: &organizations::Model,
    input: &PublishInput,
) -> Result<(), PublishError> {
    let is_member = match input.published_by {
        Some(uid) => customer_apps_auth::is_org_member(db, uid, org.id)
            .await
            .map_err(|e| PublishError::Db(e.to_string()))?,
        None => false,
    };
    if is_member {
        return Ok(());
    }
    let granted = customer_apps_auth::is_oxy_access_enabled(db, input.project_id)
        .await
        .map_err(|e| PublishError::Db(e.to_string()))?;
    if granted {
        return Ok(());
    }
    Err(PublishError::OxyAccessDenied {
        org: org.slug.clone(),
        project: input.project_id,
    })
}

async fn find_app(
    db: &DatabaseConnection,
    org_id: Uuid,
    slug: &str,
) -> Result<Option<apps::Model>, PublishError> {
    apps::Entity::find()
        .filter(apps::Column::OrgId.eq(org_id))
        .filter(apps::Column::Slug.eq(slug))
        .one(db)
        .await
        .map_err(|e| PublishError::Db(e.to_string()))
}

/// Insert (first publish) or update the `apps` row. Returns the app id
/// and `is_new = true` iff this call inserted a fresh row — the CLI uses
/// that to print "Registered new app" vs "Published new version of …"
/// so engineers spot accidental re-registration and intentional updates
/// without scanning the diff.
async fn upsert_app(
    db: &DatabaseConnection,
    org: &organizations::Model,
    input: &PublishInput,
) -> Result<(Uuid, bool), PublishError> {
    let now = Utc::now().fixed_offset();
    let existing = find_app(db, org.id, &input.app_slug).await?;
    if let Some(row) = existing {
        let id = row.id;
        let mut active: apps::ActiveModel = row.into();
        active.project_id = ActiveValue::Set(input.project_id);
        if let Some(b) = &input.branch {
            active.branch = ActiveValue::Set(b.clone());
        }
        if let Some(name) = &input.name {
            active.name = ActiveValue::Set(name.clone());
        }
        active.last_synced_at = ActiveValue::Set(Some(now));
        active.updated_at = ActiveValue::Set(now);
        active
            .update(db)
            .await
            .map_err(|e| PublishError::Db(e.to_string()))?;
        return Ok((id, false));
    }

    let id = Uuid::new_v4();
    let name = input
        .name
        .clone()
        .unwrap_or_else(|| humanize_slug(&input.app_slug));
    let model = apps::ActiveModel {
        id: ActiveValue::Set(id),
        slug: ActiveValue::Set(input.app_slug.clone()),
        name: ActiveValue::Set(name),
        org_id: ActiveValue::Set(org.id),
        project_id: ActiveValue::Set(input.project_id),
        branch: ActiveValue::Set(input.branch.clone().unwrap_or_else(|| "main".to_string())),
        source_repo: ActiveValue::Set("oxy-hq/customer-apps".to_string()),
        status: ActiveValue::Set("created".to_string()),
        source_type: ActiveValue::Set("s3".to_string()),
        source_config: ActiveValue::Set(serde_json::json!({})),
        last_synced_at: ActiveValue::Set(Some(now)),
        manifest_override: ActiveValue::NotSet,
        bootstrap_pr_url: ActiveValue::NotSet,
        // Stays a draft until promoted (here or via /admin/apps).
        published_at: ActiveValue::NotSet,
        repo_path: ActiveValue::Set(Some(format!("{}/{}", org.slug, input.app_slug))),
        draft_build_id: ActiveValue::NotSet,
        published_build_id: ActiveValue::NotSet,
        last_promoted_by: ActiveValue::NotSet,
        last_promoted_at: ActiveValue::NotSet,
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
    };
    model
        .insert(db)
        .await
        .map_err(|e| PublishError::Db(e.to_string()))?;
    Ok((id, true))
}

fn humanize_slug(slug: &str) -> String {
    slug.split('-')
        .filter(|s| !s.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

async fn record_build(
    db: &DatabaseConnection,
    app_id: Uuid,
    input: &PublishInput,
    s3_prefix: String,
    manifest_json: Option<serde_json::Value>,
) -> Result<Uuid, PublishError> {
    let build_pk = Uuid::new_v4();
    let model = app_builds::ActiveModel {
        id: ActiveValue::Set(build_pk),
        app_id: ActiveValue::Set(app_id),
        build_id: ActiveValue::Set(input.build_id.clone()),
        s3_prefix: ActiveValue::Set(s3_prefix),
        manifest_json: ActiveValue::Set(manifest_json),
        created_at: ActiveValue::Set(Utc::now().fixed_offset()),
        published_by: ActiveValue::Set(input.published_by),
    };
    model
        .insert(db)
        .await
        .map_err(|e| PublishError::Db(e.to_string()))?;
    Ok(build_pk)
}

/// Extract `(name, per-function manifest JSON)` pairs from the bundle
/// manifest's `functions` block. Empty when the manifest declares none
/// (today's static-bundle default), so this is a no-op for function-less apps.
fn function_specs(manifest_json: Option<&serde_json::Value>) -> Vec<(String, serde_json::Value)> {
    manifest_json
        .and_then(|m| m.get("functions"))
        .and_then(|f| f.as_object())
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default()
}

/// Build-store key of a function's bundled JS artifact, matching the layout
/// `oxy publish` uploads (`<build_prefix>functions/<name>.js`) and the
/// `app_functions.artifact_key` contract.
fn function_artifact_key(build_prefix: &str, name: &str) -> String {
    format!("{build_prefix}functions/{name}.js")
}

/// Record one `app_functions` row per declared function, keyed to this build,
/// so the `/fn/<name>` route can resolve them. Without this the bundled JS
/// ships in the build store but the runtime can never find it (→ 404).
async fn record_functions(
    db: &DatabaseConnection,
    app_id: Uuid,
    build_pk: Uuid,
    build_prefix: &str,
    specs: &[(String, serde_json::Value)],
) -> Result<(), PublishError> {
    for (name, spec) in specs {
        let model = app_functions::ActiveModel {
            id: ActiveValue::Set(Uuid::new_v4()),
            app_id: ActiveValue::Set(app_id),
            build_id: ActiveValue::Set(build_pk),
            name: ActiveValue::Set(name.clone()),
            manifest_json: ActiveValue::Set(Some(spec.clone())),
            artifact_key: ActiveValue::Set(function_artifact_key(build_prefix, name)),
            created_at: ActiveValue::Set(Utc::now().fixed_offset()),
        };
        model
            .insert(db)
            .await
            .map_err(|e| PublishError::Db(e.to_string()))?;
    }
    Ok(())
}

/// Point the channel(s) at the new build. Draft always; published +
/// `published_at` when promoting.
async fn set_pointers(
    db: &DatabaseConnection,
    app_id: Uuid,
    build_pk: Uuid,
    promote: bool,
) -> Result<(), PublishError> {
    let row = apps::Entity::find_by_id(app_id)
        .one(db)
        .await
        .map_err(|e| PublishError::Db(e.to_string()))?
        .ok_or_else(|| PublishError::Db(format!("app {app_id} vanished mid-publish")))?;
    let mut active: apps::ActiveModel = row.into();
    active.draft_build_id = ActiveValue::Set(Some(build_pk));
    if promote {
        active.published_build_id = ActiveValue::Set(Some(build_pk));
        active.published_at = ActiveValue::Set(Some(Utc::now().fixed_offset()));
    }
    active
        .update(db)
        .await
        .map_err(|e| PublishError::Db(e.to_string()))?;
    Ok(())
}

/// Delete builds beyond `KEEP_BUILDS`, never touching the rows the two
/// channel pointers currently reference. Best-effort on the S3 side.
async fn gc_builds(db: &DatabaseConnection, app_id: Uuid, protect: &[Uuid]) {
    let builds = match app_builds::Entity::find()
        .filter(app_builds::Column::AppId.eq(app_id))
        .order_by_desc(app_builds::Column::CreatedAt)
        .all(db)
        .await
    {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("gc_builds list failed for app {app_id}: {e}");
            return;
        }
    };
    for build in builds.into_iter().skip(KEEP_BUILDS) {
        if protect.contains(&build.id) {
            continue;
        }
        if let Err(e) = store::delete_build(app_id, &build.build_id).await {
            tracing::warn!("gc_builds S3 delete failed ({}): {e}", build.build_id);
        }
        let build_label = build.build_id.clone();
        if let Err(e) = build.delete(db).await {
            tracing::warn!("gc_builds row delete failed ({build_label}): {e}");
        }
    }
}

pub async fn publish(input: PublishInput) -> Result<PublishResult, PublishError> {
    let db = oxy::database::client::establish_connection()
        .await
        .map_err(|e| PublishError::Db(e.to_string()))?;

    let org = resolve_org(&db, &input.org_ref).await?;
    validate_project(&db, input.project_id, org.id, &org.slug).await?;
    authorize_publish(&db, &org, &input).await?;

    let files = unpack_tar_gz(&input.tarball)?;
    let index_bytes = files
        .iter()
        .find(|(p, _)| p == "index.html")
        .map(|(_, b)| Arc::new(b.clone()));
    // Capture the bundle's oxy-app.json into the build row so the manifest
    // resolver (debug endpoint) reads it from the DB, not a local file.
    // Falls back to an explicit `manifest` multipart field if the bundle
    // didn't ship one.
    let manifest_json = files
        .iter()
        .find(|(p, _)| p == "oxy-app.json")
        .and_then(|(_, b)| serde_json::from_slice::<serde_json::Value>(b).ok())
        .or_else(|| input.manifest.clone());

    let (app_id, is_new_app) = upsert_app(&db, &org, &input).await?;
    let s3_prefix = store::put_build(app_id, &input.build_id, files).await?;
    // Capture the function specs + build prefix before `record_build` consumes
    // `manifest_json` and `s3_prefix`.
    let fn_specs = function_specs(manifest_json.as_ref());
    let build_prefix = s3_prefix.clone();
    // Bytes are now stored. If recording the row or moving the pointer fails,
    // roll the orphaned build back out so a partial publish leaves no
    // half-state (leaked storage prefix, or a row no channel points at).
    let build_pk = match record_build(&db, app_id, &input, s3_prefix, manifest_json).await {
        Ok(pk) => pk,
        Err(e) => {
            if let Err(cleanup) = store::delete_build(app_id, &input.build_id).await {
                tracing::warn!("publish rollback: orphan prefix left for {app_id}: {cleanup}");
            }
            return Err(e);
        }
    };
    // Register the bundle's functions against this build so `/fn/<name>`
    // resolves. On failure roll the orphan build back out (the app_functions
    // FK cascades when the build row is deleted).
    if let Err(e) = record_functions(&db, app_id, build_pk, &build_prefix, &fn_specs).await {
        if let Err(cleanup) = store::delete_build(app_id, &input.build_id).await {
            tracing::warn!("publish rollback: orphan prefix left for {app_id}: {cleanup}");
        }
        if let Ok(Some(row)) = app_builds::Entity::find_by_id(build_pk).one(&db).await {
            let _ = row.delete(&db).await;
        }
        return Err(e);
    }
    if let Err(e) = set_pointers(&db, app_id, build_pk, input.promote).await {
        if let Err(cleanup) = store::delete_build(app_id, &input.build_id).await {
            tracing::warn!("publish rollback: orphan prefix left for {app_id}: {cleanup}");
        }
        if let Ok(Some(row)) = app_builds::Entity::find_by_id(build_pk).one(&db).await {
            let _ = row.delete(&db).await;
        }
        return Err(e);
    }
    gc_builds(&db, app_id, &[build_pk]).await;

    if let Some(bytes) = index_bytes {
        cache::seed(app_id, &input.build_id, "index.html", bytes);
    }

    Ok(PublishResult {
        app_id,
        build_id: input.build_id,
        url: format!("/customer-apps/{}/{}/", org.slug, input.app_slug),
        channel: if input.promote { "published" } else { "draft" }.to_string(),
        org_slug: org.slug.clone(),
        is_new_app,
    })
}

/// `POST /api/customer-apps/publish` — thin multipart shim over [`publish`].
///
/// The app-admin guard middleware has already authenticated the caller; we
/// pull the user via `AuthenticatedUserExtractor` (before `Multipart`, which
/// consumes the body) to stamp `published_by` on the build.
pub async fn publish_handler(
    oxy_auth::extractor::AuthenticatedUserExtractor(user): oxy_auth::extractor::AuthenticatedUserExtractor,
    mut multipart: Multipart,
) -> Result<Json<PublishResult>, (StatusCode, String)> {
    let mut org: Option<String> = None;
    let mut org_id: Option<Uuid> = None;
    // Tracks whether the publisher sent a non-empty `org_id` that
    // didn't parse — we surface that as a 400 instead of silently
    // falling back to `org`, so a typo'd UUID doesn't end up landing
    // the publish on a different org's row that happens to match an
    // unrelated `org=` slug.
    let mut org_id_invalid: Option<String> = None;
    let mut app = None;
    let mut project_id = None;
    let mut branch = None;
    let mut build_id = None;
    let mut name = None;
    let mut promote = false;
    let mut manifest = None;
    let mut tarball: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("multipart error: {e}")))?
    {
        match field.name().unwrap_or_default() {
            "bundle" => {
                tarball = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| (StatusCode::BAD_REQUEST, format!("bundle read: {e}")))?
                        .to_vec(),
                );
            }
            "manifest" => {
                let raw = field.text().await.unwrap_or_default();
                if !raw.trim().is_empty() {
                    manifest = serde_json::from_str(&raw).ok();
                }
            }
            field_name => {
                let key = field_name.to_string();
                let val = field.text().await.unwrap_or_default();
                match key.as_str() {
                    // `org` accepts either a slug or a UUID — auto-detected
                    // by `OrgRef::from_str_auto` below. `org_id` is the
                    // explicit alias for the UUID form; if both are present
                    // `org_id` wins (older clients that send just `org=`
                    // still work unchanged).
                    "org" => org = Some(val),
                    "org_id" => {
                        let trimmed = val.trim();
                        if trimmed.is_empty() {
                            // Empty string is treated as "field omitted"
                            // (consistent with how other multipart fields
                            // are handled here).
                        } else {
                            match Uuid::parse_str(trimmed) {
                                Ok(id) => org_id = Some(id),
                                Err(_) => org_id_invalid = Some(trimmed.to_string()),
                            }
                        }
                    }
                    "app" => app = Some(val),
                    "project" | "project_id" => project_id = Uuid::parse_str(val.trim()).ok(),
                    "branch" => branch = Some(val).filter(|s| !s.is_empty()),
                    "build_id" => build_id = Some(val).filter(|s| !s.is_empty()),
                    "name" => name = Some(val).filter(|s| !s.is_empty()),
                    "channel" => promote = val == "published",
                    "promote" => promote = val == "true" || val == "1",
                    _ => {}
                }
            }
        }
    }

    // Reject a bundle-supplied display name with control/JSON-breaking chars
    // before it lands in the apps row (defense against injection via the
    // uploaded manifest / multipart field).
    if let Some(n) = &name {
        crate::server::api::admin::apps::handlers::validate_display_name(n)
            .map_err(|msg| (StatusCode::BAD_REQUEST, msg))?;
    }

    if let Some(bad) = org_id_invalid {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("org_id is not a valid UUID: {bad:?}"),
        ));
    }
    let org_ref = match (org_id, org) {
        (Some(id), _) => OrgRef::Id(id),
        (None, Some(s)) => OrgRef::from_str_auto(&s),
        (None, None) => {
            return Err((
                StatusCode::BAD_REQUEST,
                "missing org: provide `org` (slug or UUID) or `org_id` (UUID)".into(),
            ));
        }
    };

    let input = PublishInput {
        org_ref,
        app_slug: app.ok_or((StatusCode::BAD_REQUEST, "missing app".into()))?,
        project_id: project_id
            .ok_or((StatusCode::BAD_REQUEST, "missing/invalid project".into()))?,
        branch,
        build_id: build_id.unwrap_or_else(|| Uuid::new_v4().simple().to_string()),
        name,
        promote,
        tarball: tarball.ok_or((StatusCode::BAD_REQUEST, "missing bundle".into()))?,
        manifest,
        published_by: Some(user.id),
    };

    publish(input)
        .await
        .map(Json)
        .map_err(|e| (e.status(), e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;

    fn make_tar_gz(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, bytes) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, path, *bytes).unwrap();
        }
        let tar_bytes = builder.into_inner().unwrap();
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        enc.write_all(&tar_bytes).unwrap();
        enc.finish().unwrap()
    }

    #[test]
    fn unpack_roundtrips_and_strips_dot_slash() {
        let gz = make_tar_gz(&[
            ("./index.html", b"<html>"),
            ("assets/app.js", b"console.log(1)"),
        ]);
        let files = unpack_tar_gz(&gz).expect("unpack");
        assert!(
            files
                .iter()
                .any(|(p, b)| p == "index.html" && b == b"<html>")
        );
        assert!(files.iter().any(|(p, _)| p == "assets/app.js"));
    }

    #[test]
    fn unpack_rejects_missing_index() {
        let gz = make_tar_gz(&[("assets/app.js", b"x")]);
        let err = unpack_tar_gz(&gz).unwrap_err();
        assert!(matches!(err, PublishError::BadTarball(_)));
    }

    #[test]
    fn function_specs_extracts_declared_functions() {
        let manifest = serde_json::json!({
            "slug": "hello-oxy",
            "functions": {
                "top-stores": { "route": true, "timeoutSeconds": 15 },
            },
        });
        let specs = function_specs(Some(&manifest));
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].0, "top-stores");
        assert_eq!(specs[0].1["route"], serde_json::json!(true));
        assert_eq!(specs[0].1["timeoutSeconds"], serde_json::json!(15));
    }

    #[test]
    fn function_specs_empty_when_no_functions_block() {
        // A static-bundle manifest (today's default) records no functions.
        assert!(function_specs(Some(&serde_json::json!({ "slug": "x" }))).is_empty());
        assert!(function_specs(None).is_empty());
        // A present-but-empty block is also a no-op.
        assert!(function_specs(Some(&serde_json::json!({ "functions": {} }))).is_empty());
    }

    #[test]
    fn function_artifact_key_matches_build_store_layout() {
        assert_eq!(
            function_artifact_key("customer-apps/abc/builds/v1/", "top-stores"),
            "customer-apps/abc/builds/v1/functions/top-stores.js"
        );
    }

    #[test]
    fn unpack_path_guard_rejects_unsafe() {
        // The tar *writer* already refuses to emit `..`, so we exercise the
        // guard predicate directly — it must reject traversal and absolute
        // paths from archives built by other tools, and allow normal ones.
        use std::path::Path;
        assert!(!is_safe_relative_path(Path::new("../escape.html")));
        assert!(!is_safe_relative_path(Path::new("a/../../b")));
        assert!(!is_safe_relative_path(Path::new("/abs/secret")));
        assert!(is_safe_relative_path(Path::new("index.html")));
        assert!(is_safe_relative_path(Path::new("assets/app.js")));
    }

    #[test]
    fn humanize_slug_titlecases_words() {
        assert_eq!(humanize_slug("weekly-sales"), "Weekly Sales");
        assert_eq!(humanize_slug("dashboard"), "Dashboard");
    }

    #[test]
    fn oxy_access_denied_is_forbidden() {
        let e = PublishError::OxyAccessDenied {
            org: "acme".to_string(),
            project: Uuid::nil(),
        };
        assert_eq!(e.status(), StatusCode::FORBIDDEN);
        assert!(e.to_string().contains("has not granted Oxy access"));
    }
}
