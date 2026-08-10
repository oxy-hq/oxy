//! Bundle-source resolution and file serving for custom apps.
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

use crate::server::api::custom_apps_bundle_cache;
use crate::server::api::custom_apps_cache::{
    CACHE_CHANNEL_LOCAL, cached_canonical_dir, invalidate_cached_canonical_dir,
    set_cached_canonical_dir,
};

use super::headers::*;
use super::rewrite::*;

/// Browser-side runtime identity for a served custom app. Injected into
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
    /// future where custom apps live on a different host than the API.
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

/// Bundle subtrees the serve plane must never hand back as files.
///
/// `functions/` holds the app's compiled Oxy Functions. They ship *inside* the
/// same bundle as the frontend (`oxy publish` writes them to
/// `<bundle>/functions/<name>.js`), but they are server-side handlers: the
/// isolate loads them straight out of the build store via
/// `custom_apps_functions`, never over HTTP. Nothing legitimate fetches them
/// through this route.
///
/// Without this list the static-asset route served them to anyone who cleared
/// the app's auth gate — handler logic, embedded query SQL, and (before the
/// `--sources-content=false` change in `publish.rs`) the author's original
/// TypeScript, carried in the inline sourcemap.
const SERVER_ONLY_PREFIXES: &[&str] = &["functions"];

/// Is this bundle-relative request path inside a server-only subtree?
///
/// Compares the first real path segment, skipping empty and `.` components so
/// `//functions/x` and `./functions/x` can't slip past, and case-insensitively
/// so a case-insensitive filesystem (a macOS dev box) doesn't answer
/// `Functions/x` when Linux wouldn't. Traversal (`..`) is already rejected
/// upstream by `resolve_safe` / `is_safe_rel`.
///
/// **A child segment is required.** The artifact is always
/// `functions/<name>.js` (see `custom_apps_functions`), never bare
/// `functions`, so matching the lone segment protected nothing and cost real
/// pages: an app routing `/functions` client-side, or a static export whose
/// own `functions.html` / `functions/index.html` the `.html`-suffix candidate
/// exists to serve, would silently get the site root at `200`. That hit apps
/// with no Oxy Functions at all, and `LocalFolder` sources that cannot
/// contain an artifact by construction.
///
/// Prefix-scoped on purpose: `assets/functions.js` and `my-functions/x` are
/// ordinary frontend files and stay servable.
pub(super) fn is_server_only_path(rest: &str) -> bool {
    let mut segments = rest.split('/').filter(|seg| !seg.is_empty() && *seg != ".");
    let Some(first) = segments.next() else {
        return false;
    };
    let reserved = SERVER_ONLY_PREFIXES
        .iter()
        .any(|p| first.eq_ignore_ascii_case(p));
    reserved && segments.next().is_some()
}

/// What the serve plane should do with a request, once the server-only check
/// has had its say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ServeGuard {
    /// Ordinary request — resolve it normally.
    Allow,
    /// Blocked, but the caller accepts HTML: hand back the SPA shell so a
    /// client-side route of the same name still renders.
    SpaShell,
    /// Blocked with no HTML fallback available — 404, identical to what a
    /// non-existent path returns, so the route doesn't confirm which handlers
    /// an app has.
    NotFound,
}

/// The single decision both serve branches take. Shared deliberately: the S3
/// and local-dir wirings resolve files differently enough that a test of one
/// would not catch a regression in the other, so the part worth getting right
/// lives in one tested function instead of twice in prose.
pub(super) fn serve_guard(rest: &str, accepts_html: bool) -> ServeGuard {
    if !is_server_only_path(rest) {
        return ServeGuard::Allow;
    }
    if accepts_html {
        ServeGuard::SpaShell
    } else {
        ServeGuard::NotFound
    }
}

/// The object key the S3 branch should fetch for a raw request path, or
/// `None` to 404.
///
/// Extracted so the branch that serves production is testable at all — the
/// rest of `serve_from_s3_build` needs a database and a build store, which is
/// why this decision previously had no coverage and the local-dir test could
/// not stand in for it.
///
/// **Order matters: guard the RAW path, then rewrite.** Guarding the rewritten
/// value turned `/functions/` into `functions/index.html`, which *does* have a
/// child segment and so was blocked — handing the site root to a static
/// export's own `/functions/` page, while the local-dir branch (which guards
/// `rest`) served it correctly. Same predicate, same input, both branches.
///
/// Guarding first is not a hole: the rewrite only ever appends `index.html` to
/// a directory-style path, and an artifact is `<name>.js` with `name` matching
/// `^[a-z][a-z0-9-]{0,63}$` (`is_valid_function_name`), so the rewrite can
/// never synthesise a key that reaches one.
fn s3_object_key(rest: &str, accepts_html: bool) -> Option<String> {
    match serve_guard(rest, accepts_html) {
        ServeGuard::NotFound => None,
        ServeGuard::SpaShell => Some("index.html".to_string()),
        ServeGuard::Allow => {
            // Empty or directory-style paths serve that directory's entry.
            let t = rest.trim_start_matches('/');
            Some(if t.is_empty() || t.ends_with('/') {
                format!("{t}index.html")
            } else {
                t.to_string()
            })
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
) -> crate::server::api::custom_apps_sync::Channel {
    use crate::server::api::custom_apps_sync::Channel;
    if staff_wants_draft {
        return Channel::Draft;
    }
    if is_published {
        Channel::Published
    } else {
        Channel::Draft
    }
}

/// Serve a custom-app file directly from S3 (the new publish pipeline).
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

    let Some(requested) = s3_object_key(rest, wants_html(headers)) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // Try the requested object; on a miss, fall back to the SPA shell
    // (`index.html`) — same behavior as the disk path's spa fallback.
    let (rel_used, bytes) =
        match custom_apps_bundle_cache::get_or_fetch(app_id, &build.build_id, &requested).await {
            Ok(Some(b)) => (requested.clone(), b),
            Ok(None) => {
                match custom_apps_bundle_cache::get_or_fetch(app_id, &build.build_id, "index.html")
                    .await
                {
                    Ok(Some(b)) => ("index.html".to_string(), b),
                    Ok(None) => return StatusCode::NOT_FOUND.into_response(),
                    Err(e) => {
                        tracing::error!("app {app_id}: S3 read index.html failed: {e}");
                        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                    }
                }
            }
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
                    "Bundle dir missing or unresolvable for custom app {id} (expected {bundle_dir:?})"
                );
                return (
                    StatusCode::NOT_FOUND,
                    format!(
                        "Bundle not deployed for custom app {id}.\n\
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
    // Server-only subtrees (`functions/<name>.js`) contribute NO real-file
    // candidate — same `serve_guard` the S3 branch takes. `allow_spa_fallback`
    // is `wants_html`, so a blocked navigation still reaches the shell below
    // and a blocked asset fetch falls through to the 404.
    let mut candidates = match serve_guard(rest, allow_spa_fallback) {
        ServeGuard::Allow => vec![
            candidate.clone(),
            candidate.with_extension("html"),
            candidate.join("index.html"),
        ],
        ServeGuard::SpaShell | ServeGuard::NotFound => Vec::new(),
    };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_only_path_matches_the_functions_subtree() {
        assert!(is_server_only_path("functions/top-stores.js"));
        // Leading / doubled / `.` segments must not slip past the check.
        assert!(is_server_only_path("/functions/top-stores.js"));
        assert!(is_server_only_path("//functions/top-stores.js"));
        assert!(is_server_only_path("./functions/top-stores.js"));
        // A case-insensitive filesystem (macOS dev box) would resolve this to
        // the real directory, so the guard can't be case-sensitive either.
        assert!(is_server_only_path("Functions/top-stores.js"));
    }

    #[test]
    fn server_only_path_leaves_ordinary_frontend_files_alone() {
        assert!(!is_server_only_path("index.html"));
        assert!(!is_server_only_path("assets/main.js"));
        // Prefix-scoped, not substring: these are frontend files whose names
        // merely start with (or contain) the word.
        assert!(!is_server_only_path("assets/functions.js"));
        assert!(!is_server_only_path("my-functions/x.js"));
        assert!(!is_server_only_path("static/functions/x.js"));
        assert!(!is_server_only_path(""));
    }

    /// The artifact is always `functions/<name>.js`, never the bare segment,
    /// so matching `functions` alone protected nothing and broke real pages:
    /// an app routing `/functions` client-side, or a static export serving its
    /// own `functions.html` / `functions/index.html`, silently got the site
    /// root at 200 — including apps with no Oxy Functions at all.
    #[test]
    fn server_only_path_requires_a_child_segment() {
        assert!(!is_server_only_path("functions"));
        assert!(!is_server_only_path("functions/"));
        assert!(!is_server_only_path("/functions"));
        assert!(!is_server_only_path("functions/."));
        // …while anything actually inside it stays blocked.
        assert!(is_server_only_path("functions/top-stores.js"));
        assert!(is_server_only_path("functions/nested/deep.js"));
    }

    /// The S3 branch's whole path decision, which otherwise needs a database
    /// and a build store to exercise.
    ///
    /// The `functions/` case is the one that regressed: guarding the value
    /// AFTER the directory-index rewrite saw `functions/index.html`, blocked
    /// it, and served the site root — on the branch that serves production,
    /// for a shape the local-dir branch handled correctly.
    #[test]
    fn s3_object_key_guards_the_raw_path_then_rewrites() {
        // A static export's own `/functions/` page resolves to its real file.
        assert_eq!(
            s3_object_key("functions/", true).as_deref(),
            Some("functions/index.html")
        );
        assert_eq!(
            s3_object_key("functions/", false).as_deref(),
            Some("functions/index.html")
        );
        // The artifact underneath is still blocked either way.
        assert_eq!(
            s3_object_key("functions/top-stores.js", true).as_deref(),
            Some("index.html"),
            "navigation falls back to the shell"
        );
        assert_eq!(s3_object_key("functions/top-stores.js", false), None);
        // An explicit request for a file inside the reserved dir is blocked
        // even when the rewrite would have produced the same key — matching
        // the local-dir branch exactly.
        assert_eq!(s3_object_key("functions/index.html", false), None);
    }

    #[test]
    fn s3_object_key_keeps_ordinary_directory_and_file_paths() {
        assert_eq!(s3_object_key("", true).as_deref(), Some("index.html"));
        assert_eq!(s3_object_key("/", true).as_deref(), Some("index.html"));
        assert_eq!(
            s3_object_key("assets/main.js", false).as_deref(),
            Some("assets/main.js")
        );
        assert_eq!(
            s3_object_key("docs/", true).as_deref(),
            Some("docs/index.html")
        );
        assert_eq!(
            s3_object_key("/assets/main.js", false).as_deref(),
            Some("assets/main.js"),
            "leading slash is stripped"
        );
    }

    /// Both serve branches route through this, so it's the one place the
    /// blocked-request policy is stated.
    #[test]
    fn serve_guard_falls_back_to_the_shell_only_for_html_requests() {
        assert_eq!(
            serve_guard("functions/top-stores.js", true),
            ServeGuard::SpaShell
        );
        assert_eq!(
            serve_guard("functions/top-stores.js", false),
            ServeGuard::NotFound
        );
        // A bare `/functions` page — and its directory-style form, which the
        // S3 branch rewrites to `functions/index.html` — are ordinary.
        assert_eq!(serve_guard("functions", true), ServeGuard::Allow);
        assert_eq!(serve_guard("functions", false), ServeGuard::Allow);
        assert_eq!(serve_guard("functions/", true), ServeGuard::Allow);
        assert_eq!(serve_guard("functions/", false), ServeGuard::Allow);
        assert_eq!(serve_guard("index.html", false), ServeGuard::Allow);
    }

    fn test_runtime() -> AppRuntimeConfig {
        AppRuntimeConfig {
            app_id: Uuid::nil(),
            slug: "hello-oxy".to_string(),
            org_id: Uuid::nil(),
            org_slug: "acme".to_string(),
            project_id: Uuid::nil(),
            branch: "main".to_string(),
            api_base_url: String::new(),
        }
    }

    /// A published bundle carries its compiled Oxy Functions next to the
    /// frontend. The asset route used to hand them out to anyone past the
    /// app's auth gate — handler logic, inlined SQL, and the author's original
    /// TypeScript via the sourcemap. Whatever the Accept header says, the
    /// bytes must never come back.
    #[tokio::test]
    async fn serve_file_never_returns_a_compiled_function() {
        // `TempDir` rather than a hand-rolled path: it cleans up on unwind, so
        // a failing assert below doesn't leak a bundle into the temp dir.
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path();
        std::fs::create_dir_all(dir.join("functions")).expect("mkdir");
        std::fs::write(dir.join("index.html"), b"<html><head></head></html>").expect("index");
        std::fs::write(
            dir.join("functions/top-stores.js"),
            b"const MARGIN_RULE='cost*1.42';export default async()=>MARGIN_RULE;",
        )
        .expect("fn");
        // A static export's own page at the same name (`trailingSlash: false`)
        // — must keep working, sharing the directory with the artifact above.
        // The `trailingSlash: true` shape gets its own fixture below, because
        // the `.html` candidate is tried first and would mask it here.
        std::fs::write(dir.join("functions.html"), b"<html>functions page</html>").expect("page");
        let root = dir.canonicalize().expect("canonicalize");
        let runtime = test_runtime();

        // Asset-style fetch (no SPA fallback): a plain 404, same as any path
        // that doesn't exist — the route doesn't confirm which handlers exist.
        let res = serve_file(&root, "functions/top-stores.js", false, &runtime).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);

        // Navigation (SPA fallback allowed): the shell, never the handler. An
        // app may legitimately route `/functions` client-side.
        let res = serve_file(&root, "functions/top-stores.js", true, &runtime).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 64 * 1024)
            .await
            .expect("body");
        let body = String::from_utf8_lossy(&body);
        assert!(
            !body.contains("MARGIN_RULE"),
            "SPA fallback leaked the function body: {body}"
        );
        assert!(
            body.contains("<html>"),
            "expected the SPA shell, got: {body}"
        );

        // The frontend next to it is unaffected.
        let res = serve_file(&root, "index.html", false, &runtime).await;
        assert_eq!(res.status(), StatusCode::OK);

        // The app's OWN `/functions` page still resolves to its real file via
        // the `.html`-suffix candidate — the guard reserves the subtree, not
        // the name. Previously this returned the site root at 200.
        let res = serve_file(&root, "functions", true, &runtime).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 64 * 1024)
            .await
            .expect("body");
        assert!(
            String::from_utf8_lossy(&body).contains("functions page"),
            "the app's own /functions page must still be served"
        );
    }

    /// The `trailingSlash: true` shape, in its own fixture: a bundler emits
    /// `functions.html` or `functions/index.html`, not both, and the candidate
    /// order (`.html` suffix before `<dir>/index.html`) means a fixture
    /// carrying both only ever exercises the first.
    ///
    /// This is the shape that regressed on the S3 branch, where the guard ran
    /// on the rewritten `functions/index.html` instead of the raw
    /// `functions/`. `s3_object_key_guards_the_raw_path_then_rewrites` pins
    /// the S3 half; this pins the local half.
    #[tokio::test]
    async fn serve_file_serves_a_directory_style_functions_page() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path();
        std::fs::create_dir_all(dir.join("functions")).expect("mkdir");
        std::fs::write(dir.join("index.html"), b"<html>site root</html>").expect("index");
        std::fs::write(
            dir.join("functions/index.html"),
            b"<html>functions dir page</html>",
        )
        .expect("dir page");
        std::fs::write(dir.join("functions/top-stores.js"), b"const SECRET=1;").expect("fn");
        let root = dir.canonicalize().expect("canonicalize");
        let runtime = test_runtime();

        let res = serve_file(&root, "functions/", true, &runtime).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 64 * 1024)
            .await
            .expect("body");
        assert!(
            String::from_utf8_lossy(&body).contains("functions dir page"),
            "the app's own /functions/ directory page must still be served"
        );

        // The artifact sharing that directory is still unreachable.
        let res = serve_file(&root, "functions/top-stores.js", false, &runtime).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
