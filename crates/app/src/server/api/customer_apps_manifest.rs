//! Manifest schema, caching, and resolution for customer apps.
//!
//! `oxy-app.json` is the source of truth for a bundle's identity (name, slug,
//! org, project). This module owns:
//!
//! - The Rust mirror of that schema (`OxyAppManifest`) — identity fields only.
//! - `resolve_manifest` — the single entry point handlers call; it checks
//!   the DB override first, then falls back to the bundle file.
//! - `pick_channel_for`, `bundle_dir_for`, `sanitize_bundle_dir_for_display`
//!   — helpers for turning an app row into a filesystem path.

use std::path::{Path as StdPath, PathBuf};

use entity::{app_builds, apps};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};

use super::customer_apps_source::AppSource;
use super::customer_apps_sync::Channel;

// ── Manifest schema ──────────────────────────────────────────────────────────

/// Optional Ask-binding declared by the app bundle: which agent the
/// global Ask overlay should bind to on this app's surfaces, plus the
/// suggested-question chips shown on the launcher card.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OxyAppAskConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_questions: Vec<String>,
}

/// Server-side mirror of the bundle's `oxy-app.json` — identity + launcher-card fields.
///
/// The producer/writer struct soup that existed here was dead code: the
/// executor that consumed it was deleted in commit 3b3dfea. The remaining
/// consumer (the debug endpoint) now exposes the raw manifest JSON, so this
/// type only needs to carry the fields that gate routing and display.
///
/// Schema version 2 is required. Version 1 bundles must be re-built with an
/// updated SDK.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OxyAppManifest {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub slug: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// One-line purpose shown on the launcher card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask: Option<OxyAppAskConfig>,
    /// Card art for the launcher — a path RELATIVE to the bundle root
    /// (e.g. "card.png"), served through the app's own public URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub art: Option<String>,
    /// Shell-rail icon — a path RELATIVE to the bundle root (e.g.
    /// "icon.svg"), served through the app's own public URL. Small square
    /// mark; the rail falls back to a name-initial tile when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Status line shown on the launcher card (e.g.
    /// "23 stores · sales +33.5% YoY · live"). A plain display string —
    /// static for now; a live data binding can replace it later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

// ── Manifest error ───────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub(super) enum ManifestError {
    #[error("oxy-app.json not found in bundle directory")]
    NotFound,
    #[error("oxy-app.json could not be read: {0}")]
    Io(String),
    #[error("oxy-app.json is not valid JSON: {0}")]
    Parse(String),
    #[error("oxy-app.json schemaVersion {0} is not supported; expected 2")]
    UnsupportedSchema(u32),
}

async fn read_manifest(bundle_dir: &StdPath) -> Result<OxyAppManifest, ManifestError> {
    let path = bundle_dir.join("oxy-app.json");
    let bytes = tokio::fs::read(&path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ManifestError::NotFound
        } else {
            ManifestError::Io(e.to_string())
        }
    })?;
    let manifest: OxyAppManifest =
        serde_json::from_slice(&bytes).map_err(|e| ManifestError::Parse(e.to_string()))?;
    if manifest.schema_version != 2 {
        return Err(ManifestError::UnsupportedSchema(manifest.schema_version));
    }
    Ok(manifest)
}

// ── Manifest resolution ──────────────────────────────────────────────────────

fn parse_manifest_value(raw: serde_json::Value) -> Result<OxyAppManifest, ManifestError> {
    let manifest: OxyAppManifest =
        serde_json::from_value(raw).map_err(|e| ManifestError::Parse(e.to_string()))?;
    if manifest.schema_version != 2 {
        return Err(ManifestError::UnsupportedSchema(manifest.schema_version));
    }
    Ok(manifest)
}

/// Resolve the manifest for an app on a given channel. Precedence:
///   1. `apps.manifest_override` (per-deployment override).
///   2. S3 apps: the manifest captured in the channel's current build row
///      (`app_builds.manifest_json`, written at publish from the bundle's
///      `oxy-app.json`). No build pointer → `NotFound`.
///   3. Local-folder apps: the `oxy-app.json` on disk (dev).
/// Called by the customer-apps debug endpoint and the workspace custom-apps
/// list (launcher card metadata); the live serve path injects identity via
/// `window.__OXY_APP__` and serves the bundle's `oxy-app.json` directly.
pub(super) async fn resolve_manifest(
    db: &DatabaseConnection,
    app: &apps::Model,
    channel: Channel,
) -> Result<OxyAppManifest, ManifestError> {
    if let Some(raw) = &app.manifest_override {
        return parse_manifest_value(raw.clone());
    }
    match AppSource::from_model(app).map_err(|_| ManifestError::NotFound)? {
        AppSource::LocalFolder { path } => read_manifest(StdPath::new(&path)).await,
        AppSource::S3 => {
            let build_pk = match channel {
                Channel::Draft => app.draft_build_id,
                Channel::Published => app.published_build_id,
            }
            .ok_or(ManifestError::NotFound)?;
            let build = app_builds::Entity::find_by_id(build_pk)
                .one(db)
                .await
                .map_err(|e| ManifestError::Io(e.to_string()))?
                .ok_or(ManifestError::NotFound)?;
            parse_manifest_value(build.manifest_json.ok_or(ManifestError::NotFound)?)
        }
        AppSource::V0 { .. } => Err(ManifestError::NotFound),
    }
}

/// Resolve manifests for MANY apps with a **single** `app_builds` query — the
/// batched counterpart to [`resolve_manifest`], for paginated list endpoints
/// (avoids an N+1 per-page). Precedence per app mirrors `resolve_manifest`:
/// `manifest_override` → the published build's `manifest_json` (falling back to
/// the draft build) → local-folder disk → none. Returns a map keyed by app id;
/// apps with no resolvable manifest are simply absent. Metadata — individual
/// failures are skipped, never fatal. See the `oxy-app-visual-identity` skill.
pub(super) async fn resolve_manifests_batch(
    db: &DatabaseConnection,
    apps: &[apps::Model],
) -> std::collections::HashMap<uuid::Uuid, OxyAppManifest> {
    use std::collections::{HashMap, HashSet};
    // 1. Collect the S3 build ids we might read (published + draft fallback),
    //    skipping apps that carry an inline override (no build lookup needed).
    let mut build_ids: HashSet<uuid::Uuid> = HashSet::new();
    for app in apps {
        if app.manifest_override.is_some() {
            continue;
        }
        if matches!(AppSource::from_model(app), Ok(AppSource::S3)) {
            build_ids.extend(app.published_build_id);
            build_ids.extend(app.draft_build_id);
        }
    }
    // 2. One query for every build manifest on the page.
    let builds: HashMap<uuid::Uuid, serde_json::Value> = if build_ids.is_empty() {
        HashMap::new()
    } else {
        app_builds::Entity::find()
            .filter(app_builds::Column::Id.is_in(build_ids))
            .all(db)
            .await
            .unwrap_or_default()
            .into_iter()
            .filter_map(|b| Some((b.id, b.manifest_json?)))
            .collect()
    };
    // 3. Resolve each app from the pre-fetched map (or its override / disk).
    let mut out = HashMap::with_capacity(apps.len());
    for app in apps {
        if let Some(m) = manifest_from_prefetched(app, &builds).await {
            out.insert(app.id, m);
        }
    }
    out
}

/// Per-app resolution against a pre-fetched `build_id → manifest_json` map — the
/// same precedence as [`resolve_manifest`], minus the DB round-trip.
async fn manifest_from_prefetched(
    app: &apps::Model,
    builds: &std::collections::HashMap<uuid::Uuid, serde_json::Value>,
) -> Option<OxyAppManifest> {
    if let Some(raw) = &app.manifest_override {
        return parse_manifest_value(raw.clone()).ok();
    }
    match AppSource::from_model(app).ok()? {
        AppSource::LocalFolder { path } => read_manifest(StdPath::new(&path)).await.ok(),
        AppSource::S3 => {
            let from_build = |id: Option<uuid::Uuid>| {
                id.and_then(|i| builds.get(&i))
                    .and_then(|v| parse_manifest_value(v.clone()).ok())
            };
            from_build(app.published_build_id).or_else(|| from_build(app.draft_build_id))
        }
        AppSource::V0 { .. } => None,
    }
}

// ── Channel selection ────────────────────────────────────────────────────────

pub(super) fn pick_channel_for(
    app: &apps::Model,
    is_staff: bool,
    cookie_wants_draft: bool,
) -> super::customer_apps_sync::Channel {
    use super::customer_apps_sync::Channel;
    if is_staff && cookie_wants_draft {
        return Channel::Draft;
    }
    if app.published_at.is_some() {
        Channel::Published
    } else {
        Channel::Draft
    }
}

// ── Bundle dir resolution ────────────────────────────────────────────────────

/// Local filesystem bundle dir, when one exists. Only `LocalFolder`
/// (dev) sources have one now; S3 apps are served from S3 with no local
/// copy, and V0 apps are iframes. Used by the debug endpoint for display.
pub(super) fn bundle_dir_for(app: &apps::Model) -> Option<PathBuf> {
    match AppSource::from_model(app).ok()? {
        AppSource::LocalFolder { path } => Some(path),
        AppSource::S3 | AppSource::V0 { .. } => None,
    }
}

pub(super) fn sanitize_bundle_dir_for_display(app: &apps::Model, dir: &StdPath) -> String {
    if app.source_type == "s3" {
        if let Some(s) = dir.to_str()
            && let Some(idx) = s.find("customer-apps/")
        {
            return s[idx..].to_string();
        }
        return dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "<unknown>".to_string());
    }
    dir.display().to_string()
}

#[cfg(test)]
mod card_metadata_tests {
    use super::*;

    #[test]
    fn manifest_parses_description_and_ask_block() {
        let json = serde_json::json!({
            "schemaVersion": 2,
            "slug": "site-scout",
            "description": "Find your next location",
            "art": "card.png",
            "ask": {
                "agent": "agents/restaurant_analyst.agentic.yml",
                "suggestedQuestions": ["Why does Pleasanton rank #1?"]
            }
        });
        let m: OxyAppManifest = serde_json::from_value(json).unwrap();
        assert_eq!(m.description.as_deref(), Some("Find your next location"));
        assert_eq!(m.art.as_deref(), Some("card.png"));
        let ask = m.ask.expect("ask block");
        assert_eq!(
            ask.agent.as_deref(),
            Some("agents/restaurant_analyst.agentic.yml")
        );
        assert_eq!(
            ask.suggested_questions,
            vec!["Why does Pleasanton rank #1?"]
        );
    }

    #[test]
    fn manifest_without_card_metadata_still_parses() {
        let json = serde_json::json!({ "schemaVersion": 2, "slug": "bare" });
        let m: OxyAppManifest = serde_json::from_value(json).unwrap();
        assert!(m.description.is_none());
        assert!(m.ask.is_none());
        assert!(m.art.is_none());
    }
}
