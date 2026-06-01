//! Diagnostic snapshot for an installed customer app.
//!
//! `GET /api/customer-apps/{org_slug}/{app_slug}/debug` returns a structured
//! snapshot of everything an admin needs to diagnose why a customer app isn't
//! loading. Org-membership gated; response shape is for human inspection and
//! not guaranteed stable.
//!
//! Auth, manifest, and bundle-dir resolution all live in
//! `customer_apps_auth`; this module is a thin handler that assembles the
//! snapshot from those reusable helpers.

use axum::Json;
use axum::extract::Path;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::Value as JsonValue;
use tracing::instrument;
use uuid::Uuid;

use super::customer_apps_auth::{AuthOutcome, authenticate_and_authorize};
use super::customer_apps_manifest::{
    bundle_dir_for, pick_channel_for, resolve_manifest, sanitize_bundle_dir_for_display,
};

// ── Response types ───────────────────────────────────────────────────────────

/// Where the resolved manifest came from.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum ManifestSource {
    /// `apps.manifest_override` is set; bundled `oxy-app.json` is ignored.
    DbOverride,
    /// No override; the bundle's `oxy-app.json` is the source.
    BundleFile,
}

#[derive(Serialize)]
struct DebugSnapshot {
    org_slug: String,
    app_slug: String,
    app: AppSnapshot,
    bundle_dir: Option<String>,
    bundle_dir_exists: bool,
    manifest_source: ManifestSource,
    /// Raw parsed manifest JSON — useful for diagnostics without leaking
    /// admin-grade internal fields (project_id / branch are on the DB app row,
    /// not here).
    manifest: Option<JsonValue>,
    manifest_error: Option<String>,
}

#[derive(Serialize)]
struct AppSnapshot {
    id: Uuid,
    slug: String,
    name: String,
    status: String,
    source_type: String,
}

// ── Handler ──────────────────────────────────────────────────────────────────

/// `GET /api/customer-apps/{org_slug}/{app_slug}/debug`
#[instrument(skip_all, fields(org_slug = %org_slug, app_slug = %app_slug))]
pub async fn get_debug(
    Path((org_slug, app_slug)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let AuthOutcome { app, is_staff } =
        match authenticate_and_authorize(&headers, &org_slug, &app_slug).await {
            Ok(v) => v,
            Err(status) => return status.into_response(),
        };

    let cookie_wants_draft = super::customer_apps_preview::wants_draft_preview(&headers);
    let channel = pick_channel_for(&app, is_staff, cookie_wants_draft);

    let manifest_source = if app.manifest_override.is_some() {
        ManifestSource::DbOverride
    } else {
        ManifestSource::BundleFile
    };

    let mut snap = DebugSnapshot {
        org_slug,
        app_slug,
        app: AppSnapshot {
            id: app.id,
            slug: app.slug.clone(),
            name: app.name.clone(),
            status: app.status.clone(),
            source_type: app.source_type.clone(),
        },
        bundle_dir: None,
        bundle_dir_exists: false,
        manifest_source,
        manifest: None,
        manifest_error: None,
    };

    if let Some(d) = bundle_dir_for(&app) {
        snap.bundle_dir_exists = d.exists();
        snap.bundle_dir = Some(sanitize_bundle_dir_for_display(&app, &d));
    }

    let manifest = match oxy::database::client::establish_connection().await {
        Ok(db) => resolve_manifest(&db, &app, channel).await,
        Err(e) => Err(super::customer_apps_manifest::ManifestError::Io(
            e.to_string(),
        )),
    };
    match manifest {
        Ok(m) => {
            // Serialize the identity manifest back to a raw JSON value so the
            // debug consumer gets a stable opaque blob rather than a typed
            // struct that may drift from the on-disk schema.
            snap.manifest = serde_json::to_value(&m).ok();
        }
        Err(e) => {
            snap.manifest_error = Some(e.to_string());
        }
    }

    Json(snap).into_response()
}
