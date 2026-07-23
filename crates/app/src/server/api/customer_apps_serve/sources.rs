//! Bundle-source resolution and file serving for customer apps.
//!
//! The per-app source decision (v0 / local / s3) is recorded at register
//! time; this module turns a resolved source into a served response —
//! reading objects from S3 through the in-memory bundle cache, or from a
//! local bundle dir with path-traversal + symlink-escape defenses.

use std::path::{Component, Path as StdPath, PathBuf};

use axum::body::Body;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use sea_orm::EntityTrait;
use tokio::fs;
use uuid::Uuid;

use crate::server::api::customer_apps_bundle_cache;
use crate::server::api::customer_apps_cache::{
    CACHE_CHANNEL_LOCAL, cached_canonical_dir, invalidate_cached_canonical_dir,
    set_cached_canonical_dir,
};

use super::headers::*;
use super::rewrite::*;

/// Browser-side runtime identity for a served customer app. Injected into
/// every HTML response under `<script>window.__OXY_APP__ = {…}</script>`
/// so the bundle's SDK can call `/api/<projectId>/…` without having those
/// ids baked in at build time. See sdk/typescript/src/config.ts.
#[derive(serde::Serialize, Debug, Clone)]
pub(crate) struct AppRuntimeConfig {
    #[serde(rename = "appId")]
    pub app_id: Uuid,
    pub slug: String,
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
    #[serde(rename = "orgSlug")]
    pub org_slug: String,
    #[serde(rename = "projectId")]
    pub project_id: Uuid,
    pub branch: String,
    /// Empty string = same-origin. v1 always emits empty; reserved for a
    /// future where customer apps live on a different host than the API.
    #[serde(rename = "apiBaseUrl")]
    pub api_base_url: String,
}

impl AppRuntimeConfig {
    pub(super) fn from_app(app: &entity::apps::Model, org_slug: &str) -> Self {
        Self {
            app_id: app.id,
            slug: app.slug.clone(),
            org_id: app.org_id,
            org_slug: org_slug.to_string(),
            project_id: app.project_id,
            branch: app.branch.clone(),
            api_base_url: String::new(),
        }
    }
}

/// Channel resolution for S3-source serve:
/// - staff with the preview-draft cookie set → draft
/// - app has been published → published (default for both staff and customer)
/// - otherwise → draft (only staff reaches here; the auth gate blocked
///   customers on unpublished apps)
pub(super) fn resolve_channel(
    staff_wants_draft: bool,
    is_published: bool,
) -> crate::server::api::customer_apps_sync::Channel {
    use crate::server::api::customer_apps_sync::Channel;
    if staff_wants_draft {
        return Channel::Draft;
    }
    if is_published {
        Channel::Published
    } else {
        Channel::Draft
    }
}

/// Serve a customer-app file directly from S3 (the new publish pipeline).
/// Resolves the build's files from its `app_builds` row and reads objects
/// through the in-memory bundle cache — no local state dir, so any node can
/// serve any build. HTML gets the same base-path rewrite + `window.__OXY_APP__`
/// injection as the legacy disk path; the cache key includes the build id, so
/// a promote/rollback serves fresh bytes with no explicit invalidation.
pub(crate) async fn serve_from_s3_build(
    db: &sea_orm::DatabaseConnection,
    app_id: Uuid,
    build_pk: Uuid,
    rest: &str,
    runtime: &AppRuntimeConfig,
    headers: &HeaderMap,
) -> Response {
    let build = match entity::app_builds::Entity::find_by_id(build_pk)
        .one(db)
        .await
    {
        Ok(Some(b)) => b,
        Ok(None) => {
            tracing::error!("app {app_id}: build pointer {build_pk} has no app_builds row");
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(e) => {
            tracing::error!("app {app_id}: failed to load build {build_pk}: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Empty or directory-style paths serve the SPA entry.
    let requested = {
        let t = rest.trim_start_matches('/');
        if t.is_empty() || t.ends_with('/') {
            format!("{t}index.html")
        } else {
            t.to_string()
        }
    };

    // Try the requested object; on a miss, fall back to the SPA shell
    // (`index.html`) — same behavior as the disk path's spa fallback.
    let (rel_used, bytes) =
        match customer_apps_bundle_cache::get_or_fetch(app_id, &build.build_id, &requested).await {
            Ok(Some(b)) => (requested.clone(), b),
            Ok(None) => match customer_apps_bundle_cache::get_or_fetch(
                app_id,
                &build.build_id,
                "index.html",
            )
            .await
            {
                Ok(Some(b)) => ("index.html".to_string(), b),
                Ok(None) => return StatusCode::NOT_FOUND.into_response(),
                Err(e) => {
                    tracing::error!("app {app_id}: S3 read index.html failed: {e}");
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            },
            Err(e) => {
                tracing::error!("app {app_id}: S3 read {requested} failed: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

    let resolved_path = StdPath::new(&rel_used);
    let mime = guess_content_type(resolved_path);
    let cache = cache_control_for(&requested, resolved_path);
    let body_bytes = if mime.starts_with("text/html") {
        let expected_prefix = format!("/customer-apps/{}/{}/", runtime.org_slug, runtime.slug);
        let rewritten =
            rewrite_bundle_base_path(bytes.as_slice(), &expected_prefix, runtime.app_id);
        inject_app_config(&rewritten, runtime, resolved_path)
    } else {
        bytes.to_vec()
    };
    if mime.starts_with("text/html") {
        let etag = etag_for(&body_bytes);
        if if_none_match(headers, &etag) {
            return (
                StatusCode::NOT_MODIFIED,
                [
                    (header::ETAG, etag),
                    (header::CACHE_CONTROL, cache.to_string()),
                ],
            )
                .into_response();
        }
        return (
            [
                (header::CONTENT_TYPE, mime.to_string()),
                (header::CACHE_CONTROL, cache.to_string()),
                (header::ETAG, etag),
            ],
            Body::from(body_bytes),
        )
            .into_response();
    }
    (
        [(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, cache)],
        Body::from(body_bytes),
    )
        .into_response()
}

pub(super) async fn serve_from_local(
    id: Uuid,
    configured_path: &StdPath,
    rest: &str,
    headers: &HeaderMap,
    runtime: &AppRuntimeConfig,
) -> Response {
    // The configured path IS the bundle dir — the dev points us
    // directly at the directory that holds `index.html` + assets.
    // We used to append `out/` on the assumption it was always a
    // Next.js static export; that broke Vite (`dist/`), Astro
    // (`dist/`), Rsbuild (`dist/`), and anything with a custom out
    // dir. Today the convention is explicit on the dev's side: point
    // at the right folder, however your bundler names it.
    serve_from_dir(
        id,
        CACHE_CHANNEL_LOCAL,
        configured_path,
        rest,
        headers,
        runtime,
    )
    .await
}

/// Shared serving path for both local-folder and s3 sources. Resolves the
/// per-uuid canonical bundle dir (cached), then defers to `serve_file` for
/// the path-traversal-safe + symlink-defended file read.
///
/// `channel_key` keys the cache so draft and published bundle dirs don't
/// cross-contaminate (a staff preview-draft request must never be cached
/// and served back to a customer asking for published, and vice versa).
/// Pass `CACHE_CHANNEL_LOCAL` for local-folder sources; otherwise pass
/// the active S3 channel's `.as_str()` value.
async fn serve_from_dir(
    id: Uuid,
    channel_key: &'static str,
    bundle_dir: &StdPath,
    rest: &str,
    headers: &HeaderMap,
    runtime: &AppRuntimeConfig,
) -> Response {
    let canonical_dir = if let Some(p) = cached_canonical_dir(id, channel_key) {
        p
    } else {
        match bundle_dir.canonicalize() {
            Ok(p) if p.is_dir() => {
                set_cached_canonical_dir(id, channel_key, p.clone());
                p
            }
            _ => {
                // Echo the resolved path back so the operator can see at a
                // glance whether the wrong directory is configured — common
                // cause for LocalFolder apps after the implicit `out/`
                // suffix was dropped, and for S3 apps that haven't been
                // synced yet.
                tracing::warn!(
                    "Bundle dir missing or unresolvable for customer app {id} (expected {bundle_dir:?})"
                );
                return (
                    StatusCode::NOT_FOUND,
                    format!(
                        "Bundle not deployed for customer app {id}.\n\
                         Server looked for files at: {}\n\n\
                         Common causes:\n\
                         - LocalFolder source: source_config.path doesn't point at the directory containing index.html.\n\
                         - S3 source: bundle hasn't been synced yet, or publish ran against an empty draft prefix.",
                        bundle_dir.display()
                    ),
                )
                    .into_response();
            }
        }
    };
    let allow_spa_fallback = wants_html(headers);
    let response = serve_file(&canonical_dir, rest, allow_spa_fallback, runtime).await;
    // If the symlink-escape guard fired, the canonical dir we cached points
    // at something suspicious — invalidate now so a follow-up request after
    // the operator fixes the symlink doesn't have to wait CACHE_TTL.
    if response.status() == StatusCode::FORBIDDEN {
        invalidate_cached_canonical_dir(id, channel_key);
    }
    response
}

/// Serve a file from `bundle_dir`. `bundle_dir` MUST be a canonicalized,
/// absolute path — the symlink-escape check at the bottom compares against
/// it as a fixed prefix.
async fn serve_file(
    bundle_dir: &StdPath,
    rest: &str,
    allow_spa_fallback: bool,
    runtime: &AppRuntimeConfig,
) -> Response {
    let Some(candidate) = resolve_safe(bundle_dir, rest) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    // Candidate order: literal path → `.html` suffix (Next.js
    // trailingSlash=false) → index.html in the requested directory. The
    // bundle-root index.html SPA fallback is appended only when the request
    // accepts HTML — that way an asset XHR for a missing file still gets a
    // proper 404 instead of an opaque 200 with the SPA shell, which makes
    // operator debugging much harder.
    let mut candidates = vec![
        candidate.clone(),
        candidate.with_extension("html"),
        candidate.join("index.html"),
    ];
    if allow_spa_fallback {
        candidates.push(bundle_dir.join("index.html"));
    }
    let resolved = first_existing(&candidates).await;

    let Some(path) = resolved else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // Symlink-escape defense: bundles come from CI output, not a hand-curated
    // directory, so a symlink inside `<bundle_dir>` that points outside it
    // (`out/secrets -> /etc/passwd`) would otherwise be served. Canonicalize
    // and verify the real file lives inside the canonical bundle root.
    let canon = match fs::canonicalize(&path).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to canonicalize {path:?}: {e}");
            return StatusCode::NOT_FOUND.into_response();
        }
    };
    if !canon.starts_with(bundle_dir) {
        tracing::warn!(
            "Refusing to serve {canon:?}: escapes bundle root {bundle_dir:?} via symlink"
        );
        return StatusCode::FORBIDDEN.into_response();
    }

    match fs::read(&canon).await {
        Ok(bytes) => {
            let mime = guess_content_type(&canon);
            let cache = cache_control_for(rest, &canon);
            // HTML responses get two passes:
            //   1. base-path rewrite — fixes the build-time-vs-serve-time
            //      mismatch that bites when an engineer renames a slug
            //      without rebuilding (the bundle's `<script src=...>`
            //      hardcodes the absolute base it was built with).
            //   2. runtime identity injection — `window.__OXY_APP__` into
            //      `<head>` so the SDK can read identity at runtime.
            //
            // Non-HTML responses pass through unchanged. The rewrite is
            // safe to skip for them because Vite/Next-style bundles only
            // emit absolute base-path strings in the HTML entry; JS/CSS
            // chunks reference each other relatively at runtime.
            let body_bytes = if mime.starts_with("text/html") {
                let expected_prefix =
                    format!("/customer-apps/{}/{}/", runtime.org_slug, runtime.slug);
                let rewritten = rewrite_bundle_base_path(&bytes, &expected_prefix, runtime.app_id);
                inject_app_config(&rewritten, runtime, &canon)
            } else {
                bytes
            };
            (
                [(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, cache)],
                Body::from(body_bytes),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to read bundle file {canon:?}: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Join `bundle_dir` with `rest`, rejecting paths that try to escape via
/// `..` or absolute components. Without this guard a request for
/// `../../etc/passwd` would happily traverse out of the bundle root.
fn resolve_safe(bundle_dir: &StdPath, rest: &str) -> Option<PathBuf> {
    let rest = rest.trim_start_matches('/');
    let rel = StdPath::new(rest);
    for c in rel.components() {
        match c {
            Component::Normal(_) | Component::CurDir => {}
            _ => return None,
        }
    }
    Some(bundle_dir.join(rel))
}

async fn first_existing(paths: &[PathBuf]) -> Option<PathBuf> {
    for p in paths {
        if let Ok(meta) = fs::metadata(p).await
            && meta.is_file()
        {
            return Some(p.clone());
        }
    }
    None
}
