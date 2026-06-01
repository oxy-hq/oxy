//! Manifest schema, caching, and resolution for customer apps.
//!
//! `oxy-app.json` is the source of truth for a bundle's identity (name, slug,
//! org, project). This module owns:
//!
//! - The Rust mirror of that schema (`OxyAppManifest`) — identity fields only.
//! - A mtime-keyed in-process cache so repeated asset requests re-use the
//!   last parsed manifest without re-reading disk.
//! - `resolve_manifest` — the single entry point handlers call; it checks
//!   the DB override first, then falls back to the bundle file.
//! - `pick_channel_for`, `bundle_dir_for`, `sanitize_bundle_dir_for_display`
//!   — helpers for turning an app row into a filesystem path.

use std::path::{Path as StdPath, PathBuf};

use entity::{app_builds, apps};
use sea_orm::{DatabaseConnection, EntityTrait};
use serde::{Deserialize, Serialize};

use super::customer_apps_source::AppSource;
use super::customer_apps_sync::Channel;

// ── Manifest schema ──────────────────────────────────────────────────────────

/// Server-side mirror of the bundle's `oxy-app.json` — identity fields only.
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

fn read_manifest(bundle_dir: &StdPath) -> Result<OxyAppManifest, ManifestError> {
    let path = bundle_dir.join("oxy-app.json");
    let bytes = std::fs::read(&path).map_err(|e| {
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
/// Debug-endpoint only; the live serve path injects identity via
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
        AppSource::LocalFolder { path } => read_manifest(StdPath::new(&path)),
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
        AppSource::LocalFolder { path } => Some(PathBuf::from(path)),
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
