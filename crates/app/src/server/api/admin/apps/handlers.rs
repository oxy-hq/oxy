//! Admin endpoints for the customer-apps registry. Gated by oxy_owner_guard
//! at the router layer (mounted under /admin in router/global.rs).

use axum::Json;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use chrono::Utc;
use entity::apps;
use entity::org_members;
use entity::organizations;
use entity::prelude::{AppBuilds, Apps, OrgMembers, Organizations};
use oxy::database::client::establish_connection;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_shared::utils::slugify;
use sea_orm::ActiveModelTrait;
use sea_orm::ActiveValue;
use sea_orm::ColumnTrait;
use sea_orm::DatabaseConnection;
use sea_orm::EntityTrait;
use sea_orm::ModelTrait;
use sea_orm::QueryFilter;
use sea_orm::QueryOrder;
use sea_orm::QuerySelect;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::server::api::customer_apps_source::SourceSpec;

/// Standard JSON error body for 4xx/5xx responses. The frontend reads
/// `err.response.data.message` for actionable messaging in the create
/// dialog, so every fail path here surfaces a `message` field rather
/// than relying on the status code alone.
#[derive(Serialize, Debug)]
pub struct ErrorBody {
    pub message: String,
}

/// Tuple form axum recognises as a response: `(StatusCode, Json<body>)`.
/// Use this for all 4xx returns from the apps admin handlers.
pub type ApiErr = (StatusCode, Json<ErrorBody>);

/// Build an [`ApiErr`] with a custom message. Prefer this for paths
/// the user can fix (slug collision, missing org, malformed input);
/// use `internal()` for unexpected failures the operator can't act on.
fn api_err(status: StatusCode, message: impl Into<String>) -> ApiErr {
    (
        status,
        Json(ErrorBody {
            message: message.into(),
        }),
    )
}

/// 500-equivalent for `.map_err`. Keeps call sites short while still
/// producing a body the frontend can show consistently.
fn internal(_: impl std::fmt::Display) -> ApiErr {
    api_err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error.")
}

#[derive(Deserialize, Debug)]
pub struct CreateAppRequest {
    pub name: String,
    /// Owning org. The admin UI's org picker resolves the org by name and
    /// supplies the uuid directly — no slug lookup required.
    pub org_id: Uuid,
    pub project_id: Uuid,
    #[serde(default = "default_branch")]
    pub branch: String,
    /// Optional URL slug override. If absent, derived from `name` and
    /// de-duplicated within the org by appending `-2`, `-3`, … on collision.
    /// Must match the same shape as auto-derived slugs when provided.
    #[serde(default)]
    pub slug: Option<String>,
    /// Where the app's bundle comes from. Default to s3 so older clients
    /// that omit this field keep behaving exactly as before.
    #[serde(default = "default_source")]
    pub source: SourceSpec,
    /// When true and `source` is `s3`, open a PR on
    /// `OXY_CUSTOMER_APPS_REPO` scaffolding the apps/<org>/<slug>/ folder
    /// before returning. PR URL ends up on `bootstrap_pr_url`.
    #[serde(default)]
    pub scaffold_pr: bool,
    /// When true and `source` is `local` with an empty `path`, oxy
    /// creates `$OXY_STATE_DIR/customer-apps/<uuid>/source/` itself
    /// and pre-populates `source_config.path` with that path. Lets
    /// the Create-new-app dialog hand the engineer a ready-made
    /// folder without making them think about the filesystem layout.
    ///
    /// Rejected when `OXY_STATE_DIR` is unset or when the deployment
    /// is in cloud mode (no engineer-reachable filesystem to write
    /// to).
    #[serde(default)]
    pub provision_local_source: bool,
    /// Curated template id to scaffold from. Defaults to `"vite"` when
    /// absent (back-compat). Validated against the registry; unknown
    /// ids return 400 before any row is inserted.
    #[serde(default)]
    pub template_id: Option<String>,
    /// Stable bundle identifier — the `<repo-org>/<repo-slug>` path
    /// under the customer-apps git repo where this bundle's source
    /// lives. Drives the S3 key
    /// (`customer-apps/<repo_path>/{draft,published}/...`) so the
    /// bundle has the same storage path across every environment.
    ///
    /// Only meaningful for `source: s3`. Defaults to `<org_slug>/<slug>`
    /// when absent — covers the common case where the operator's
    /// admin-row identity matches the repo layout. Operators with
    /// per-env slug drift type this field explicitly so dev and prod
    /// stay aligned.
    #[serde(default)]
    pub repo_path: Option<String>,
}

fn default_source() -> SourceSpec {
    SourceSpec::S3
}

fn default_branch() -> String {
    "main".to_string()
}

/// Resolve and validate a caller-supplied `template_id` against the
/// registry. Returns `"vite"` when `id` is `None` (back-compat). Returns
/// an error string when `id` is `Some` but not registered.
///
/// Extracted as a pure helper so unit tests can exercise the validation
/// logic without spinning up an HTTP server or a database connection.
pub(crate) fn validate_template_id(id: Option<&str>) -> Result<&str, String> {
    let id = id.unwrap_or("vite");
    if crate::customer_app_template::registry::get_template(id).is_none() {
        return Err(format!("unknown template_id: {id}"));
    }
    Ok(id)
}

/// Validate a human-facing display name before it gets substituted
/// into JSON (`oxy-app.json`, `package.json`) and JSX (`App.tsx`)
/// templates. Rejects characters that would break the host language —
/// quotes, backslashes, angle brackets (HTML/JSX), curly braces (JSX
/// expressions), and ASCII control chars including newlines.
///
/// The substituter is intentionally a dumb string-replace so it can
/// be reused by both Rust and TypeScript code paths; pushing
/// validation to the boundary keeps the substituter simple and stops
/// the whole class of injection (JSON-shape, JSX-shape, HTML
/// fragment) at one place. Names that need richer characters can be
/// updated post-scaffold by editing the generated files directly.
pub(crate) fn validate_display_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("name must not be empty".into());
    }
    // Count unicode scalar values, not bytes — the error message says
    // "characters" and a CJK app name of 43 characters is otherwise
    // rejected at 129 bytes (3 bytes per char in UTF-8).
    if trimmed.chars().count() > 128 {
        return Err("name must be at most 128 characters".into());
    }
    for c in trimmed.chars() {
        if c.is_control() {
            return Err("name must not contain control characters or newlines".into());
        }
        if matches!(c, '"' | '\\' | '<' | '>' | '{' | '}') {
            return Err(format!(
                "name must not contain the character {c:?} (would break the scaffolded JSON/JSX)"
            ));
        }
    }
    Ok(())
}

#[derive(Serialize, Clone, Debug)]
pub struct AppResponse {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub org_id: Uuid,
    /// Denormalised on the response so the frontend doesn't have to parse
    /// the URL to build a sync path. Source of truth is the orgs table.
    pub org_slug: String,
    pub project_id: Uuid,
    pub branch: String,
    pub source_repo: String,
    pub status: String,
    /// Canonical pretty URL `<base>/customer-apps/<org_slug>/<app_slug>/`.
    /// Always set; works for every source_type.
    pub url: String,
    /// Subdomain URL for v0 sources when this cluster has
    /// `OXY_CUSTOMER_APPS_SUBDOMAIN_SUFFIX` configured, e.g.
    /// `https://mars--command-center.customer-apps-dev.oxygen-hq.com/`.
    /// `None` otherwise — the admin UI shows whichever URLs are present
    /// and hides the row when both are unavailable for the current source.
    pub url_subdomain: Option<String>,
    pub source_type: String,
    pub source_config: serde_json::Value,
    /// Set after a successful PR scaffold; null otherwise.
    pub bootstrap_pr_url: Option<String>,
    pub last_synced_at: Option<String>,
    /// Set by `POST /api/admin/apps/{id}/publish`. NULL = draft.
    /// Customers (non-app-admins) only see / can reach an app when this
    /// is set; app admins always see.
    pub published_at: Option<String>,
    /// Stable bundle identifier in the customer-apps git repo
    /// (`<repo-org>/<repo-slug>`). Drives the S3 key. NULL on
    /// non-S3 sources; on S3 sources, defaults to the row's
    /// `<org_slug>/<slug>` when not explicitly overridden.
    pub repo_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// `MAX(custom_app_view_event.viewed_at)` for this app, or `None`
    /// when nobody has opened the app yet. Drives the list-level
    /// "last active" column on the Custom apps admin page so operators
    /// can sort by "stale apps". Populated by `list_apps` via a single
    /// batched query — `from_model_with_org` leaves it `None` because
    /// per-row queries would be N+1 on the list page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_active_at: Option<String>,
    /// Email of whoever last promoted a build for this app (published /
    /// made-live / rolled back). Resolved by `list_apps` in one batched
    /// query; `None` on the cheap single responses (the detail view reads
    /// richer per-build attribution from `/builds`). Drives the list's
    /// "promoted by" line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_promoted_by_email: Option<String>,
    /// When that last promotion happened. Taken straight from the model
    /// column on every response, so the UI can show "promoted 2d ago".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_promoted_at: Option<String>,
    /// Manifest-derived app glyph URL (`<url><manifest.icon>`), or `None` when
    /// the app declares no `icon`. Same source + shape the homepage launcher
    /// uses (there is no favicon.ico probe); the frontend renders it with a
    /// monogram fallback via `AppMark`. Populated by the list/get handlers via
    /// the shared resolver. See the `oxy-app-visual-identity` skill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    /// Manifest-derived preview-image URL (`<url><manifest.art>`), or `None`.
    /// Rendered with a letter-tile fallback via `AppArt`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub art_url: Option<String>,
    /// Soft warnings the UI should surface to the operator. Populated
    /// by `create_app` + `update_app` after side-effect validation
    /// (e.g. "no index.html at the configured local path"). The row
    /// itself was still persisted — these are hints, not errors. List
    /// + Get endpoints leave this empty to keep them cheap.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Build the canonical pretty URL for an app.
///
/// Returns a **relative** URL (`/customer-apps/<org>/<app>/`); the client
/// renders it against its own origin. Customer-app bundles share the SPA's
/// domain in the current model — no whitelabelling yet — so no per-host
/// prefix is needed. (When whitelabelling lands, the right surface will be
/// per-app config in the DB, not a global env var.)
pub(crate) fn build_pretty_url(org_slug: &str, app_slug: &str) -> String {
    format!("/customer-apps/{org_slug}/{app_slug}/")
}

impl AppResponse {
    fn from_model_with_org(m: apps::Model, org_slug: &str) -> Self {
        let url = build_pretty_url(org_slug, &m.slug);
        // Subdomain URL applies to every source type that gets served
        // through the customer-apps surface — both v0 (reverse-proxied
        // to Vercel) and s3 (served from this oxy backend's bundle
        // cache). The host dispatcher's `already_canonicalized` guard
        // (#2466) made S3 apps work on the subdomain too, but this
        // function still had the original v0-only gate from when the
        // feature first shipped — so the admin UI surfaced subdomain
        // URLs only for v0 apps and operators wondered why their
        // `oxy publish`-deployed apps showed just the subpath. Drop
        // the gate; `subdomain_url_for` returns None when the
        // cluster's admin host doesn't fit the auto-derivation
        // convention (local dev / custom-branded host), which is the
        // only case where the row should be hidden.
        let url_subdomain =
            crate::server::api::customer_apps_host_dispatch::subdomain_url_for(org_slug, &m.slug);
        Self {
            id: m.id,
            slug: m.slug,
            name: m.name,
            org_id: m.org_id,
            org_slug: org_slug.to_string(),
            project_id: m.project_id,
            branch: m.branch,
            source_repo: m.source_repo,
            status: m.status,
            url,
            url_subdomain,
            source_type: m.source_type,
            source_config: m.source_config,
            bootstrap_pr_url: m.bootstrap_pr_url,
            last_synced_at: m.last_synced_at.map(|d| d.to_rfc3339()),
            published_at: m.published_at.map(|d| d.to_rfc3339()),
            repo_path: m.repo_path,
            created_at: m.created_at.to_rfc3339(),
            updated_at: m.updated_at.to_rfc3339(),
            last_active_at: None,
            last_promoted_by_email: None,
            last_promoted_at: m.last_promoted_at.map(|d| d.to_rfc3339()),
            // Manifest-derived; left None on the cheap constructor and filled by
            // the list/get handlers (which have `db`) via the batched resolver
            // (`icon_art_by_app` / `resolve_manifests_batch`).
            icon_url: None,
            art_url: None,
            warnings: Vec::new(),
        }
    }
}

/// Validate a LocalFolder app's configured path. Returns operator-
/// facing warning strings — nothing here is an error (the row is
/// already persisted); these are hints the UI surfaces as toasts so
/// the operator catches a misconfigured path BEFORE they click
/// Preview and stare at a broken iframe.
///
/// Three things make a path "wrong" in different ways:
///   1. Empty / unset — path was never configured
///   2. Path exists but isn't a directory
///   3. Path is a directory but has no `index.html`
///
/// All three produce a single combined message because they share a
/// fix (set / correct the path to point at the build output).
fn validate_local_source(
    source_type: &str,
    source_config: &serde_json::Value,
    org_slug: &str,
    app_slug: &str,
) -> Vec<String> {
    if source_type != "local" {
        return Vec::new();
    }
    let raw = source_config
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if raw.trim().is_empty() {
        return vec![
            "Local source has no path configured. Set it from Settings → Local bundle path."
                .to_string(),
        ];
    }
    let path = std::path::Path::new(raw);
    match path.metadata() {
        Ok(meta) if !meta.is_dir() => {
            vec![format!(
                "Local bundle path {raw:?} exists but isn't a directory. \
                 Point at the folder that holds index.html (Next.js export → out/, Vite → dist/)."
            )]
        }
        Ok(_) => {
            if path.join("index.html").exists() {
                check_baked_base_path(path, org_slug, app_slug)
            } else {
                vec![format!(
                    "Local bundle path {raw:?} has no index.html. \
                     Did you run `pnpm build`? Or point at the build output dir \
                     (Next.js export → out/, Vite → dist/)."
                )]
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            vec![format!(
                "Local bundle path {raw:?} doesn't exist on the oxy host. \
                 Check the path and try again."
            )]
        }
        Err(e) => {
            // Permission denied or other transient — surface verbatim
            // so the operator can act on it.
            vec![format!("Local bundle path {raw:?} can't be read: {e}.")]
        }
    }
}

/// Read the bundle's `index.html`, extract the baked
/// `/customer-apps/<org>/<slug>/` prefix, and warn if it doesn't
/// match the chosen `<org_slug>/<app_slug>`. This is the warning we
/// most need to surface at link time — the serve-time rewrite patches
/// `index.html` but cannot reach into the bundle's JS chunks, so a
/// slug-vs-baked mismatch means every data fetch from the bundle 404s
/// and the dashboard sits forever at "Loading…".
fn check_baked_base_path(
    bundle_dir: &std::path::Path,
    org_slug: &str,
    app_slug: &str,
) -> Vec<String> {
    let Ok(bytes) = std::fs::read(bundle_dir.join("index.html")) else {
        return Vec::new();
    };
    let Ok(html) = std::str::from_utf8(&bytes) else {
        return Vec::new();
    };
    let Some(baked) = crate::server::api::customer_apps_serve::first_customer_apps_prefix(html)
    else {
        // Bundle doesn't reference any /customer-apps/* prefix —
        // probably built without OXY_APP_BASE_PATH. The serve-time
        // path rewrite handles this case by injecting the expected
        // prefix, so it's not actionable here.
        return Vec::new();
    };
    let expected = format!("/customer-apps/{org_slug}/{app_slug}/");
    if baked == expected {
        return Vec::new();
    }
    vec![format!(
        "Bundle was built with base path {baked:?} baked in, but this app is \
         registered as {expected:?}. The JS chunks fetch from the baked path and \
         will 404 every data product (the dashboard will sit at 'Loading…' \
         forever). Fix by either rebuilding with OXY_APP_BASE_PATH={expected} \
         or changing the app slug to match the baked path."
    )]
}

/// Create `$OXY_STATE_DIR/customer-apps/<id>/source/` for a freshly
/// inserted local-source app. Returns the canonical path on success.
/// Failures are typed back as the HTTP status the caller should
/// surface so the rollback path stays simple.
async fn provision_local_dir_for(id: Uuid) -> Result<std::path::PathBuf, ApiErr> {
    let state_root = std::env::var("OXY_STATE_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            api_err(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Cannot provision a local bundle dir: OXY_STATE_DIR is not set.",
            )
        })?;

    let dir = std::path::PathBuf::from(state_root)
        .join("customer-apps")
        .join(id.to_string())
        .join("source");

    tokio::fs::create_dir_all(&dir).await.map_err(|e| {
        tracing::error!("create_dir_all({}): {e}", dir.display());
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Couldn't create bundle directory at {}.", dir.display()),
        )
    })?;

    Ok(dir)
}

/// Bulk-resolve org_id → org_slug for a batch of apps. One query regardless
/// of how many apps; missing orgs (deleted out from under us — should be
/// impossible thanks to the FK cascade, but just in case) get a fallback
/// "unknown-org" slug so the response still serialises.
async fn org_slugs_for(
    db: &DatabaseConnection,
    apps: &[apps::Model],
) -> Result<std::collections::HashMap<Uuid, String>, sea_orm::DbErr> {
    use std::collections::HashSet;
    let ids: HashSet<Uuid> = apps.iter().map(|a| a.org_id).collect();
    if ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let rows = Organizations::find()
        .filter(organizations::Column::Id.is_in(ids.iter().copied()))
        .all(db)
        .await?;
    Ok(rows.into_iter().map(|o| (o.id, o.slug)).collect())
}

#[derive(Serialize)]
pub struct BuildConfigResponse {
    pub project_id: Uuid,
    pub branch: String,
    /// Org slug the app is registered under. Echoed back so the
    /// customer-apps `just build` recipe can construct the exact
    /// `OXY_APP_BASE_PATH=/customer-apps/<org>/<slug>/` value without
    /// having to ask the operator — the org might differ from the
    /// folder name (a bundle in `apps/pokehouse/franchise-report/`
    /// can be linked under `test` for local smoke-testing, and the
    /// build still needs to bake the linked org).
    pub org_slug: String,
    /// App slug as registered. Mirrors the URL slug exactly; useful
    /// for the same OXY_APP_BASE_PATH derivation and as a sanity
    /// check against the bundle's own oxy-app.json.
    pub app_slug: String,
}

/// Public endpoint — returns the build-time config for an app by pretty
/// path. Read by the customer-apps CI workflow (and `just build`) before
/// running `pnpm build` so no per-app env config has to live in the
/// customer-apps repo.
///
/// Public on purpose: project_id, branch, and the slugs themselves are
/// not secrets — they're already in the URL. The data behind them is
/// gated by org membership at `/api/*`.
pub async fn get_build_config(
    Path((org_slug, app_slug)): Path<(String, String)>,
) -> Result<Json<BuildConfigResponse>, StatusCode> {
    let db = establish_connection()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (org, row) = lookup_by_pretty_path(&db, &org_slug, &app_slug)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(BuildConfigResponse {
        project_id: row.project_id,
        branch: row.branch,
        org_slug: org.slug,
        app_slug: row.slug,
    }))
}

pub async fn create_app(Json(req): Json<CreateAppRequest>) -> Result<Json<AppResponse>, ApiErr> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("DB connection error: {e}");
        internal(e)
    })?;

    // Validate the template_id up-front before any side effects so we
    // return 400 immediately for unknown ids without touching the DB.
    let template_id = validate_template_id(req.template_id.as_deref())
        .map_err(|msg| api_err(StatusCode::BAD_REQUEST, msg))?;

    // Validate the display name: rejects chars that would break the
    // scaffolded JSON / JSX so a poisonous name can't produce a
    // bundle that fails the probe (or worse) after we open the PR.
    validate_display_name(&req.name).map_err(|msg| api_err(StatusCode::BAD_REQUEST, msg))?;

    // Resolve org first — we need org.slug for the URL we return.
    let org = Organizations::find_by_id(req.org_id)
        .one(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to look up org {}: {e}", req.org_id);
            internal(e)
        })?
        .ok_or_else(|| {
            api_err(
                StatusCode::NOT_FOUND,
                format!("Organization {} not found.", req.org_id),
            )
        })?;

    // Slug: caller-supplied wins, but we still validate + collision-check.
    // No caller slug? Auto-derive from name and dedupe with `-2`, `-3`, …
    // until unique within this org.
    let slug = match req.slug.as_deref() {
        Some(s) => {
            let s = s.trim();
            if !is_valid_slug(s) {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    format!(
                        "Slug {s:?} isn't valid. Use lowercase letters, digits, and \
                         single hyphens; 1–63 chars; no leading/trailing/double hyphens."
                    ),
                ));
            }
            if slug_taken_in_org(&db, req.org_id, s).await? {
                return Err(api_err(
                    StatusCode::CONFLICT,
                    format!(
                        "An app with slug {s:?} already exists in org {:?}. \
                         The slug is locked to the bundle's baked OXY_APP_BASE_PATH, \
                         so the existing app needs to be deleted (or renamed) before \
                         you can link this bundle.",
                        org.slug
                    ),
                ));
            }
            s.to_string()
        }
        None => unique_slug_for_name(&db, req.org_id, &req.name).await?,
    };

    let id = Uuid::new_v4();
    let now = Utc::now().fixed_offset();
    let (source_type, source_config) = req.source.into_columns();

    // `repo_path` is the stable cross-env identifier for S3-sourced
    // bundles. Operator-overridable; defaults to the row's
    // `<org_slug>/<slug>` pair so the common case (admin row name
    // matches the repo path) requires no extra input. For non-S3
    // sources we record None — those source types don't read from S3.
    let repo_path: Option<String> = match source_type.as_str() {
        "s3" => Some(
            req.repo_path
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.trim_matches('/').to_string())
                .unwrap_or_else(|| format!("{}/{slug}", org.slug)),
        ),
        _ => None,
    };

    // Only S3 apps have a draft/published bundle separation worth
    // gating on — Local and V0 are essentially "the engineer already
    // controls the bundle", so we auto-publish them on create. That
    // way they show up in the customer sidebar immediately, and the
    // explicit Publish button is a no-op for them (a sidebar
    // visibility toggle, not a deploy step).
    let initial_published_at = match source_type.as_str() {
        "s3" => ActiveValue::NotSet,
        _ => ActiveValue::Set(Some(now)),
    };

    let model = apps::ActiveModel {
        id: ActiveValue::Set(id),
        slug: ActiveValue::Set(slug),
        name: ActiveValue::Set(req.name),
        org_id: ActiveValue::Set(req.org_id),
        project_id: ActiveValue::Set(req.project_id),
        branch: ActiveValue::Set(req.branch),
        source_repo: ActiveValue::Set("oxy-hq/customer-apps".to_string()),
        status: ActiveValue::Set("created".to_string()),
        source_type: ActiveValue::Set(source_type),
        source_config: ActiveValue::Set(source_config),
        bootstrap_pr_url: ActiveValue::NotSet,
        last_synced_at: ActiveValue::NotSet,
        // No per-deployment override on create — defaults to "use the
        // bundle's bundled oxy-app.json." CI sets the override later
        // via `oxy apps ensure --manifest-override` when one bundle
        // template needs to back multiple customers.
        manifest_override: ActiveValue::NotSet,
        published_at: initial_published_at,
        repo_path: ActiveValue::Set(repo_path),
        draft_build_id: ActiveValue::NotSet,
        published_build_id: ActiveValue::NotSet,
        last_promoted_by: ActiveValue::NotSet,
        last_promoted_at: ActiveValue::NotSet,
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
    };

    let inserted = model.insert(&db).await.map_err(|e| {
        // Defensive 409 — the in-process dedup above should have caught any
        // collision, but a race between two concurrent admins could still
        // hit the unique index.
        if oxy::database::errors::is_unique_violation(&e) {
            return api_err(
                StatusCode::CONFLICT,
                "Another admin just created an app with this slug. Retry.",
            );
        }
        tracing::error!("Failed to insert app: {e}");
        internal(e)
    })?;

    let mut row = inserted;

    // Local-source provisioning: mkdir the engineer's empty folder
    // under the state dir and update the row's source_config.path so
    // the serve handler can find it. Failure rolls back the row so
    // we never leave behind an app whose configured path doesn't
    // exist on disk (the silent-loading-iframe class of bug).
    if req.provision_local_source && row.source_type == "local" {
        match provision_local_dir_for(row.id).await {
            Ok(path) => {
                let active = apps::ActiveModel {
                    id: ActiveValue::Unchanged(row.id),
                    source_config: ActiveValue::Set(serde_json::json!({
                        "path": path.display().to_string(),
                    })),
                    updated_at: ActiveValue::Set(Utc::now().fixed_offset()),
                    ..Default::default()
                };
                match active.update(&db).await {
                    Ok(updated) => {
                        row = updated;
                    }
                    Err(e) => {
                        tracing::error!("Failed to persist provisioned local path: {e}");
                        // Best-effort cleanup of the freshly-created dir
                        // before we drop the row — leaves no debris.
                        let _ = tokio::fs::remove_dir_all(&path).await;
                        let _ = row.clone().delete(&db).await;
                        return Err(internal(e));
                    }
                }
            }
            Err(err) => {
                tracing::error!(
                    "Local-source provisioning failed for {}: {:?}; rolling back app row",
                    row.id,
                    err.1
                );
                let _ = row.clone().delete(&db).await;
                return Err(err);
            }
        }
    }

    // If the caller asked for a scaffold PR and the source is s3, open one
    // synchronously. Failure rolls the row back so we never persist an app
    // whose caller asked for a PR and didn't get one — the only state that
    // exists post-handler is the state the response reflects.
    if req.scaffold_pr && row.source_type == "s3" {
        match crate::server::api::customer_apps_scaffold::scaffold_pr(&db, &row, &org, template_id)
            .await
        {
            Ok(pr_url) => {
                let active = apps::ActiveModel {
                    id: ActiveValue::Unchanged(row.id),
                    bootstrap_pr_url: ActiveValue::Set(Some(pr_url.clone())),
                    updated_at: ActiveValue::Set(Utc::now().fixed_offset()),
                    ..Default::default()
                };
                row = active.update(&db).await.map_err(|e| {
                    tracing::error!("Failed to persist bootstrap_pr_url: {e}");
                    internal(e)
                })?;
            }
            Err(err) => {
                tracing::error!(
                    "PR scaffold failed for {}: {err}; rolling back app row",
                    row.id
                );
                let _ = row.clone().delete(&db).await;
                return Err(api_err(
                    StatusCode::BAD_GATEWAY,
                    format!("Couldn't open scaffold PR: {err}"),
                ));
            }
        }
    }

    let warnings =
        validate_local_source(&row.source_type, &row.source_config, &org.slug, &row.slug);
    let mut resp = AppResponse::from_model_with_org(row, &org.slug);
    resp.warnings = warnings;
    Ok(Json(resp))
}

#[derive(Deserialize, Debug, Default)]
pub struct ListAppsQuery {
    #[serde(default = "default_limit")]
    pub limit: u64,
    #[serde(default)]
    pub offset: u64,
}

/// Page size for the admin app list. Tuned for "recently active is
/// usually what you want" — 50 covers a working session without
/// requiring scroll for typical org sizes, and keeps the first
/// payload small enough to render quickly even with a few hundred
/// apps in the DB.
fn default_limit() -> u64 {
    50
}

#[derive(Serialize)]
pub struct ListAppsResponse {
    pub items: Vec<AppResponse>,
    /// Offset for the next page. `None` when this response returned
    /// fewer items than `limit` (= we're at the tail). Lets the
    /// frontend's infinite query stop fetching without a separate
    /// `total` round trip.
    pub next_offset: Option<u64>,
}

/// Resolve a set of user ids to their emails in one query. Deduped by the
/// `IN` clause; missing/legacy ids simply don't appear in the map.
async fn emails_by_user_id(
    db: &DatabaseConnection,
    ids: Vec<Uuid>,
) -> Result<std::collections::HashMap<Uuid, String>, sea_orm::DbErr> {
    if ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    Ok(entity::prelude::Users::find()
        .filter(entity::users::Column::Id.is_in(ids))
        .all(db)
        .await?
        .into_iter()
        .map(|u| (u.id, u.email))
        .collect())
}

/// Ceiling on `list_apps` page size — bounds the per-page batched lookups
/// (org slugs, promoter emails, last-active, manifests) so a caller-supplied
/// `?limit=` can't turn one admin request into an unbounded scan.
const MAX_LIST_LIMIT: u64 = 200;

/// Build the per-app `(icon_url, art_url)` map for a page, resolving every
/// manifest in ONE batched `app_builds` query (no N+1) and turning them into
/// URLs with the same helper the homepage launcher uses — so admin + launcher
/// agree on the picture. Published build preferred, draft as fallback (handled
/// in the batch resolver). Metadata: unresolved apps get `(None, None)`. See
/// the `oxy-app-visual-identity` skill.
async fn icon_art_by_app(
    db: &sea_orm::DatabaseConnection,
    rows: &[apps::Model],
    org_slugs: &std::collections::HashMap<Uuid, String>,
) -> std::collections::HashMap<Uuid, (Option<String>, Option<String>)> {
    let manifests =
        crate::server::api::customer_apps_manifest::resolve_manifests_batch(db, rows).await;
    rows.iter()
        .filter_map(|app| {
            let slug = org_slugs.get(&app.org_id)?;
            Some((
                app.id,
                crate::server::api::workspace_custom_apps::icon_art_urls(
                    manifests.get(&app.id),
                    slug,
                    &app.slug,
                ),
            ))
        })
        .collect()
}

pub async fn list_apps(
    Query(q): Query<ListAppsQuery>,
) -> Result<Json<ListAppsResponse>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("DB connection error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    // Clamp the caller-supplied page size — the per-page batched lookups below
    // scale with it, so an unbounded `?limit=` shouldn't be operator-tunable.
    let limit = q.limit.clamp(1, MAX_LIST_LIMIT);
    // Sort by `updated_at` DESC so the most recently touched apps
    // (whether that's a sync, a config edit, or a publish) sit at the
    // top of the page. Pairs with the frontend's `useInfiniteQuery`
    // so "load more" walks back in time from the most recent.
    let rows = Apps::find()
        .order_by_desc(apps::Column::UpdatedAt)
        .limit(Some(limit))
        .offset(q.offset)
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list apps: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let returned = rows.len() as u64;
    let org_slugs = org_slugs_for(&db, &rows).await.map_err(|e| {
        tracing::error!("Failed to load org slugs for apps list: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    // Batch-load last-active timestamps for the page (single query;
    // N+1 would be brutal on an org with 100+ apps). Failures fall
    // back to `None` — the list page should never 500 because the
    // tracking table is unavailable.
    let app_ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let last_active =
        crate::server::api::customer_apps_activity::last_active_at_by_app(&db, &app_ids)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("last_active_at_by_app failed (filling Nones): {e}");
                Default::default()
            });
    // Map each app → its last promoter, then resolve those user ids to emails
    // in one query (same N+1-avoidance as last_active). A failed lookup falls
    // back to no attribution rather than 500ing the list.
    let promoter_by_app: std::collections::HashMap<Uuid, Uuid> = rows
        .iter()
        .filter_map(|r| r.last_promoted_by.map(|p| (r.id, p)))
        .collect();
    let promoter_emails = emails_by_user_id(&db, promoter_by_app.values().copied().collect())
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("promoter email lookup failed (no attribution): {e}");
            Default::default()
        });
    // Manifest-derived icon/art for the whole page in ONE batched query — same
    // N+1-avoidance as the promoter/last-active lookups above.
    let mut icon_art = icon_art_by_app(&db, &rows, &org_slugs).await;
    let mut items = rows_to_responses(rows, &org_slugs);
    for item in items.iter_mut() {
        if let Some(ts) = last_active.get(&item.id) {
            item.last_active_at = Some(ts.to_rfc3339());
        }
        item.last_promoted_by_email = promoter_by_app
            .get(&item.id)
            .and_then(|pid| promoter_emails.get(pid).cloned());
        if let Some((icon, art)) = icon_art.remove(&item.id) {
            item.icon_url = icon;
            item.art_url = art;
        }
    }
    // `next_offset` only exists when the page came back full — short
    // pages are the tail of the dataset and signal "stop fetching" to
    // the client without a separate `total` count query.
    let next_offset = if returned >= limit {
        Some(q.offset + returned)
    } else {
        None
    };
    Ok(Json(ListAppsResponse { items, next_offset }))
}

pub async fn list_my_apps(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<Json<Vec<AppResponse>>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("DB connection error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let memberships = OrgMembers::find()
        .filter(org_members::Column::UserId.eq(user.id))
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query memberships: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let org_ids: Vec<Uuid> = memberships.iter().map(|m| m.org_id).collect();
    if org_ids.is_empty() {
        return Ok(Json(vec![]));
    }

    // Customer-facing endpoint: only show published apps. Drafts live
    // in /admin/apps for staff.
    let rows = Apps::find()
        .filter(apps::Column::OrgId.is_in(org_ids))
        .filter(apps::Column::PublishedAt.is_not_null())
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query apps: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let org_slugs = org_slugs_for(&db, &rows).await.map_err(|e| {
        tracing::error!("Failed to load org slugs for apps/mine: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    // Manifest-derived icon/art in ONE batched query (no N+1).
    let mut icon_art = icon_art_by_app(&db, &rows, &org_slugs).await;
    let mut items = rows_to_responses(rows, &org_slugs);
    for item in items.iter_mut() {
        if let Some((icon, art)) = icon_art.remove(&item.id) {
            item.icon_url = icon;
            item.art_url = art;
        }
    }
    Ok(Json(items))
}

pub async fn get_app(Path(id): Path<Uuid>) -> Result<Json<AppResponse>, StatusCode> {
    let db = establish_connection()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let row = Apps::find_by_id(id)
        .one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let org = Organizations::find_by_id(row.org_id)
        .one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let manifests = crate::server::api::customer_apps_manifest::resolve_manifests_batch(
        &db,
        std::slice::from_ref(&row),
    )
    .await;
    let (icon_url, art_url) = crate::server::api::workspace_custom_apps::icon_art_urls(
        manifests.get(&row.id),
        &org.slug,
        &row.slug,
    );
    let mut resp = AppResponse::from_model_with_org(row, &org.slug);
    resp.icon_url = icon_url;
    resp.art_url = art_url;
    Ok(Json(resp))
}

#[derive(Deserialize, Debug)]
pub struct UpdateAppRequest {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub project_id: Option<Uuid>,
    pub branch: Option<String>,
    pub status: Option<String>,
    /// Repoint the bundle source. Most useful for LocalFolder paths
    /// (e.g. fixing a wrong-folder mistake) and for moving an app
    /// between v0 / local / s3 without delete+recreate.
    pub source: Option<SourceSpec>,
}

pub async fn update_app(
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateAppRequest>,
) -> Result<Json<AppResponse>, StatusCode> {
    let db = establish_connection()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let existing = Apps::find_by_id(id)
        .one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let org_id = existing.org_id;

    if let Some(slug) = req.slug.as_deref() {
        let slug = slug.trim();
        if !is_valid_slug(slug) {
            return Err(StatusCode::BAD_REQUEST);
        }
        // update_app still returns plain StatusCode; map the
        // structured ApiErr back to its status. The richer body only
        // matters on create today, where the dialog reads it.
        if slug != existing.slug
            && slug_taken_in_org(&db, org_id, slug)
                .await
                .map_err(|(sc, _)| sc)?
        {
            return Err(StatusCode::CONFLICT);
        }
    }

    let mut active: apps::ActiveModel = existing.into();
    if let Some(n) = req.name {
        validate_display_name(&n).map_err(|_| StatusCode::BAD_REQUEST)?;
        active.name = ActiveValue::Set(n);
    }
    if let Some(s) = req.slug {
        active.slug = ActiveValue::Set(s.trim().to_string());
    }
    if let Some(pid) = req.project_id {
        active.project_id = ActiveValue::Set(pid);
    }
    if let Some(b) = req.branch {
        active.branch = ActiveValue::Set(b);
    }
    if let Some(s) = req.status {
        active.status = ActiveValue::Set(s);
    }
    if let Some(source) = req.source {
        let (source_type, source_config) = source.into_columns();
        active.source_type = ActiveValue::Set(source_type);
        active.source_config = ActiveValue::Set(source_config);
    }
    active.updated_at = ActiveValue::Set(Utc::now().fixed_offset());
    let updated = active.update(&db).await.map_err(|e| {
        if oxy::database::errors::is_unique_violation(&e) {
            return StatusCode::CONFLICT;
        }
        tracing::error!("Failed to update app {id}: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let org = Organizations::find_by_id(org_id)
        .one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let warnings = validate_local_source(
        &updated.source_type,
        &updated.source_config,
        &org.slug,
        &updated.slug,
    );
    let mut resp = AppResponse::from_model_with_org(updated, &org.slug);
    resp.warnings = warnings;
    Ok(Json(resp))
}

fn rows_to_responses(
    rows: Vec<apps::Model>,
    org_slugs: &std::collections::HashMap<Uuid, String>,
) -> Vec<AppResponse> {
    rows.into_iter()
        .map(|row| {
            let org_slug = org_slugs
                .get(&row.org_id)
                .cloned()
                .unwrap_or_else(|| "unknown-org".to_string());
            AppResponse::from_model_with_org(row, &org_slug)
        })
        .collect()
}

/// Publish:
///
/// - **S3 source**: server-side copy `apps/<org>/<slug>/draft/*` →
///   `apps/<org>/<slug>/published/*`, then sync the `published`
///   channel into the local state-dir. This is what isolates the
///   customer view from the engineer's draft.
/// - **Local / V0 source**: there's no bundle channel to promote —
///   just stamp `published_at`. Publishing is purely a sidebar
///   visibility toggle for these.
///
/// In all cases stamp `published_at = now()` so the customer-facing
/// auth gate flips and the workspace sidebar picks up the entry.
pub async fn publish_app(
    oxy_auth::extractor::AuthenticatedUserExtractor(user): oxy_auth::extractor::AuthenticatedUserExtractor,
    Path(id): Path<Uuid>,
) -> Result<Json<AppResponse>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("publish_app DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let updated = publish_one(&db, id, user.id).await.map_err(|e| e.status)?;
    crate::server::api::customer_apps_auth::invalidate_access_cache();
    let org = load_org(&db, updated.org_id).await.map_err(|e| e.status)?;

    // The SAME action taken by a partner was audited and by Oxy staff was not, so
    // the trail recorded the delegated tier and was blind to the privileged one —
    // backwards. Publishing puts an app in front of a customer's users; who did it
    // is exactly the question an incident asks first.
    crate::server::api::audit::record_best_effort(
        &db,
        crate::server::api::audit::AuditEntry::new(user.email.clone(), "app.published")
            .actor(user.id, crate::server::api::audit::ActorType::User)
            .org(updated.org_id)
            .target("app", updated.id.to_string(), updated.name.clone()),
    )
    .await;

    Ok(Json(AppResponse::from_model_with_org(updated, &org.slug)))
}

/// Unpublish: null out `published_at`. Non-app-admins lose access on
/// next request; the bundle itself stays untouched.
pub async fn unpublish_app(
    oxy_auth::extractor::AuthenticatedUserExtractor(user): oxy_auth::extractor::AuthenticatedUserExtractor,
    Path(id): Path<Uuid>,
) -> Result<Json<AppResponse>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("unpublish_app DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let updated = unpublish_one(&db, id).await.map_err(|e| e.status)?;
    crate::server::api::customer_apps_auth::invalidate_access_cache();
    let org = load_org(&db, updated.org_id).await.map_err(|e| e.status)?;

    // Taking a customer's app DOWN is at least as auditable as putting it up.
    crate::server::api::audit::record_best_effort(
        &db,
        crate::server::api::audit::AuditEntry::new(user.email.clone(), "app.unpublished")
            .actor(user.id, crate::server::api::audit::ActorType::User)
            .org(updated.org_id)
            .target("app", updated.id.to_string(), updated.name.clone()),
    )
    .await;

    Ok(Json(AppResponse::from_model_with_org(updated, &org.slug)))
}

/// Response for a manual function-job trigger: the seeded run to watch.
#[derive(Debug, Serialize)]
pub struct RunFunctionJobResponse {
    pub run_id: String,
}

/// `POST /admin/apps/{id}/functions/{name}/runs` — trigger a one-off background
/// run of a customer-app Oxy Function as a job (the manual "run now" that isn't
/// tied to a cron schedule). An optional JSON request body is handed to the
/// function as its `req` input params (same shape a route invocation receives);
/// an empty body runs it with no params. Enqueues a durable task on the global
/// fleet and returns its `run_id`; the caller watches it in the orchestrator
/// dashboard. Thin transport: parse input → enqueue → serialize (the work is in
/// `customer_apps_functions::trigger_function_job`).
pub async fn run_function_job(
    oxy_auth::extractor::AuthenticatedUserExtractor(_user): oxy_auth::extractor::AuthenticatedUserExtractor,
    Path((id, name)): Path<(Uuid, String)>,
    body: axum::body::Bytes,
) -> Result<Json<RunFunctionJobResponse>, StatusCode> {
    // Empty body → no params; a non-empty body must be valid JSON.
    let input = if body.is_empty() {
        None
    } else {
        Some(
            serde_json::from_slice::<serde_json::Value>(&body)
                .map_err(|_| StatusCode::BAD_REQUEST)?,
        )
    };
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("run_function_job DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let run_id =
        crate::server::api::customer_apps_functions::trigger_function_job(&db, id, &name, input)
            .await
            .map_err(|e| {
                tracing::warn!("run_function_job failed for {id}/{name}: {e}");
                StatusCode::BAD_REQUEST
            })?;
    Ok(Json(RunFunctionJobResponse { run_id }))
}

/// One row of an app's build history (newest first), with flags marking
/// which build each channel currently points at.
#[derive(Debug, Serialize)]
pub struct BuildSummary {
    /// `app_builds.id` — pass this to rollback.
    pub id: Uuid,
    /// Engineer-facing version string (git sha / CI run id).
    pub build_id: String,
    pub created_at: String,
    pub is_draft: bool,
    pub is_published: bool,
    /// Email of the app-admin who ran the publish. `None` for builds
    /// created before the `published_by` column existed.
    pub published_by_email: Option<String>,
    /// Git provenance captured by `oxy publish` (all `None` for legacy /
    /// non-git builds). `source_repo` is the raw remote URL; the frontend
    /// normalizes it to a GitHub link against `commit_sha`.
    pub source_repo: Option<String>,
    pub commit_sha: Option<String>,
    pub source_branch: Option<String>,
}

/// `GET /{id}/builds` response: the build history plus who last promoted a
/// build to live (distinct from each build's original publisher).
#[derive(Debug, Serialize)]
pub struct BuildHistoryResponse {
    pub builds: Vec<BuildSummary>,
    pub promoted_by_email: Option<String>,
    pub promoted_at: Option<String>,
}

/// `GET /api/customer-apps/{id}/builds` — newest-first build history for
/// the new publish pipeline. Empty for legacy `s3`/local/v0 rows that
/// have never been published via `oxy publish`.
pub async fn list_builds(Path(id): Path<Uuid>) -> Result<Json<BuildHistoryResponse>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("list_builds DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let app = Apps::find_by_id(id)
        .one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let builds = AppBuilds::find()
        .filter(entity::app_builds::Column::AppId.eq(id))
        .order_by_desc(entity::app_builds::Column::CreatedAt)
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("list_builds query failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    // Resolve emails in a single query for the "who deployed" column: each
    // build's original publisher + whoever last promoted (made live). Missing
    // ids (legacy/NULL) simply render without an email.
    let mut publisher_ids: Vec<Uuid> = builds.iter().filter_map(|b| b.published_by).collect();
    if let Some(promoter) = app.last_promoted_by {
        publisher_ids.push(promoter);
    }
    let emails: std::collections::HashMap<Uuid, String> = if publisher_ids.is_empty() {
        std::collections::HashMap::new()
    } else {
        entity::prelude::Users::find()
            .filter(entity::users::Column::Id.is_in(publisher_ids))
            .all(&db)
            .await
            .map_err(|e| {
                tracing::error!("list_builds publisher lookup failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .into_iter()
            .map(|u| (u.id, u.email))
            .collect()
    };
    let builds_out: Vec<BuildSummary> = builds
        .into_iter()
        .map(|b| BuildSummary {
            is_draft: app.draft_build_id == Some(b.id),
            is_published: app.published_build_id == Some(b.id),
            published_by_email: b.published_by.and_then(|uid| emails.get(&uid).cloned()),
            id: b.id,
            build_id: b.build_id,
            created_at: b.created_at.to_rfc3339(),
            source_repo: b.source_repo,
            commit_sha: b.commit_sha,
            source_branch: b.source_branch,
        })
        .collect();
    Ok(Json(BuildHistoryResponse {
        builds: builds_out,
        promoted_by_email: app
            .last_promoted_by
            .and_then(|uid| emails.get(&uid).cloned()),
        promoted_at: app.last_promoted_at.map(|t| t.to_rfc3339()),
    }))
}

#[derive(Debug, Deserialize)]
pub struct RollbackRequest {
    /// `app_builds.id` (from `GET .../builds`) to make live.
    pub build_id: Uuid,
}

/// `POST /api/customer-apps/{id}/rollback` — repoint the published channel
/// at any retained build. Pure pointer move; the build's bytes are already
/// in S3. Validates the build belongs to this app.
pub async fn rollback_app(
    oxy_auth::extractor::AuthenticatedUserExtractor(user): oxy_auth::extractor::AuthenticatedUserExtractor,
    Path(id): Path<Uuid>,
    Json(req): Json<RollbackRequest>,
) -> Result<Json<AppResponse>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("rollback_app DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let row = Apps::find_by_id(id)
        .one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let build = AppBuilds::find_by_id(req.build_id)
        .one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if build.app_id != id {
        return Err(StatusCode::BAD_REQUEST);
    }
    // Promotion gate (validator-can't-be-bypassed): a historical build may be
    // rolled back to the live channel only if its recorded validation is
    // `passed` — otherwise rollback would be a way to make a failed build live.
    if build.validation_status != "passed" {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    let org = Organizations::find_by_id(row.org_id)
        .one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let now = Utc::now().fixed_offset();
    let mut active: apps::ActiveModel = row.into();
    active.published_build_id = ActiveValue::Set(Some(req.build_id));
    active.published_at = ActiveValue::Set(Some(now));
    active.last_promoted_by = ActiveValue::Set(Some(user.id));
    active.last_promoted_at = ActiveValue::Set(Some(now));
    active.updated_at = ActiveValue::Set(now);
    let updated = active.update(&db).await.map_err(|e| {
        tracing::error!("rollback_app update failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    crate::server::api::customer_apps_auth::invalidate_access_cache();
    crate::server::api::customer_apps_cache::invalidate_cached_canonical_dir_all_channels(id);
    Ok(Json(AppResponse::from_model_with_org(updated, &org.slug)))
}

pub async fn delete_app(Path(id): Path<Uuid>) -> Result<StatusCode, StatusCode> {
    let db = establish_connection()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    delete_one(&db, id).await.map_err(|e| e.status)?;
    Ok(StatusCode::NO_CONTENT)
}

// ===========================================================================
// Shared single-app mutations + batch endpoints
//
// The publish/unpublish/delete handlers above and the batch endpoints below
// share one core mutation each (`*_one`) so a bulk action can never drift from
// its single-app counterpart. Batch endpoints are best-effort: every id is
// attempted independently and its outcome recorded, so one failure never
// aborts the rest.
// ===========================================================================

/// Shared failure type for the single-app and batch mutation paths. `status`
/// drives the one-shot routes' HTTP code; `message` names the failure in a
/// batch result row.
pub(crate) struct AppOpError {
    pub(crate) status: StatusCode,
    message: String,
}

impl AppOpError {
    fn not_found() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: "App not found.".into(),
        }
    }

    fn internal() -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Internal server error.".into(),
        }
    }

    /// A build can't be promoted because its recorded validation status is not
    /// `passed` (the validator-can't-be-bypassed gate).
    fn validation_failed(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: detail.into(),
        }
    }
}

/// Promotion gate (validator-can't-be-bypassed): a build may reach the live
/// (published) channel only if its recorded validation status is `passed`.
/// Shared by EVERY AppOpError promotion path — publish, promote-latest — so no
/// path can quietly re-open the invariant. A missing build is refused (you can't
/// validate what isn't there), matching `customer_apps_publish::gate_promotion`.
/// Dormant today (every stored build is `passed`); load-bearing once gate 2 can
/// record `failed`.
async fn gate_build_promotion(db: &DatabaseConnection, build_pk: Uuid) -> Result<(), AppOpError> {
    let build = AppBuilds::find_by_id(build_pk)
        .one(db)
        .await
        .map_err(|e| {
            tracing::error!("promotion gate: build {build_pk} load failed: {e}");
            AppOpError::internal()
        })?
        .ok_or_else(|| {
            AppOpError::validation_failed(
                "build not found — cannot promote a missing build to live".to_string(),
            )
        })?;
    if build.validation_status != "passed" {
        return Err(AppOpError::validation_failed(format!(
            "build validation status is '{}', not 'passed' — cannot promote to live",
            build.validation_status
        )));
    }
    Ok(())
}

/// Load an app's org — needed to build [`AppResponse`]. A missing org is a
/// broken FK, surfaced as an internal error.
async fn load_org(
    db: &DatabaseConnection,
    org_id: Uuid,
) -> Result<organizations::Model, AppOpError> {
    Organizations::find_by_id(org_id)
        .one(db)
        .await
        .map_err(|_| AppOpError::internal())?
        .ok_or_else(AppOpError::internal)
}

/// Core publish mutation shared by [`publish_app`] and [`batch_publish_apps`].
/// Pure pointer move: stamp `published_at`/promoter, repoint the published
/// channel at the current draft build, and drop the canonical-dir cache so the
/// serve path resolves the freshly-published channel instead of a stale entry.
pub(crate) async fn publish_one(
    db: &DatabaseConnection,
    id: Uuid,
    actor: Uuid,
) -> Result<apps::Model, AppOpError> {
    let row = Apps::find_by_id(id)
        .one(db)
        .await
        .map_err(|e| {
            tracing::error!("publish_one {id} load failed: {e}");
            AppOpError::internal()
        })?
        .ok_or_else(AppOpError::not_found)?;

    let draft_ptr = row.draft_build_id;
    // Promotion gate (validator-can't-be-bypassed): a draft goes live only if its
    // recorded validation status is `passed`. Gate 1 stamps that at publish; a
    // future deploy-time probe (gate 2) may downgrade it. Held at draft otherwise.
    if let Some(ptr) = draft_ptr {
        gate_build_promotion(db, ptr).await?;
    }
    let now = Utc::now().fixed_offset();
    let mut active: apps::ActiveModel = row.into();
    active.published_at = ActiveValue::Set(Some(now));
    active.last_promoted_by = ActiveValue::Set(Some(actor));
    active.last_promoted_at = ActiveValue::Set(Some(now));
    if let Some(ptr) = draft_ptr {
        active.published_build_id = ActiveValue::Set(Some(ptr));
    }
    active.updated_at = ActiveValue::Set(now);
    let updated = active.update(db).await.map_err(|e| {
        tracing::error!("publish_one {id} update failed: {e}");
        AppOpError::internal()
    })?;

    // Per-app cache only — the global access cache is invalidated ONCE by the
    // caller (a batch would otherwise do N full global invalidations).
    crate::server::api::customer_apps_cache::invalidate_cached_canonical_dir_all_channels(id);
    Ok(updated)
}

/// Core unpublish mutation shared by [`unpublish_app`] and
/// [`batch_unpublish_apps`]. Nulls `published_at` + the published channel
/// pointer; the bundle bytes stay untouched.
pub(crate) async fn unpublish_one(
    db: &DatabaseConnection,
    id: Uuid,
) -> Result<apps::Model, AppOpError> {
    let row = Apps::find_by_id(id)
        .one(db)
        .await
        .map_err(|e| {
            tracing::error!("unpublish_one {id} load failed: {e}");
            AppOpError::internal()
        })?
        .ok_or_else(AppOpError::not_found)?;

    let now = Utc::now().fixed_offset();
    let mut active: apps::ActiveModel = row.into();
    active.published_at = ActiveValue::Set(None);
    active.published_build_id = ActiveValue::Set(None);
    active.updated_at = ActiveValue::Set(now);
    let updated = active.update(db).await.map_err(|e| {
        tracing::error!("unpublish_one {id} update failed: {e}");
        AppOpError::internal()
    })?;

    // Per-app cache only — the global access cache is invalidated ONCE by the
    // caller (a batch would otherwise do N full global invalidations).
    crate::server::api::customer_apps_cache::invalidate_cached_canonical_dir_all_channels(id);
    Ok(updated)
}

/// Core delete shared by [`delete_app`] and [`batch_delete_apps`]. Removes the
/// bundle bytes before the DB row so a partial failure leaves a recoverable
/// orphan row rather than an orphan S3 prefix; build-store failure is logged,
/// never fatal.
async fn delete_one(db: &DatabaseConnection, id: Uuid) -> Result<(), AppOpError> {
    let row = Apps::find_by_id(id)
        .one(db)
        .await
        .map_err(|_| AppOpError::internal())?
        .ok_or_else(AppOpError::not_found)?;

    if let Err(e) = crate::server::api::customer_apps_build_store::delete_app(id).await {
        tracing::warn!(
            "delete_one {id}: bundle bytes could not be removed from build store: {e} \
             — proceeding with DB row delete; reclaim manually if needed"
        );
    }

    row.delete(db).await.map_err(|_| AppOpError::internal())?;
    Ok(())
}

/// Core "promote to latest" shared by [`batch_promote_latest_apps`]: point the
/// published channel at the app's newest build and stamp `published_at`. This
/// is the bulk "roll everyone forward to their latest version" primitive —
/// distinct from `publish_one` (which promotes the *draft* pointer): it always
/// targets the most recently created build regardless of channel. An app with
/// no builds is a per-item failure, not a fatal one.
async fn promote_latest_one(
    db: &DatabaseConnection,
    id: Uuid,
    actor: Uuid,
) -> Result<apps::Model, AppOpError> {
    let row = Apps::find_by_id(id)
        .one(db)
        .await
        .map_err(|e| {
            tracing::error!("promote_latest_one {id} load failed: {e}");
            AppOpError::internal()
        })?
        .ok_or_else(AppOpError::not_found)?;

    let latest = AppBuilds::find()
        .filter(entity::app_builds::Column::AppId.eq(id))
        .order_by_desc(entity::app_builds::Column::CreatedAt)
        .one(db)
        .await
        .map_err(|e| {
            tracing::error!("promote_latest_one {id} build lookup failed: {e}");
            AppOpError::internal()
        })?
        .ok_or_else(|| AppOpError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: "No builds to promote.".into(),
        })?;

    // Promotion gate (validator-can't-be-bypassed): the newest build goes live
    // only if its recorded validation is `passed`. This is exactly where a
    // freshly-uploaded-then-failed build would sit once gate 2 can downgrade a
    // stored build, so promote-latest must not skip the check.
    gate_build_promotion(db, latest.id).await?;

    let now = Utc::now().fixed_offset();
    let mut active: apps::ActiveModel = row.into();
    active.published_build_id = ActiveValue::Set(Some(latest.id));
    active.published_at = ActiveValue::Set(Some(now));
    active.last_promoted_by = ActiveValue::Set(Some(actor));
    active.last_promoted_at = ActiveValue::Set(Some(now));
    active.updated_at = ActiveValue::Set(now);
    let updated = active.update(db).await.map_err(|e| {
        tracing::error!("promote_latest_one {id} update failed: {e}");
        AppOpError::internal()
    })?;

    // Per-app cache only — the global access cache is invalidated ONCE by the
    // caller (a batch would otherwise do N full global invalidations).
    crate::server::api::customer_apps_cache::invalidate_cached_canonical_dir_all_channels(id);
    Ok(updated)
}

/// Upper bound on ids accepted by a batch endpoint. The admin surface is
/// small-scale; this only rejects a pathological request, it is not a paging
/// limit.
const MAX_BATCH_IDS: usize = 500;

/// Request body for every batch endpoint: the app ids to act on.
#[derive(Debug, Deserialize)]
pub struct BatchIdsRequest {
    pub ids: Vec<Uuid>,
}

/// One app's outcome in a batch response. `ok = false` carries a short reason
/// (e.g. "App not found.") so the UI can name which apps failed.
#[derive(Debug, Serialize)]
pub struct BatchItemResult {
    pub id: Uuid,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl BatchItemResult {
    fn ok(id: Uuid) -> Self {
        Self {
            id,
            ok: true,
            error: None,
        }
    }

    fn failed(id: Uuid, message: String) -> Self {
        Self {
            id,
            ok: false,
            error: Some(message),
        }
    }
}

/// Aggregate result of a batch mutation. The request is 200 whenever it is
/// well-formed — individual failures live in `results`, not the status code —
/// so the UI can report "published 4, 1 failed" from a single response.
#[derive(Debug, Serialize)]
pub struct BatchResponse {
    pub succeeded: usize,
    pub failed: usize,
    pub results: Vec<BatchItemResult>,
}

impl BatchResponse {
    fn from_results(results: Vec<BatchItemResult>) -> Self {
        let succeeded = results.iter().filter(|r| r.ok).count();
        Self {
            failed: results.len() - succeeded,
            succeeded,
            results,
        }
    }
}

/// Reject empty or oversized batches before touching the DB.
fn validate_batch(ids: &[Uuid]) -> Result<(), ApiErr> {
    if ids.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "No apps selected."));
    }
    if ids.len() > MAX_BATCH_IDS {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            format!("Too many apps in one request (max {MAX_BATCH_IDS})."),
        ));
    }
    Ok(())
}

/// `POST /api/customer-apps/batch/publish` — publish many apps at once.
pub async fn batch_publish_apps(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Json(req): Json<BatchIdsRequest>,
) -> Result<Json<BatchResponse>, ApiErr> {
    validate_batch(&req.ids)?;
    let db = establish_connection().await.map_err(internal)?;
    let mut results = Vec::with_capacity(req.ids.len());
    for id in req.ids {
        results.push(match publish_one(&db, id, user.id).await {
            Ok(_) => BatchItemResult::ok(id),
            Err(e) => BatchItemResult::failed(id, e.message),
        });
    }
    // One global access-cache invalidation for the whole batch (per-app
    // canonical-dir caches are dropped inside publish_one). Skip when nothing
    // changed.
    if results.iter().any(|r| r.ok) {
        crate::server::api::customer_apps_auth::invalidate_access_cache();
    }
    Ok(Json(BatchResponse::from_results(results)))
}

/// `POST /api/customer-apps/batch/promote-latest` — roll many apps forward to
/// their newest build in one call (the bulk "mass promote latest versions"
/// action). Best-effort per id; an app with no builds is reported as a
/// per-item failure.
pub async fn batch_promote_latest_apps(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Json(req): Json<BatchIdsRequest>,
) -> Result<Json<BatchResponse>, ApiErr> {
    validate_batch(&req.ids)?;
    let db = establish_connection().await.map_err(internal)?;
    let mut results = Vec::with_capacity(req.ids.len());
    for id in req.ids {
        results.push(match promote_latest_one(&db, id, user.id).await {
            Ok(_) => BatchItemResult::ok(id),
            Err(e) => BatchItemResult::failed(id, e.message),
        });
    }
    if results.iter().any(|r| r.ok) {
        crate::server::api::customer_apps_auth::invalidate_access_cache();
    }
    Ok(Json(BatchResponse::from_results(results)))
}

/// `POST /api/customer-apps/batch/unpublish` — unpublish many apps at once.
pub async fn batch_unpublish_apps(
    Json(req): Json<BatchIdsRequest>,
) -> Result<Json<BatchResponse>, ApiErr> {
    validate_batch(&req.ids)?;
    let db = establish_connection().await.map_err(internal)?;
    let mut results = Vec::with_capacity(req.ids.len());
    for id in req.ids {
        results.push(match unpublish_one(&db, id).await {
            Ok(_) => BatchItemResult::ok(id),
            Err(e) => BatchItemResult::failed(id, e.message),
        });
    }
    if results.iter().any(|r| r.ok) {
        crate::server::api::customer_apps_auth::invalidate_access_cache();
    }
    Ok(Json(BatchResponse::from_results(results)))
}

/// `POST /api/customer-apps/batch/delete` — delete many app registrations at
/// once. POST (not DELETE) because the id set travels in the request body.
pub async fn batch_delete_apps(
    Json(req): Json<BatchIdsRequest>,
) -> Result<Json<BatchResponse>, ApiErr> {
    validate_batch(&req.ids)?;
    let db = establish_connection().await.map_err(internal)?;
    let mut results = Vec::with_capacity(req.ids.len());
    for id in req.ids {
        results.push(match delete_one(&db, id).await {
            Ok(()) => BatchItemResult::ok(id),
            Err(e) => BatchItemResult::failed(id, e.message),
        });
    }
    Ok(Json(BatchResponse::from_results(results)))
}

/// Resolve `<org_slug>/<app_slug>` → (org row, app row). Returns `Ok(None)`
/// if either lookup misses; `Err` only on a real DB failure.
/// Resolve `<org>/<app>` → (org row, app row). The `org` segment can be
/// either a slug or a UUID — auto-detected on parse, mirroring the
/// publish-side `OrgRef::from_str_auto`. Lets `oxy publish --org <uuid>`
/// reach `build-config` without falling over on the slug-only lookup
/// the route used to do; the customer-facing `/customer-apps/<org>/<app>/`
/// URLs are unaffected because they only ever carry slugs (UUIDs in
/// browser URLs would be ugly and the serve dispatcher passes slugs
/// straight through).
pub(crate) async fn lookup_by_pretty_path(
    db: &DatabaseConnection,
    org_segment: &str,
    app_slug: &str,
) -> Result<Option<(organizations::Model, apps::Model)>, StatusCode> {
    let org = match Uuid::parse_str(org_segment) {
        Ok(id) => Organizations::find_by_id(id).one(db).await,
        Err(_) => {
            Organizations::find()
                .filter(organizations::Column::Slug.eq(org_segment))
                .one(db)
                .await
        }
    }
    .map_err(|e| {
        tracing::error!("Org lookup failed for {org_segment}: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(org) = org else {
        return Ok(None);
    };
    let app = Apps::find()
        .filter(apps::Column::OrgId.eq(org.id))
        .filter(apps::Column::Slug.eq(app_slug))
        .one(db)
        .await
        .map_err(|e| {
            tracing::error!("App lookup failed for {org_segment}/{app_slug}: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(app.map(|a| (org, a)))
}

/// Validate that a caller-supplied slug fits the URL-safe shape we
/// auto-derive in [`slugify`]: lowercase ASCII letters/digits/dashes,
/// 1–63 chars, no leading/trailing dash, no consecutive dashes.
pub(crate) fn is_valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 63
        && !s.starts_with('-')
        && !s.ends_with('-')
        && !s.contains("--")
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

async fn slug_taken_in_org(
    db: &DatabaseConnection,
    org_id: Uuid,
    slug: &str,
) -> Result<bool, ApiErr> {
    Apps::find()
        .filter(apps::Column::OrgId.eq(org_id))
        .filter(apps::Column::Slug.eq(slug))
        .one(db)
        .await
        .map(|opt| opt.is_some())
        .map_err(|e| {
            tracing::error!("Slug uniqueness check failed for {org_id}/{slug}: {e}");
            internal(e)
        })
}

/// Derive a slug from a human name and dedupe by appending `-2`, `-3`, …
/// until unique within the org. Tries up to 50 suffixes before giving up;
/// the realistic ceiling for any one org is far below that.
async fn unique_slug_for_name(
    db: &DatabaseConnection,
    org_id: Uuid,
    name: &str,
) -> Result<String, ApiErr> {
    let base = slugify(name);
    // Fetch all candidate-shape slugs in this org with one query, then
    // dedupe in-process. The previous implementation walked up to 50
    // sequential `SELECT 1 FROM apps WHERE slug = ?` round trips for
    // densely-populated orgs; this collapses it to one. We use LIKE
    // 'base%' on the indexed `slug` column so even a large `apps`
    // table only scans the per-base subset.
    //
    // `like_escape` defends against a base whose slug happens to
    // contain `%` or `_`. Slugs are normally restricted to lowercase
    // ASCII letters + digits + `-` (see `is_valid_slug`), so the
    // escape rarely fires — but `slugify` doesn't guarantee shape,
    // and a slug like `a_b` would otherwise match `a*b` and prune
    // valid free numbers from the candidate set.
    let pattern = format!("{}%", like_escape(&base));
    let taken: Vec<String> = Apps::find()
        .filter(apps::Column::OrgId.eq(org_id))
        .filter(apps::Column::Slug.like(&pattern))
        .select_only()
        .column(apps::Column::Slug)
        .into_tuple()
        .all(db)
        .await
        .map_err(|e| {
            tracing::error!("Slug-candidate scan failed for {org_id} base={base:?}: {e}");
            internal(e)
        })?;
    let taken: std::collections::HashSet<&str> = taken.iter().map(|s| s.as_str()).collect();
    if !taken.contains(base.as_str()) {
        return Ok(base);
    }
    for n in 2..=50 {
        let candidate = format!("{base}-{n}");
        if !taken.contains(candidate.as_str()) {
            return Ok(candidate);
        }
    }
    tracing::error!("Could not find unique slug for {name:?} in org {org_id}");
    Err(api_err(
        StatusCode::CONFLICT,
        format!(
            "Couldn't find a free slug for {name:?} in this org \
             (tried '{base}', '{base}-2' through '{base}-50'). \
             Provide a slug explicitly or delete some unused apps."
        ),
    ))
}

/// Escape SQL `LIKE`-pattern metacharacters in a literal value. Slugs
/// almost never need it, but `slugify` doesn't guarantee shape and
/// we'd rather over-match nothing than under-match a candidate.
fn like_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '%' | '_' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_template_id_accepts_vite() {
        assert_eq!(validate_template_id(Some("vite")).unwrap(), "vite");
    }

    #[test]
    fn validate_template_id_defaults_to_vite_when_none() {
        assert_eq!(validate_template_id(None).unwrap(), "vite");
    }

    #[test]
    fn validate_template_id_rejects_unknown() {
        assert!(validate_template_id(Some("does-not-exist")).is_err());
    }

    #[test]
    fn validate_display_name_accepts_typical_names() {
        assert!(validate_display_name("Acme Analytics").is_ok());
        assert!(validate_display_name("Store Pulse — v2 (beta)").is_ok());
        assert!(validate_display_name("数据看板").is_ok()); // unicode is fine
    }

    #[test]
    fn validate_display_name_rejects_empty_or_whitespace_only() {
        assert!(validate_display_name("").is_err());
        assert!(validate_display_name("   ").is_err());
    }

    #[test]
    fn validate_display_name_rejects_json_breaking_chars() {
        // Each of these would corrupt the rendered oxy-app.json /
        // package.json scaffolds when blindly substituted.
        assert!(validate_display_name("My \"App\"").is_err()); // double quote
        assert!(validate_display_name("path\\to").is_err()); // backslash
        assert!(validate_display_name("line\nbreak").is_err()); // newline
        assert!(validate_display_name("tab\there").is_err()); // tab
    }

    #[test]
    fn validate_display_name_rejects_jsx_breaking_chars() {
        // The dashboard template's `<h1>{{APP_DISPLAY_NAME}}</h1>`
        // would break (or worse, render unintended markup) if these
        // landed there post-substitution.
        assert!(validate_display_name("<script>").is_err());
        assert!(validate_display_name("name>x").is_err());
        assert!(validate_display_name("name{x").is_err());
        assert!(validate_display_name("name}x").is_err());
    }

    #[test]
    fn validate_display_name_rejects_overlong() {
        let s = "a".repeat(129);
        assert!(validate_display_name(&s).is_err());
        let s = "a".repeat(128);
        assert!(validate_display_name(&s).is_ok());
    }

    #[test]
    fn validate_display_name_counts_chars_not_bytes() {
        // CJK names: 128 chars at 3 bytes each = 384 bytes. The old
        // byte-count check rejected at 43 chars (129 bytes).
        let cjk_128 = "数".repeat(128);
        assert!(validate_display_name(&cjk_128).is_ok());
        let cjk_129 = "数".repeat(129);
        assert!(validate_display_name(&cjk_129).is_err());
    }

    #[test]
    fn is_valid_slug_accepts_lower_kebab() {
        assert!(is_valid_slug("acme"));
        assert!(is_valid_slug("acme-analytics"));
        assert!(is_valid_slug("a"));
        assert!(is_valid_slug("acme-2"));
        assert!(is_valid_slug(&"a".repeat(63)));
    }

    #[test]
    fn is_valid_slug_rejects_bad_shapes() {
        assert!(!is_valid_slug(""));
        assert!(!is_valid_slug("Acme")); // uppercase
        assert!(!is_valid_slug("acme--analytics")); // consecutive dashes
        assert!(!is_valid_slug("-acme")); // leading dash
        assert!(!is_valid_slug("acme-")); // trailing dash
        assert!(!is_valid_slug("acme.analytics")); // dot
        assert!(!is_valid_slug("acme/x")); // slash
        assert!(!is_valid_slug(&"a".repeat(64))); // too long
    }

    #[test]
    fn build_pretty_url_is_relative() {
        assert_eq!(
            build_pretty_url("acme", "analytics"),
            "/customer-apps/acme/analytics/"
        );
    }

    #[test]
    fn batch_response_counts_ok_and_failed() {
        let resp = BatchResponse::from_results(vec![
            BatchItemResult::ok(Uuid::nil()),
            BatchItemResult::failed(Uuid::nil(), "App not found.".into()),
            BatchItemResult::ok(Uuid::nil()),
        ]);
        assert_eq!(resp.succeeded, 2);
        assert_eq!(resp.failed, 1);
        assert_eq!(resp.results.len(), 3);
    }

    #[test]
    fn validate_batch_rejects_empty_and_oversized() {
        assert!(validate_batch(&[]).is_err());
        let too_many = vec![Uuid::nil(); MAX_BATCH_IDS + 1];
        assert!(validate_batch(&too_many).is_err());
        assert!(validate_batch(&[Uuid::nil()]).is_ok());
    }
}
