//! Server-side folder picker for the "Add customer app" dialog.
//!
//! The operator needs to pick an existing bundle directory to link as the
//! source of a customer app. Asking them to type the absolute path is
//! brittle — they hit `~`, `$HOME`, symlinks, trailing slashes, and silent
//! typos that only surface as a 404 the next time the iframe loads.
//!
//! This endpoint mirrors a server-side `ls -d` so the dialog can render
//! a breadcrumb + entry list. The operator clicks their way to a folder,
//! the dialog POSTs that exact path as the bundle source — no string
//! typing.
//!
//! Available whenever the admin guard passes — no extra env flag required.
//!
//! Gated by the surrounding router middleware:
//!   - `/api/admin/...` → oxy_owner_guard
//!   - `/api/customer-apps/...` → oxy_owner_or_app_admin_guard
//!
//! See `internal-docs/customer-apps.md` for the broader design.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::server::router::AppState;

#[derive(Debug, Deserialize)]
pub struct ListdirQuery {
    /// Absolute path to list. If empty, returns the default root
    /// (`$OXY_STATE_DIR/customer-apps` when present, else `$HOME`).
    #[serde(default)]
    pub path: String,
    /// Include dotfiles / dotdirs. Off by default — the operator
    /// almost always wants project folders, not `.git` clutter.
    #[serde(default)]
    pub show_hidden: bool,
}

#[derive(Debug, Serialize)]
pub struct ListdirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[derive(Debug, Serialize)]
pub struct ListdirResponse {
    /// The absolute path actually listed (after `~` / default fallback).
    pub path: String,
    /// Parent directory, or null when at filesystem root.
    pub parent: Option<String>,
    /// Directory entries; files included so the picker can show them
    /// greyed-out, but `is_dir=false` entries are not selectable.
    pub entries: Vec<ListdirEntry>,
}

pub async fn listdir(
    State(_app_state): State<AppState>,
    Query(q): Query<ListdirQuery>,
) -> Result<Json<ListdirResponse>, (StatusCode, String)> {
    let target = resolve_target(&q.path)?;

    let canonical = tokio::fs::canonicalize(&target)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, format!("{}: {e}", target.display())))?;

    let mut rd = tokio::fs::read_dir(&canonical).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("read_dir({}): {e}", canonical.display()),
        )
    })?;

    let mut entries = Vec::new();
    while let Some(de) = rd.next_entry().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("next_entry: {e}"),
        )
    })? {
        let name = de.file_name().to_string_lossy().into_owned();
        if !q.show_hidden && name.starts_with('.') {
            continue;
        }
        // `file_type` follows symlinks intentionally — operators
        // commonly link bundles via symlink and would otherwise see
        // them as non-selectable files.
        let ft = match de.file_type().await {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        entries.push(ListdirEntry {
            name,
            path: de.path().display().to_string(),
            is_dir: ft.is_dir(),
        });
    }
    // Dirs first, then case-insensitive name. Mirrors how every file
    // picker the operator has used since Windows 95 sorts entries.
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(Json(ListdirResponse {
        path: canonical.display().to_string(),
        parent: canonical.parent().map(|p| p.display().to_string()),
        entries,
    }))
}

/// Resolve the requested target path, applying these fallbacks (in order):
///   1. `~` and `~/` → `$HOME`
///   2. empty/`null` request → `$OXY_STATE_DIR/customer-apps` if it
///      exists, else `$HOME`, else `/`
///   3. Rejects non-absolute paths (after expansion).
fn resolve_target(requested: &str) -> Result<PathBuf, (StatusCode, String)> {
    let trimmed = requested.trim();

    if trimmed.is_empty() {
        // Default landing: the dir we'd `mkdir` into for "Create new"
        // is the most useful starting point for "Link existing" too.
        if let Ok(state) = std::env::var("OXY_STATE_DIR") {
            let candidate = PathBuf::from(state).join("customer-apps");
            if candidate.exists() {
                return Ok(candidate);
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            return Ok(PathBuf::from(home));
        }
        return Ok(PathBuf::from("/"));
    }

    let expanded = if let Some(rest) = trimmed.strip_prefix("~/") {
        let home = std::env::var("HOME").map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "cannot expand ~: HOME is not set".to_string(),
            )
        })?;
        PathBuf::from(home).join(rest)
    } else if trimmed == "~" {
        let home = std::env::var("HOME").map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "cannot expand ~: HOME is not set".to_string(),
            )
        })?;
        PathBuf::from(home)
    } else {
        PathBuf::from(trimmed)
    };

    if !Path::new(&expanded).is_absolute() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("path must be absolute: {}", expanded.display()),
        ));
    }
    Ok(expanded)
}

// ── Bundle probe ────────────────────────────────────────────────────────────
//
// When the operator picks a folder in the "Link existing" flow, we'd
// like to know up-front what the bundle thinks its own identity is:
//   - Does it carry an `oxy-app.json` with `name` / `slug`? Those are
//     the canonical values — lock the dialog fields to them so the
//     operator can't choose a slug that won't match the baked base
//     path.
//   - What `/customer-apps/<org>/<slug>/` prefix is baked into the
//     bundle's `index.html`? If the operator's chosen slug doesn't
//     match, the bundle's JS chunks will 404 every data fetch (the
//     serve-time rewrite only patches index.html, not the chunks).
//
// The probe is informational — it never errors on missing files.
// "Doesn't have a manifest" is a valid state for a hand-built bundle.

/// Minimal subset of [`OxyAppManifest`] that we read in probe mode.
/// Reads `schemaVersion` to enforce the v2 identity-only shape and
/// detects v1 fields (`products`, `writers`) so they can be flagged.
#[derive(Debug, Deserialize)]
struct ManifestProbe {
    #[serde(default, rename = "schemaVersion")]
    schema_version: u32,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    slug: Option<String>,
    /// Mirror of `OxyAppManifest::org_slug` / `project_id`. JSON keys
    /// match the manifest's camelCase (orgSlug / projectId).
    #[serde(default, rename = "orgSlug")]
    org_slug: Option<String>,
    #[serde(default, rename = "projectId")]
    project_id: Option<String>,
    /// v1-only fields — presence is an error.
    #[serde(default)]
    products: Option<serde_json::Value>,
    #[serde(default)]
    writers: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct ProbeResponse {
    /// False when the manifest fails v2 validation. The dialog should
    /// surface `warnings` to the operator and block submission.
    pub ok: bool,
    /// Human-readable explanations for any validation failures.
    pub warnings: Vec<String>,
    /// Absolute path actually probed (after canonicalisation).
    pub bundle_dir: String,
    /// Name declared in `oxy-app.json`, if present.
    pub manifest_name: Option<String>,
    /// Slug declared in `oxy-app.json`, if present. When set, the
    /// dialog locks the slug field — overriding it produces a bundle
    /// that 404s every data fetch.
    pub manifest_slug: Option<String>,
    /// Org slug declared in `oxy-app.json`, if present. Prefills the
    /// dialog's org picker; operator can still override. Has no
    /// access-control weight — the actual gate is on the linked row.
    pub manifest_org_slug: Option<String>,
    /// Project (workspace) uuid declared in `oxy-app.json`, if
    /// present. Prefills the dialog's project picker; operator can
    /// still override.
    pub manifest_project_id: Option<String>,
    /// `/customer-apps/<org>/<slug>/` prefix baked into the bundle's
    /// `index.html` at build time (extracted by reading the file's
    /// asset references). When set and it doesn't match the dialog's
    /// chosen slug, the bundle won't work after the link.
    pub baked_base_path: Option<String>,
    /// True when an `index.html` was found at any of the candidate
    /// roots (`<path>`, `<path>/out`, `<path>/dist`). False means the
    /// path isn't a built bundle yet — picker should show this as a
    /// soft warning rather than a hard block.
    pub has_index_html: bool,
    /// Whether this bundle's source uses `@oxy-hq/vite-plugin` (the
    /// Oxy App Kit). `None` = couldn't determine (no `package.json`
    /// found in the probed dir or its parent). `Some(true)` = kit
    /// detected. `Some(false)` = source package.json exists but the
    /// plugin isn't declared. The dialog uses this to nudge operators
    /// of non-kit bundles toward the kit. Not a hard rejection — many
    /// bundles will be hand-rolled forever and that's fine.
    pub uses_oxy_kit: Option<bool>,
}

/// Validate a parsed `ManifestProbe` and return any warnings. An empty
/// `Vec` means the manifest is v2-compliant. A non-empty `Vec` means the
/// dialog should block submission and surface the warnings to the operator.
/// URL the v1-rejection warnings link to. Default points at the
/// GitHub-rendered internal-docs migration guide. Override via
/// `OXY_DOCS_BASE_URL` if you publish the doc elsewhere.
fn migration_doc_url() -> String {
    let base = std::env::var("OXY_DOCS_BASE_URL").unwrap_or_else(|_| {
        "https://github.com/oxy-hq/oxygen-internal/blob/main/internal-docs".to_string()
    });
    // The standalone v1 → v2 migration guide was folded into the
    // consolidated platform doc; bundle authors landing here from a
    // probe-rejection warning find SDK + manifest guidance under §5.
    format!("{}/customer-apps.md", base.trim_end_matches('/'))
}

fn validate_probe(probed: &ManifestProbe) -> Vec<String> {
    let mut warnings = Vec::new();
    // Enforce v2: only schemaVersion:2 identity-only manifests are accepted.
    // v1 manifests carried `products` / `writers` configuration that has
    // since moved server-side. Surface the migration doc inline so the
    // operator has an actionable next step rather than just a rejection.
    let doc = migration_doc_url();
    if probed.schema_version != 2 {
        warnings.push(format!(
            "oxy-app.json schemaVersion is {} — only 2 is supported. \
             Upgrade to the identity-only manifest shape: {doc}",
            probed.schema_version
        ));
    }
    if probed.products.is_some() || probed.writers.is_some() {
        warnings.push(format!(
            "oxy-app.json declares `products` or `writers` (v1 fields); \
             the MVP refactor requires identity-only manifests. \
             Migration guide: {doc}"
        ));
    }
    warnings
}

pub async fn probe(
    State(_app_state): State<AppState>,
    Query(q): Query<ListdirQuery>,
) -> Result<Json<ProbeResponse>, (StatusCode, String)> {
    let target = resolve_target(&q.path)?;
    let canonical = tokio::fs::canonicalize(&target)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, format!("{}: {e}", target.display())))?;

    // Strict: only probe the exact folder the operator picked. We used
    // to also peek into `<dir>/out` and `<dir>/dist` as a convenience
    // for Next.js / Vite project roots, but that "smart" behavior
    // surprised operators — picking a project root would silently
    // surface a manifest from a subfolder they hadn't navigated into.
    // Now what you point at is what gets probed; if you pick the
    // project root and the bundle lives in `out/`, you'll see
    // `has_index_html: false` and the picker prompts you to dive in.
    let mut manifest_name = None;
    let mut manifest_slug = None;
    let mut manifest_org_slug = None;
    let mut manifest_project_id = None;
    let mut baked_base_path = None;
    let mut has_index_html = false;
    let mut warnings: Vec<String> = Vec::new();

    if let Ok(bytes) = tokio::fs::read(canonical.join("oxy-app.json")).await
        && let Ok(probed) = serde_json::from_slice::<ManifestProbe>(&bytes)
    {
        let root_warnings = validate_probe(&probed);
        if root_warnings.is_empty() {
            let trim = |s: Option<String>| s.filter(|s| !s.trim().is_empty());
            manifest_name = trim(probed.name);
            manifest_slug = trim(probed.slug);
            manifest_org_slug = trim(probed.org_slug);
            manifest_project_id = trim(probed.project_id);
        } else {
            warnings = root_warnings;
        }
    }
    if let Ok(bytes) = tokio::fs::read(canonical.join("index.html")).await {
        has_index_html = true;
        if let Ok(html) = std::str::from_utf8(&bytes) {
            baked_base_path =
                crate::server::api::customer_apps_serve::first_customer_apps_prefix(html);
        }
    }

    // Kit detection: look for `@oxy-hq/vite-plugin` in the source's
    // package.json. The probed dir is usually the build output
    // (`out/`), so check both the probed dir and its parent — covers
    // the common shapes (operator picks the source dir, or picks
    // `<source>/out/`).
    let uses_oxy_kit = detect_oxy_kit(&canonical).await;

    let ok = warnings.is_empty();
    Ok(Json(ProbeResponse {
        ok,
        warnings,
        bundle_dir: canonical.display().to_string(),
        manifest_name,
        manifest_slug,
        manifest_org_slug,
        manifest_project_id,
        baked_base_path,
        has_index_html,
        uses_oxy_kit,
    }))
}

/// Heuristic kit detection. Walks UP from the probed dir looking for
/// the first ancestor with a `package.json`, then checks whether
/// `@oxy-hq/vite-plugin` appears in `dependencies` or
/// `devDependencies`. Returns:
///   - `Some(true)` — package.json found and plugin is listed
///   - `Some(false)` — package.json found but plugin is absent
///   - `None`        — no package.json anywhere up to root (bundle
///                     was probed in isolation with no nearby source)
///
/// Why walk up: operators commonly point the picker at the build
/// output directory, which can be `<source>/out/` (the convention),
/// `<source>/dist/` (vite default), or `<source>/build/static/` (a
/// monorepo's nested output). Checking only `<dir>` and `<dir>/..`
/// missed the third shape and silently returned `None`, hiding the
/// kit nudge from operators of monorepo apps even when their source
/// did use the kit. Bounded walk depth (4 ancestors) prevents a
/// pathological symlink loop from blocking the probe.
async fn detect_oxy_kit(canonical: &Path) -> Option<bool> {
    let mut dir = Some(canonical);
    for _ in 0..5 {
        let Some(d) = dir else { break };
        if let Some(found) = check_package_json_for_plugin(d).await {
            return Some(found);
        }
        dir = d.parent();
    }
    None
}

async fn check_package_json_for_plugin(dir: &Path) -> Option<bool> {
    let bytes = tokio::fs::read(dir.join("package.json")).await.ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let has = ["dependencies", "devDependencies"].iter().any(|key| {
        value
            .get(key)
            .and_then(|v| v.as_object())
            .is_some_and(|m| m.contains_key("@oxy-hq/vite-plugin"))
    });
    Some(has)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_relative_paths() {
        let res = resolve_target("relative/path");
        assert!(matches!(res, Err((StatusCode::BAD_REQUEST, _))));
    }

    #[test]
    fn accepts_absolute_paths() {
        let res = resolve_target("/tmp");
        assert_eq!(res.unwrap(), PathBuf::from("/tmp"));
    }

    #[test]
    fn empty_returns_some_default() {
        // We can't predict the env in tests; assert only that we get a
        // path rather than an error.
        let res = resolve_target("");
        assert!(res.is_ok());
    }

    /// v1 manifest (schemaVersion:1, products present) must produce a
    /// schemaVersion warning and a v1-fields warning via `validate_probe`.
    #[test]
    fn probe_rejects_v1_manifest() {
        let json = r#"{"schemaVersion":1,"slug":"x","products":{}}"#;
        let probed: ManifestProbe = serde_json::from_str(json).unwrap();
        let warnings = validate_probe(&probed);

        assert!(
            !warnings.is_empty(),
            "expected warnings for v1 manifest, got none"
        );
        assert!(
            warnings.iter().any(|w| w.contains("schemaVersion")),
            "expected schemaVersion warning, got {:?}",
            warnings
        );
        assert!(
            warnings.iter().any(|w| w.contains("products")),
            "expected v1-fields warning, got {:?}",
            warnings
        );
    }

    /// A v2 identity-only manifest must produce no warnings via `validate_probe`.
    #[test]
    fn probe_accepts_v2_manifest() {
        let json = r#"{"schemaVersion":2,"slug":"acme","orgSlug":"acme-org","projectId":"00000000-0000-0000-0000-000000000001"}"#;
        let probed: ManifestProbe = serde_json::from_str(json).unwrap();
        let warnings = validate_probe(&probed);

        assert!(
            warnings.is_empty(),
            "expected no warnings for v2 manifest, got {:?}",
            warnings
        );
    }

    /// A manifest with no schemaVersion field (defaults to 0) must produce a
    /// schemaVersion warning — freshly-scaffolded bundles that omit the field
    /// need to be caught and upgraded.
    #[test]
    fn probe_rejects_missing_schema_version() {
        let json = r#"{"slug":"x"}"#;
        let probed: ManifestProbe = serde_json::from_str(json).unwrap();
        let warnings = validate_probe(&probed);

        assert!(
            warnings.iter().any(|w| w.contains("schemaVersion")),
            "expected schemaVersion warning for missing field (defaults to 0), got {:?}",
            warnings
        );
    }

    /// An identity-only v2 manifest (schemaVersion + slug only, no orgSlug or
    /// projectId) must produce no warnings. Those two fields are dev hints for
    /// prefilling the dialog — the admin operator provides them via the form,
    /// not the bundle file.
    #[test]
    fn validate_probe_accepts_manifest_without_orgslug_or_projectid() {
        let json = r#"{"schemaVersion":2,"slug":"minimal"}"#;
        let probed: ManifestProbe = serde_json::from_str(json).unwrap();
        let warnings = validate_probe(&probed);
        assert!(
            warnings.is_empty(),
            "expected no warnings for identity-only manifest, got {:?}",
            warnings
        );
    }
}
