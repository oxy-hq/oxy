//! Workspace-scoped operator routes. Mounted by the app crate under
//! `/{workspace_id}/...` behind the standard `workspace_middleware`
//! (cloud) or `local_context_middleware` (local). The path's
//! `workspace_id` is therefore trusted — `workspace_middleware`
//! rejects callers who aren't in the workspace's org *before* a
//! handler in here runs.
//!
//! Service-layer fns enforce a second check: that the referenced
//! resource (site, camera) belongs to the URL's `workspace_id`. This
//! protects against a caller in workspace A guessing or remembering a
//! `site_id` / `cam_id` from workspace B. The two checks together close
//! the workspace-leak surface.

use axum::extract::{Path, Query};
use axum::http::HeaderValue;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post, put};
use axum::{Extension, Json, Router};
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::routes::dto::{
    ApplyUpdateBody, BindExistingBody, BulkPatchTargetBody, BulkPatchTargetResponse,
    CameraComplianceSummaryDto, CameraDto, CameraSummaryDto, ClaimDto, ComplianceReportDto,
    CreateCameraBody, CreateSiteBody, DomainPackDto, DomainPackSummaryDto, EdgeBoxDto,
    EdgeBoxSummaryDto, FleetMemberDto, PatchConsentBody, PatchEdgeBoxTargetBody, PatchRoleBody,
    PatchZonesBody, PostPackForkBody, PrepareDeviceBody, PrepareDeviceResponse, PutPackBody,
    RemoveFleetMemberBody, SiteDto, StarterPackSummaryDto, UnifiImportBody, UnifiPreviewBody,
    WebrtcSessionDto, WorkspacePath,
};
use crate::routes::errors::map as map_err;
use crate::service::{
    audit, budget, camera_health, clips, compliance, config, cost, fleet, listing, logs,
    onboarding, packs, preview, provisioning, updates, webrtc,
};
use oxy_auth::extractor::OptionalUserExtractor;
use oxy_unifi::UnifiClient;

/// All workspace-scoped operator endpoints.
///
/// URL shape (after the app crate nests under `/{workspace_id}`):
///
/// **List / detail (UI dashboard):**
/// - `GET    /{wid}/cameras/sites`                — list sites in workspace
/// - `GET    /{wid}/cameras/sites/{site_id}`      — one site
/// - `GET    /{wid}/cameras/edge-boxes`           — list edge boxes (+ site_name)
/// - `GET    /{wid}/cameras/edge-boxes/{box_id}`  — one edge box
/// - `GET    /{wid}/cameras`                      — list cameras (+ joined names)
/// - `GET    /{wid}/cameras/{cam_id}`             — one camera
///
/// **Mutations:**
/// - `POST   /{wid}/cameras/edge-boxes`           — register a new edge box
/// - `PATCH  /{wid}/cameras/{cam_id}/zones`       — update zone / line geometry
/// - `POST   /{wid}/integrations/unifi/preview`   — preview UniFi inventory
/// - `POST   /{wid}/integrations/unifi/import`    — import sites + cameras
///
/// Axum routes literal segments before parameter ones, so
/// `/cameras/sites` and `/cameras/edge-boxes` correctly win over
/// `/cameras/{cam_id}` even with both registered.
///
/// Returned as a single `Router<S>` so the app crate can mount it
/// alongside the rest of `build_workspace_routes` (which is also
/// generic over `AppState`).
pub fn routes<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        // List/detail (literal segments first; axum's matcher prefers
        // them over `{cam_id}` anyway, but registering in this order
        // makes the layout obvious to a reader).
        .route("/cameras/sites", get(list_sites).post(post_site))
        .route("/cameras/sites/{site_id}", get(get_site).patch(patch_site))
        // NAT bulk rewrite. Operator pastes per-camera LAN URLs;
        // server rewrites them onto the site's public_ip + UniFi
        // WAN port (7447). Pre-edge-box flow — once the box is
        // installed, the worker will reconfigure to LAN paths.
        .route(
            "/cameras/sites/{site_id}/bulk-rtsp-rewrite",
            post(post_site_bulk_rtsp_rewrite),
        )
        // Targeted UniFi resync — refreshes just `sites.public_ip`
        // for one site against the UniFi controller's current WAN
        // IP. Cheap alternative to running a full re-import when an
        // ISP rotation breaks the rewritten RTSP URLs.
        .route(
            "/cameras/sites/{site_id}/resync-public-ip",
            post(post_site_resync_public_ip),
        )
        // Workspace-scoped UniFi API key store. PUT to set/upsert,
        // GET status for the UI badge ("configured" without leaking
        // the key), DELETE to clear. Plaintext lives encrypted in
        // org_secrets; this table only holds the lookup + audit.
        .route(
            "/cameras/unifi-credentials",
            axum::routing::put(put_unifi_credentials).delete(delete_unifi_credentials),
        )
        .route(
            "/cameras/unifi-credentials/status",
            get(get_unifi_credentials_status),
        )
        // VLM ↔ YOLO arbitration. PUT writes (idempotent — same
        // report_id upserts), GET reads, DELETE clears the operator's
        // verdict. The list endpoint feeds the "recent arbitrations"
        // dashboard widget.
        .route(
            "/cameras/arbitrations",
            get(list_arbitrations).put(put_arbitration),
        )
        .route(
            "/cameras/arbitrations/{report_id}",
            get(get_arbitration).delete(delete_arbitration),
        )
        // Per-site rollup of compliance incidents across cameras.
        // Drives the Compliance tab's landing view (one row per
        // camera, with total + violations + last_received_at).
        .route(
            "/cameras/sites/{site_id}/compliance-summary",
            get(get_site_compliance_summary),
        )
        .route("/cameras/edge-boxes", get(list_edge_boxes))
        .route(
            "/cameras/fleet",
            get(list_fleet_members).delete(delete_fleet_member),
        )
        .route("/cameras/edge-boxes/{box_id}", get(get_edge_box))
        .route(
            "/cameras/edge-boxes/{box_id}/target",
            patch(patch_edge_box_target),
        )
        .route("/cameras/edge-boxes/{box_id}/cost", get(get_edge_box_cost))
        .route("/cameras/edge-boxes/{box_id}/logs", get(get_edge_box_logs))
        .route("/cameras/edge-boxes/bulk/target", patch(patch_bulk_target))
        .route("/cameras/cost", get(get_workspace_cost))
        .route(
            "/cameras/budget",
            get(get_budget_status).patch(patch_budget),
        )
        .route("/cameras/audit-events", get(list_audit_events))
        .route("/cameras/health-summary", get(get_camera_health_summary))
        .route("/cameras/clips/{report_id}/url", get(get_clip_playback_url))
        .route("/cameras", get(list_cameras).post(post_camera))
        .route("/cameras/{cam_id}", get(get_camera))
        .route("/cameras/{cam_id}/preview/snapshot.jpg", get(get_snapshot))
        // HLS wildcard: matches both the playlist (`/index.m3u8`) and
        // every segment (`/seg000.ts`, `/init.mp4`, etc.) under the
        // same prefix, so relative URLs in the playlist resolve back
        // through us without rewriting.
        .route("/cameras/{cam_id}/preview/hls/{*tail}", get(get_hls))
        // Recording playback: takes ?start=<iso8601>&duration=<seconds>
        // and returns a single fMP4 clip. Used by compliance-report
        // viewers to play the evidence window.
        .route(
            "/cameras/{cam_id}/preview/recording",
            get(get_recording_clip),
        )
        // Lists which time windows actually have recordings on disk.
        // Lets the UI shade the scrubber and snap free-scrub clicks
        // onto a valid segment instead of erroring on gaps.
        .route(
            "/cameras/{cam_id}/preview/recording-segments",
            get(list_recording_segments),
        )
        // WebRTC live-preview session helper. Mints per-session URL
        // + ephemeral TURN credential the browser uses to dial the
        // box's MediaMTX directly via Tailscale Funnel. Oxy is in
        // the signaling path only — media goes browser ⇆ Funnel ⇆
        // box, never through Oxy.
        .route(
            "/cameras/{cam_id}/preview/webrtc/session",
            post(post_webrtc_session),
        )
        // Compliance reports for one camera, newest first. Drives the
        // Developer Portal's Compliance tab list page. Reads from
        // Airhouse via the broker (ComplianceReportsRead purpose).
        .route(
            "/cameras/{cam_id}/compliance-reports",
            get(list_compliance_reports),
        )
        // Mutations
        .route("/cameras/{cam_id}/zones", patch(patch_zones))
        .route(
            "/cameras/{cam_id}/proposed-geometry",
            put(put_proposed_geometry),
        )
        .route(
            "/cameras/{cam_id}/proposed-geometry/approve",
            post(post_approve_proposed_geometry),
        )
        .route(
            "/cameras/{cam_id}/proposed-geometry/reject",
            post(post_reject_proposed_geometry),
        )
        .route("/cameras/{cam_id}/consent", patch(patch_consent))
        .route("/cameras/{cam_id}/role", patch(patch_role))
        .route("/camera-packs", get(list_packs).post(post_pack_fork))
        .route("/camera-packs/active", get(get_active_pack))
        .route("/camera-packs/starters", get(list_starters))
        .route("/camera-packs/starter-updates", get(list_starter_updates))
        .route("/camera-packs/{pack_key}/body", put(put_pack_body))
        .route(
            "/camera-packs/{pack_key}/activate",
            post(post_pack_activate),
        )
        .route(
            "/camera-packs/{pack_key}/apply-update",
            post(post_apply_update),
        )
        .route("/integrations/unifi/preview", post(unifi_preview))
        .route("/integrations/unifi/import", post(unifi_import))
        .route("/fleet/claims/bind-existing", post(post_bind_existing))
        .route("/fleet/devices", get(list_fleet_devices))
        .route("/fleet/prepared-devices", post(post_prepare_device))
        .route("/fleet/dashboard", get(get_dashboard_rollup))
        .route("/fleet/alerts", get(get_recent_alerts))
        .route(
            "/fleet/compliance-reports",
            get(list_fleet_compliance_reports),
        )
        .route(
            "/fleet/compliance-summary",
            get(get_fleet_compliance_summary),
        )
        .route("/fleet/test-alert", post(post_test_alert))
        .route(
            "/cameras/edge-boxes/{box_id}/cohort",
            patch(patch_edge_box_cohort),
        )
        .route(
            "/fleet/rollouts",
            get(list_rollout_plans).post(create_rollout_plan),
        )
        .route("/fleet/rollouts/{plan_id}/abort", post(abort_rollout_plan))
        .route(
            "/fleet/rollouts/{plan_id}/convergence",
            get(get_rollout_convergence),
        )
}

// ── Sites ───────────────────────────────────────────────────────────────────

async fn list_sites(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
) -> Response {
    match listing::list_sites(&db, workspace_id).await {
        Ok(sites) => {
            let dtos: Vec<SiteDto> = sites.into_iter().map(Into::into).collect();
            Json(dtos).into_response()
        }
        Err(e) => map_err(e),
    }
}

#[derive(serde::Deserialize)]
struct WorkspaceSitePath {
    workspace_id: Uuid,
    site_id: Uuid,
}

/// `POST /{wid}/cameras/sites` — operator creates a site manually
/// (no UniFi import). See `service::provisioning::create_site` for
/// the validation rules. Returns the created site row.
async fn post_site(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    Json(body): Json<CreateSiteBody>,
) -> Response {
    let input = provisioning::CreateSiteInput {
        name: body.name,
        timezone: body.timezone,
        region: body.region,
    };
    match provisioning::create_site(&db, workspace_id, input).await {
        Ok(site) => Json(SiteDto::from(site)).into_response(),
        Err(e) => map_err(e),
    }
}

async fn get_site(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceSitePath {
        workspace_id,
        site_id,
    }): Path<WorkspaceSitePath>,
) -> Response {
    match listing::get_site(&db, workspace_id, site_id).await {
        Ok(site) => Json(SiteDto::from(site)).into_response(),
        Err(e) => map_err(e),
    }
}

/// Body for `PATCH /{wid}/cameras/sites/{site_id}`. Each field is
/// `Option<Option<T>>`-shaped via serde's `double_option` so the
/// route can distinguish absent (no change) from explicit null
/// (clear the field).
///
/// Today only `public_ip`, `region`, and `timezone` are editable
/// post-create. Adding new fields is `add a column + ActiveModel
/// hunk in service::sites::update_site + DTO field here`.
#[derive(Debug, serde::Deserialize)]
struct PatchSiteBody {
    #[serde(default, with = "::serde_with::rust::double_option")]
    public_ip: Option<Option<String>>,
    #[serde(default, with = "::serde_with::rust::double_option")]
    region: Option<Option<String>>,
    timezone: Option<String>,
}

/// `PATCH /{wid}/cameras/sites/{site_id}` — operator-facing edit.
async fn patch_site(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceSitePath {
        workspace_id,
        site_id,
    }): Path<WorkspaceSitePath>,
    Json(body): Json<PatchSiteBody>,
) -> Response {
    let input = crate::service::sites::UpdateSiteInput {
        public_ip: body.public_ip,
        region: body.region,
        timezone: body.timezone,
    };
    match crate::service::sites::update_site(&db, workspace_id, site_id, input).await {
        Ok(site) => Json(SiteDto::from(site)).into_response(),
        Err(e) => map_err(e),
    }
}

/// `POST /{wid}/cameras/sites/{site_id}/bulk-rtsp-rewrite` — operator
/// pastes `Camera Name, rtsps://<lan>:7441/<stream_id>` lines; we
/// rewrite each onto `rtsp://<site.public_ip>:7447/<stream_id>` and
/// update the matched camera row's `rtsp_url`. Site must have
/// `public_ip` set (errors with InvalidInput otherwise).
async fn post_site_bulk_rtsp_rewrite(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceSitePath {
        workspace_id,
        site_id,
    }): Path<WorkspaceSitePath>,
    Json(body): Json<crate::service::sites::BulkRewriteInput>,
) -> Response {
    match crate::service::sites::bulk_rewrite_camera_rtsp(&db, workspace_id, site_id, body).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => map_err(e),
    }
}

/// Body for `POST /:wid/cameras/sites/:id/resync-public-ip`. `api_key`
/// is now optional — when omitted, the service falls back to the
/// stored UniFi credential (PUT via /cameras/unifi-credentials). The
/// inline path stays so first-time setup / one-off ad-hoc invocations
/// still work before the operator commits a stored cred.
#[derive(Debug, Default, serde::Deserialize)]
struct ResyncPublicIpBody {
    #[serde(default)]
    api_key: Option<String>,
}

async fn post_site_resync_public_ip(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceSitePath {
        workspace_id,
        site_id,
    }): Path<WorkspaceSitePath>,
    body: Option<Json<ResyncPublicIpBody>>,
) -> Response {
    let api_key = body.and_then(|Json(b)| b.api_key);
    match crate::service::sites::resync_public_ip_from_unifi(&db, workspace_id, site_id, api_key)
        .await
    {
        Ok(result) => Json(result).into_response(),
        Err(e) => map_err(e),
    }
}

#[derive(Debug, serde::Deserialize)]
struct PutUnifiCredentialsBody {
    api_key: String,
}

/// `PUT /:wid/cameras/unifi-credentials` — upsert the stored key.
async fn put_unifi_credentials(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    OptionalUserExtractor(user): OptionalUserExtractor,
    Json(body): Json<PutUnifiCredentialsBody>,
) -> Response {
    let actor = user.as_ref().map(|u| u.id);
    match crate::service::unifi_credentials::set_api_key(&db, workspace_id, &body.api_key, actor)
        .await
    {
        Ok(status) => Json(status).into_response(),
        Err(e) => map_err(e),
    }
}

/// `GET /:wid/cameras/unifi-credentials/status` — drives the
/// "configured ✓" badge in the UI. Never returns the secret itself.
async fn get_unifi_credentials_status(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
) -> Response {
    match crate::service::unifi_credentials::get_status(&db, workspace_id).await {
        Ok(status) => Json(status).into_response(),
        Err(e) => map_err(e),
    }
}

/// `DELETE /:wid/cameras/unifi-credentials` — clear the stored key.
/// No-op when nothing is configured.
async fn delete_unifi_credentials(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
) -> Response {
    match crate::service::unifi_credentials::delete_api_key(&db, workspace_id).await {
        Ok(()) => axum::http::StatusCode::NO_CONTENT.into_response(),
        Err(e) => map_err(e),
    }
}

// ── VLM ↔ YOLO arbitrations ─────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
struct PutArbitrationBody {
    report_id: String,
    /// One of `vlm_right` | `yolo_right` | `neither`.
    verdict: String,
    /// Optional operator note (free text). Stored verbatim.
    #[serde(default)]
    notes: Option<String>,
}

/// `PUT /:wid/cameras/arbitrations` — upsert. Same `report_id`
/// replaces the existing verdict; the table only holds the operator's
/// current belief, not a history.
async fn put_arbitration(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    OptionalUserExtractor(user): OptionalUserExtractor,
    Json(body): Json<PutArbitrationBody>,
) -> Response {
    let verdict = match crate::service::arbitrations::ArbitrationVerdict::parse(&body.verdict) {
        Ok(v) => v,
        Err(e) => return map_err(e),
    };
    let actor = user.as_ref().map(|u| u.id);
    match crate::service::arbitrations::upsert(
        &db,
        workspace_id,
        body.report_id,
        verdict,
        body.notes,
        actor,
    )
    .await
    {
        Ok(row) => Json(row).into_response(),
        Err(e) => map_err(e),
    }
}

#[derive(Debug, serde::Deserialize)]
struct ListArbitrationsQuery {
    /// Defaults to 50 — dashboard widget cap. Server clamps to 500.
    limit: Option<u32>,
}

/// `GET /:wid/cameras/arbitrations?limit=N` — most-recent-updated
/// arbitrations across the workspace. Feeds the "labeled events"
/// dashboard widget + the future training-data export.
async fn list_arbitrations(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    Query(query): Query<ListArbitrationsQuery>,
) -> Response {
    match crate::service::arbitrations::list_for_workspace(
        &db,
        workspace_id,
        query.limit.unwrap_or(50),
    )
    .await
    {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => map_err(e),
    }
}

#[derive(Debug, serde::Deserialize)]
struct ArbitrationPath {
    workspace_id: Uuid,
    report_id: String,
}

/// `GET /:wid/cameras/arbitrations/:report_id` — lookup for a single
/// report. Returns 404 when no arbitration exists.
async fn get_arbitration(
    Extension(db): Extension<DatabaseConnection>,
    Path(ArbitrationPath {
        workspace_id,
        report_id,
    }): Path<ArbitrationPath>,
) -> Response {
    match crate::service::arbitrations::get_for_report(&db, workspace_id, &report_id).await {
        Ok(Some(row)) => Json(row).into_response(),
        Ok(None) => axum::http::StatusCode::NOT_FOUND.into_response(),
        Err(e) => map_err(e),
    }
}

/// `DELETE /:wid/cameras/arbitrations/:report_id` — clear an
/// arbitration. Idempotent.
async fn delete_arbitration(
    Extension(db): Extension<DatabaseConnection>,
    Path(ArbitrationPath {
        workspace_id,
        report_id,
    }): Path<ArbitrationPath>,
) -> Response {
    match crate::service::arbitrations::delete_for_report(&db, workspace_id, &report_id).await {
        Ok(()) => axum::http::StatusCode::NO_CONTENT.into_response(),
        Err(e) => map_err(e),
    }
}

/// Query string for [`get_site_compliance_summary`]. Same `since`
/// semantics as the per-camera report list — RFC 3339, omit for "all
/// time."
#[derive(serde::Deserialize)]
struct ComplianceSummaryQuery {
    since: Option<String>,
}

/// Site rollup: incident counts per camera in this site. Drives the
/// Compliance tab landing view. Returns one row per camera even when
/// the camera has zero reports (so the UI can show "0 incidents" as
/// actionable signal — the camera is configured, just clean).
async fn get_site_compliance_summary(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceSitePath {
        workspace_id,
        site_id,
    }): Path<WorkspaceSitePath>,
    Query(ComplianceSummaryQuery { since }): Query<ComplianceSummaryQuery>,
) -> Response {
    let since_dt = match since {
        None => None,
        Some(s) => match chrono::DateTime::parse_from_rfc3339(&s) {
            Ok(dt) => Some(dt.with_timezone(&chrono::Utc)),
            Err(e) => {
                return map_err(crate::service::ServiceError::InvalidInput(format!(
                    "invalid `since` (expected RFC 3339): {e}"
                )));
            }
        },
    };
    match compliance::summary_for_site(&db, workspace_id, site_id, since_dt).await {
        Ok(rows) => {
            let dtos: Vec<CameraComplianceSummaryDto> = rows.into_iter().map(Into::into).collect();
            Json(dtos).into_response()
        }
        Err(e) => map_err(e),
    }
}

// ── Edge boxes (list / detail) ──────────────────────────────────────────────

async fn list_edge_boxes(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
) -> Response {
    match listing::list_edge_boxes(&db, workspace_id).await {
        Ok(rows) => {
            let dtos: Vec<EdgeBoxSummaryDto> = rows.into_iter().map(Into::into).collect();
            Json(dtos).into_response()
        }
        Err(e) => map_err(e),
    }
}

#[derive(Debug, serde::Deserialize)]
struct DashboardQuery {
    /// ISO-8601 timestamp; defaults to "24h ago" when missing so a
    /// fresh page load doesn't require a deliberate range pick.
    since: Option<chrono::DateTime<chrono::Utc>>,
    /// Optional site narrow. When unset, rollup covers the whole
    /// workspace. Ownership of the site is checked in the service.
    site_id: Option<Uuid>,
}

/// `GET /{wid}/fleet/dashboard?since=&site_id=?` — single rollup that
/// powers the Dashboard tab. Returns totals + per-hour buckets + top
/// cameras by detections + top cameras by violations. Replaces the
/// per-site fan-out the frontend used in v0; one Airhouse round-trip
/// (two SELECTs back-to-back over the same connection) regardless of
/// how many sites the workspace owns.
async fn get_dashboard_rollup(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    axum::extract::Query(query): axum::extract::Query<DashboardQuery>,
) -> Response {
    let since = query
        .since
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::hours(24));
    match crate::service::dashboard::rollup(&db, workspace_id, query.site_id, since).await {
        Ok(rollup) => Json(rollup).into_response(),
        Err(e) => map_err(e),
    }
}

#[derive(Debug, serde::Deserialize)]
struct RecentAlertsQuery {
    site_id: Option<Uuid>,
    /// Defaults to 20 — Dashboard panel shows a short list with a
    /// "see all" affordance over to the Audit tab for the full feed.
    limit: Option<u32>,
}

#[derive(Debug, serde::Deserialize)]
struct FleetComplianceReportsQuery {
    site_id: Option<Uuid>,
    since: Option<chrono::DateTime<chrono::Utc>>,
    /// Defaults server-side to a sane page size. Clamped to MAX_LIST_LIMIT
    /// inside the service.
    limit: Option<u32>,
}

/// `GET /{wid}/fleet/compliance-reports?site_id=?&since=?&limit=?` —
/// newest-first compliance reports across the workspace (or one site).
/// One Airhouse round-trip regardless of camera count. Drives the
/// Detections page (replaces a per-camera fan-out).
async fn list_fleet_compliance_reports(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    axum::extract::Query(query): axum::extract::Query<FleetComplianceReportsQuery>,
) -> Response {
    let limit = query
        .limit
        .unwrap_or(crate::service::compliance::DEFAULT_LIST_LIMIT);
    match crate::service::compliance::list_for_fleet(
        &db,
        workspace_id,
        query.site_id,
        query.since,
        limit,
    )
    .await
    {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => map_err(e),
    }
}

#[derive(Debug, serde::Deserialize)]
struct FleetComplianceSummaryQuery {
    site_id: Option<Uuid>,
    since: Option<chrono::DateTime<chrono::Utc>>,
}

/// `GET /{wid}/fleet/compliance-summary?site_id=?&since=?` —
/// per-camera totals across the workspace (or one site). Drives the
/// Playback Grid view (replaces a per-site fan-out).
async fn get_fleet_compliance_summary(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    axum::extract::Query(query): axum::extract::Query<FleetComplianceSummaryQuery>,
) -> Response {
    match crate::service::compliance::summary_for_fleet(
        &db,
        workspace_id,
        query.site_id,
        query.since,
    )
    .await
    {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => map_err(e),
    }
}

/// `GET /{wid}/fleet/alerts?site_id=?&limit=?` — most-recent-first
/// camera-health transition feed. Backed by `audit_events` rows
/// written by the alerter, enriched with camera + site names so the
/// UI doesn't need to fan out per-row.
async fn get_recent_alerts(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    axum::extract::Query(query): axum::extract::Query<RecentAlertsQuery>,
) -> Response {
    let limit = query.limit.unwrap_or(20);
    match crate::service::alerts::recent_alerts(&db, workspace_id, query.site_id, limit).await {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => map_err(e),
    }
}

/// `POST /{wid}/fleet/test-alert` — fires a no-op alert through the
/// configured Slack channel so operators can validate the wiring
/// without waiting for a real degradation. Response body returns
/// `{delivered: bool}` — `false` means the workspace has no Slack
/// install or no default channel, which is treated as a normal
/// state rather than an error so the UI can show a "configure
/// Slack" CTA instead of a red error toast.
async fn post_test_alert(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
) -> Response {
    let payload = crate::service::alerts::AlertPayload {
        title: "Test alert from Oxy Edge".into(),
        detail: Some("If you see this in Slack, the wiring works.".into()),
        url: None,
    };
    match crate::service::alerts::notify(&db, workspace_id, &payload).await {
        Ok(delivered) => Json(serde_json::json!({ "delivered": delivered })).into_response(),
        Err(e) => map_err(e),
    }
}

// ── Rollout plans ───────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
struct CreateRolloutBody {
    target_image_tag: String,
    canary_cohort: String,
    #[serde(default = "default_canary_minutes")]
    canary_min_minutes: i32,
    #[serde(default = "default_failure_threshold_pct")]
    failure_threshold_pct: f32,
}

fn default_canary_minutes() -> i32 {
    1440
}
fn default_failure_threshold_pct() -> f32 {
    5.0
}

/// `POST /{wid}/fleet/rollouts` — create a canary rollout plan. The
/// supervisor (running in-process every 60s) picks the plan up on its
/// next tick, sets target_image_tag on every box in `canary_cohort`,
/// then auto-promotes to the rest of the workspace when the soak
/// window elapses without failures.
async fn create_rollout_plan(
    Extension(db): Extension<DatabaseConnection>,
    user: Option<axum::Extension<oxy_auth::AuthenticatedUser>>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    Json(body): Json<CreateRolloutBody>,
) -> Response {
    let input = crate::service::rollouts::CreatePlanInput {
        workspace_id,
        target_image_tag: body.target_image_tag,
        canary_cohort: body.canary_cohort,
        canary_min_minutes: body.canary_min_minutes,
        failure_threshold_pct: body.failure_threshold_pct,
        actor_user_id: user.map(|u| u.0.id),
    };
    match crate::service::rollouts::create_plan(&db, input).await {
        Ok(plan) => Json(plan).into_response(),
        Err(e) => map_err(e),
    }
}

async fn list_rollout_plans(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
) -> Response {
    match crate::service::rollouts::list_plans(&db, workspace_id).await {
        Ok(plans) => Json(plans).into_response(),
        Err(e) => map_err(e),
    }
}

#[derive(Debug, serde::Deserialize)]
struct AbortRolloutBody {
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct RolloutPath {
    workspace_id: Uuid,
    plan_id: Uuid,
}

async fn abort_rollout_plan(
    Extension(db): Extension<DatabaseConnection>,
    Path(RolloutPath {
        workspace_id,
        plan_id,
    }): Path<RolloutPath>,
    Json(body): Json<AbortRolloutBody>,
) -> Response {
    let reason = body.reason.as_deref().unwrap_or("operator aborted");
    match crate::service::rollouts::abort_plan(&db, workspace_id, plan_id, reason).await {
        Ok(plan) => Json(plan).into_response(),
        Err(e) => map_err(e),
    }
}

/// `GET /{wid}/fleet/rollouts/{plan_id}/convergence` — per-box
/// breakdown for the rollout detail page. Returns one row per edge
/// box in the workspace tagged with its current convergence status
/// (`converged` / `failed` / `pending` / `untargeted`) and an
/// `in_canary` flag so the UI can group canary rows on top.
async fn get_rollout_convergence(
    Extension(db): Extension<DatabaseConnection>,
    Path(RolloutPath {
        workspace_id,
        plan_id,
    }): Path<RolloutPath>,
) -> Response {
    match crate::service::rollouts::plan_convergence(&db, workspace_id, plan_id).await {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => map_err(e),
    }
}

/// `DELETE /{wid}/cameras/fleet` — remove a fleet member. The
/// body tells us which: pass `edge_box_id` to retire a real
/// box (revokes bearer + linked claim), pass `device_id` to
/// revoke a pending claim that hasn't bootstrapped yet, or
/// both. Same-shape body as the merged list row so the UI can
/// hand back whichever fields it has.
///
/// Idempotent: a second remove on an already-retired box
/// reports `edge_box_retired: false` (no state changed)
/// rather than 404 — operators sometimes click twice.
async fn delete_fleet_member(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    OptionalUserExtractor(user): OptionalUserExtractor,
    Json(body): Json<RemoveFleetMemberBody>,
) -> Response {
    let actor = user.as_ref().map(|u| u.id);
    match fleet::remove_member(&db, workspace_id, body.edge_box_id, body.device_id).await {
        Ok(out) => {
            audit::record(
                &db,
                workspace_id,
                actor,
                "device.remove",
                Some(out.target_id),
                serde_json::json!({
                    "edge_box_id": body.edge_box_id,
                    "device_id": body.device_id,
                    "edge_box_retired": out.edge_box_retired,
                    "claim_revoked": out.claim_revoked,
                }),
            )
            .await;
            Json(out).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `GET /{wid}/cameras/fleet` — merged "physical boxes" view.
/// Includes every edge_box in the workspace AND every pending
/// device claim that hasn't materialized an edge_box yet. The
/// UI uses this as the single source of truth for the Boxes
/// table (replacing the previously-split Edge Boxes + Fleet
/// tabs).
async fn list_fleet_members(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
) -> Response {
    match listing::list_fleet_members(&db, workspace_id).await {
        Ok(rows) => {
            let dtos: Vec<FleetMemberDto> = rows.into_iter().map(Into::into).collect();
            Json(dtos).into_response()
        }
        Err(e) => map_err(e),
    }
}

#[derive(serde::Deserialize)]
struct WorkspaceEdgeBoxPath {
    workspace_id: Uuid,
    box_id: Uuid,
}

async fn get_edge_box(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceEdgeBoxPath {
        workspace_id,
        box_id,
    }): Path<WorkspaceEdgeBoxPath>,
) -> Response {
    match listing::get_edge_box(&db, workspace_id, box_id).await {
        Ok(eb) => Json(EdgeBoxDto::from(eb)).into_response(),
        Err(e) => map_err(e),
    }
}

/// `PATCH /{wid}/cameras/edge-boxes/bulk/target` — applies one
/// `target_image_tag` + `held_until` to every box in `box_ids`
/// atomically. If any id belongs to another workspace OR doesn't
/// exist, the whole call rejects with 403 / 404 before touching
/// the DB. This avoids the half-rolled-out chain state that an
/// operator-as-installer fleet absolutely doesn't want.
///
/// Returns `{updated_count}`; equals `len(unique(box_ids))` on
/// success. Empty `box_ids` returns 0 with no error so the UI
/// can wire the action button without guarding against empties.
async fn patch_bulk_target(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    OptionalUserExtractor(user): OptionalUserExtractor,
    Json(body): Json<BulkPatchTargetBody>,
) -> Response {
    let box_ids = body.box_ids.clone();
    let target_tag = body.target_image_tag.clone();
    let held_until = body.held_until;
    match updates::bulk_set_target(
        &db,
        workspace_id,
        &body.box_ids,
        body.target_image_tag,
        body.held_until,
    )
    .await
    {
        Ok(updated_count) => {
            audit::record(
                &db,
                workspace_id,
                user.as_ref().map(|u| u.id),
                audit::ACTION_BULK_SET_TARGET,
                None,
                serde_json::json!({
                    "box_ids": box_ids,
                    "target_image_tag": target_tag,
                    "held_until": held_until,
                    "updated_count": updated_count,
                }),
            )
            .await;
            Json(BulkPatchTargetResponse { updated_count }).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// Query string for the cost endpoints. `range` controls the
/// rollup window; defaults to 24h so a plain `GET` returns the
/// most-actionable view ("what is it costing me right now?").
#[derive(Debug, serde::Deserialize)]
struct CostQuery {
    #[serde(default = "default_cost_range")]
    range: cost::CostRange,
}

fn default_cost_range() -> cost::CostRange {
    cost::CostRange::H24
}

/// `GET /{wid}/cameras/edge-boxes/{box_id}/cost?range=24h|7d|30d`
/// — per-box VLM cost rollup with model breakdown. See
/// [`cost::box_cost_for`] for the ownership chain and aggregation.
async fn get_edge_box_cost(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceEdgeBoxPath {
        workspace_id,
        box_id,
    }): Path<WorkspaceEdgeBoxPath>,
    Query(CostQuery { range }): Query<CostQuery>,
) -> Response {
    match cost::box_cost_for(&db, workspace_id, box_id, range).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => map_err(e),
    }
}

/// Query string for the per-box logs endpoint. Mirrors
/// [`logs::LogListFilter`] but with `since` typed as a string so
/// Axum's Query extractor can parse it consistently from a URL.
#[derive(Debug, serde::Deserialize)]
struct LogsQuery {
    #[serde(default)]
    min_severity: Option<String>,
    #[serde(default)]
    event_contains: Option<String>,
    #[serde(default)]
    since: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    limit: Option<u32>,
}

/// `GET /{wid}/cameras/edge-boxes/{box_id}/logs` — operator-facing
/// reader. Same workspace-ownership defense as the cost
/// endpoints: we re-resolve `edge_box → site → workspace` even
/// though the URL has both, because a bug in route assembly would
/// otherwise turn into a cross-workspace log leak.
async fn get_edge_box_logs(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceEdgeBoxPath {
        workspace_id,
        box_id,
    }): Path<WorkspaceEdgeBoxPath>,
    Query(q): Query<LogsQuery>,
) -> Response {
    // Resolve + ownership-check the edge box via the listing
    // service so we get the same "Forbidden vs NotFound" semantics
    // as other operator routes.
    match listing::get_edge_box(&db, workspace_id, box_id).await {
        Ok(_) => {}
        Err(e) => return map_err(e),
    }
    let filter = logs::LogListFilter {
        min_severity: q.min_severity,
        event_contains: q.event_contains,
        since: q.since,
        limit: q.limit,
    };
    match logs::list_for_box(workspace_id, box_id, filter).await {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => map_err(e),
    }
}

/// `GET /{wid}/cameras/cost?range=24h|7d|30d` — workspace-wide
/// rollup with per-box breakdown sorted heaviest-first.
async fn get_workspace_cost(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    Query(CostQuery { range }): Query<CostQuery>,
) -> Response {
    match cost::workspace_cost_for(&db, workspace_id, range).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => map_err(e),
    }
}

/// `GET /{wid}/cameras/budget` — month-to-date spend, projected
/// month-end, and the configured ceiling. Drives the Cameras
/// dashboard banner. Same workspace gate as the cost endpoints.
async fn get_budget_status(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
) -> Response {
    match budget::get_status(&db, workspace_id).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => map_err(e),
    }
}

/// Query string for the audit-events list endpoint.
#[derive(Debug, serde::Deserialize)]
struct AuditQuery {
    #[serde(default)]
    action_prefix: Option<String>,
    #[serde(default)]
    target_id: Option<Uuid>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
}

/// `GET /{wid}/cameras/audit-events` — most-recent-first list of
/// operator-action audit rows. Drives the Audit tab in the
/// Cameras settings UI.
async fn list_audit_events(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    Query(q): Query<AuditQuery>,
) -> Response {
    let filter = audit::ListFilter {
        action_prefix: q.action_prefix,
        target_id: q.target_id,
        limit: q.limit,
        offset: q.offset,
    };
    match audit::list_for_workspace(&db, workspace_id, filter).await {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => map_err(e),
    }
}

/// `GET /{wid}/cameras/health-summary` — most-recent health row
/// per camera, with a status badge (`ok` | `degraded` | `stale`
/// | `unknown`) the UI surfaces in the cameras table. Includes
/// cameras that have never reported (as `unknown`) so the
/// operator can distinguish "silent" from "configured."
async fn get_camera_health_summary(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
) -> Response {
    match camera_health::summarize_for_workspace(&db, workspace_id).await {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => map_err(e),
    }
}

/// Path for `GET /{wid}/cameras/clips/{report_id}/url`.
#[derive(Debug, serde::Deserialize)]
struct WorkspaceReportPath {
    workspace_id: Uuid,
    report_id: Uuid,
}

/// Query carries the bucket-relative key. The route layer doesn't
/// know which camera the report came from without an extra
/// lookup, and the worker already stamped `evidence_s3_key`
/// on the report — so the UI just passes it through. We
/// re-verify the workspace prefix server-side regardless.
#[derive(Debug, serde::Deserialize)]
struct ClipUrlQuery {
    key: String,
}

/// `GET /{wid}/cameras/clips/{report_id}/url?key=...` — mint a
/// presigned GET URL the operator's browser can use to download
/// the archived clip. Defense-in-depth: `clips::mint_get_url`
/// re-checks that the supplied `key` starts with the
/// workspace's prefix, so a UI bug or hostile caller can't
/// fetch another tenant's clip via this path. `report_id` is
/// in the URL for audit-trace symmetry with the rest of the
/// camera surface; the actual S3 lookup is keyed off `key`.
async fn get_clip_playback_url(
    Path(WorkspaceReportPath {
        workspace_id,
        report_id: _report_id,
    }): Path<WorkspaceReportPath>,
    Query(q): Query<ClipUrlQuery>,
) -> Response {
    match clips::mint_get_url(workspace_id, &q.key).await {
        Ok(out) => Json(out).into_response(),
        Err(e) => map_err(e),
    }
}

/// `PATCH /{wid}/cameras/budget` — set or clear the monthly VLM
/// budget. `null` clears (banner hides); a numeric value sets.
/// Negative values rejected at the service layer.
async fn patch_budget(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    Json(body): Json<budget::BudgetPayload>,
) -> Response {
    match budget::set_budget(&db, workspace_id, body.monthly_budget_micros).await {
        Ok(v) => Json(serde_json::json!({ "monthly_budget_micros": v })).into_response(),
        Err(e) => map_err(e),
    }
}

/// `PATCH /{wid}/cameras/edge-boxes/{box_id}/target` — operator
/// sets the target image tag for this box. The device's update
/// agent picks it up on its next poll of `/control/boxes/{id}/target`
/// and pulls + restarts. Body fields are independently nullable —
/// passing only `target_image_tag` doesn't clobber an existing
/// `held_until`, and vice versa.
async fn patch_edge_box_target(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceEdgeBoxPath {
        workspace_id,
        box_id,
    }): Path<WorkspaceEdgeBoxPath>,
    OptionalUserExtractor(user): OptionalUserExtractor,
    Json(body): Json<PatchEdgeBoxTargetBody>,
) -> Response {
    let target_tag = body.target_image_tag.clone();
    let held_until = body.held_until;
    match updates::set_target(
        &db,
        workspace_id,
        box_id,
        body.target_image_tag,
        body.held_until,
    )
    .await
    {
        Ok(eb) => {
            audit::record(
                &db,
                workspace_id,
                user.as_ref().map(|u| u.id),
                audit::ACTION_SET_TARGET,
                Some(box_id),
                serde_json::json!({
                    "target_image_tag": target_tag,
                    "held_until": held_until,
                }),
            )
            .await;
            Json(EdgeBoxDto::from(eb)).into_response()
        }
        Err(e) => map_err(e),
    }
}

// ── Cohort tagging ──────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
struct PatchEdgeBoxCohortBody {
    cohort: String,
}

/// `PATCH /{wid}/cameras/edge-boxes/{box_id}/cohort` — operator sets
/// the cohort label used to scope canary rollouts. Audit-logged so
/// "who moved my boxes between cohorts mid-rollout" is answerable.
async fn patch_edge_box_cohort(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceEdgeBoxPath {
        workspace_id,
        box_id,
    }): Path<WorkspaceEdgeBoxPath>,
    OptionalUserExtractor(user): OptionalUserExtractor,
    Json(body): Json<PatchEdgeBoxCohortBody>,
) -> Response {
    let cohort = body.cohort.clone();
    match updates::set_cohort(&db, workspace_id, box_id, &body.cohort).await {
        Ok(eb) => {
            audit::record(
                &db,
                workspace_id,
                user.as_ref().map(|u| u.id),
                audit::ACTION_SET_COHORT,
                Some(box_id),
                serde_json::json!({ "cohort": cohort }),
            )
            .await;
            Json(EdgeBoxDto::from(eb)).into_response()
        }
        Err(e) => map_err(e),
    }
}

// ── Cameras (list / detail) ─────────────────────────────────────────────────

async fn list_cameras(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
) -> Response {
    match listing::list_cameras(&db, workspace_id).await {
        Ok(rows) => {
            let dtos: Vec<CameraSummaryDto> = rows.into_iter().map(Into::into).collect();
            Json(dtos).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `POST /{wid}/cameras` — operator adds a camera manually by
/// pasting its RTSP URL. The auto-claim path in
/// `service::config::fetch_cameras_for_edge` binds the camera to
/// the first edge box at the same site that polls config (same
/// mechanism the UniFi importer relies on). See
/// `service::provisioning::create_camera` for validation rules.
async fn post_camera(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    Json(body): Json<CreateCameraBody>,
) -> Response {
    let input = provisioning::CreateCameraInput {
        site_id: body.site_id,
        name: body.name,
        rtsp_url: body.rtsp_url,
        substream_url: body.substream_url,
        credentials_ref: body.credentials_ref,
    };
    match provisioning::create_camera(&db, workspace_id, input).await {
        Ok(cam) => Json(CameraDto::from(cam)).into_response(),
        Err(e) => map_err(e),
    }
}

async fn get_camera(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceCameraPath {
        workspace_id,
        cam_id,
    }): Path<WorkspaceCameraPath>,
) -> Response {
    match listing::get_camera(&db, workspace_id, cam_id).await {
        Ok(cam) => Json(CameraDto::from(cam)).into_response(),
        Err(e) => map_err(e),
    }
}

/// HLS pass-through (playlist + segments). The `tail` segment match
/// arrives as a single string (no leading slash) thanks to axum's
/// `{*name}` wildcard syntax. Content-type comes from the upstream
/// MediaMTX response so `.m3u8`, `.ts`, and `.m4s` all flow through
/// with the right MIME.
#[derive(serde::Deserialize)]
struct HlsPath {
    workspace_id: Uuid,
    cam_id: Uuid,
    tail: String,
}

async fn get_hls(
    Extension(db): Extension<DatabaseConnection>,
    Path(HlsPath {
        workspace_id,
        cam_id,
        tail,
    }): Path<HlsPath>,
) -> Response {
    match preview::hls_fetch(&db, workspace_id, cam_id, &tail).await {
        Ok(asset) => {
            let mut resp = asset.bytes.into_response();
            if let Ok(ct) = HeaderValue::from_str(&asset.content_type) {
                resp.headers_mut().insert(CONTENT_TYPE, ct);
            }
            // Playlists must NEVER be cached (they update every
            // segment duration). Segments are addressed by filename
            // and immutable in practice, but a no-store policy here
            // keeps the matrix simple and lets MediaMTX trim segments
            // without dealing with stale CDN holds.
            resp.headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
            resp
        }
        Err(e) => map_err(e),
    }
}

/// JPEG snapshot proxy. See `service::preview::snapshot` for the flow.
/// Browser caches are explicitly disabled — the UI polls every few
/// seconds, and a stale CDN snapshot would defeat the point.
async fn get_snapshot(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceCameraPath {
        workspace_id,
        cam_id,
    }): Path<WorkspaceCameraPath>,
) -> Response {
    match preview::snapshot(&db, workspace_id, cam_id).await {
        Ok(snap) => {
            let mut resp = snap.bytes.into_response();
            // The bytes default to `application/octet-stream`; rewrite
            // to whatever the edge sent (image/jpeg in practice).
            if let Ok(ct) = HeaderValue::from_str(&snap.content_type) {
                resp.headers_mut().insert(CONTENT_TYPE, ct);
            }
            resp.headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
            resp
        }
        Err(e) => map_err(e),
    }
}

/// Query string for the clip playback endpoint. `start` is forwarded
/// as-is to MediaMTX (it accepts ISO-8601 and a few aliases); `duration`
/// is clamped in the service layer to a sensible upper bound so an
/// operator can't accidentally fan out a 24-hour download.
#[derive(serde::Deserialize)]
struct RecordingQuery {
    start: String,
    duration: f64,
}

/// Recorded-clip proxy. Sits on top of MediaMTX's playback server
/// (`:9996`, see `mediamtx.yml`) — see `service::preview::recording_clip`
/// for the workspace/edge-box guard chain.
///
/// Response is the fMP4 bytes; `Cache-Control: no-store` keeps stale
/// clips out of intermediate caches since the underlying retention
/// window (`recordDeleteAfter`) is short.
async fn get_recording_clip(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceCameraPath {
        workspace_id,
        cam_id,
    }): Path<WorkspaceCameraPath>,
    Query(RecordingQuery { start, duration }): Query<RecordingQuery>,
) -> Response {
    match preview::recording_clip(&db, workspace_id, cam_id, &start, duration).await {
        Ok(clip) => {
            let mut resp = clip.bytes.into_response();
            if let Ok(ct) = HeaderValue::from_str(&clip.content_type) {
                resp.headers_mut().insert(CONTENT_TYPE, ct);
            }
            resp.headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
            resp
        }
        Err(e) => map_err(e),
    }
}

/// `GET /:wid/cameras/:cam_id/preview/recording-segments` — JSON list
/// of `{ start, duration_seconds }` for every contiguous block of
/// recorded footage MTX still has on disk for this camera. The UI
/// uses this to shade the scrubber and snap free-scrub clicks onto
/// real footage instead of erroring on gaps.
async fn list_recording_segments(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceCameraPath {
        workspace_id,
        cam_id,
    }): Path<WorkspaceCameraPath>,
) -> Response {
    match preview::list_recordings(&db, workspace_id, cam_id).await {
        Ok(segs) => Json(segs).into_response(),
        Err(e) => map_err(e),
    }
}

/// Env var name for the shared HMAC secret coturn knows. Same
/// secret on both sides; coturn's `--static-auth-secret=$X` flag
/// derives credentials from this. Empty value → 503 Service
/// Unavailable on the WebRTC session endpoint (we'd rather refuse
/// than hand out cryptographically meaningless creds).
const TURN_AUTH_SECRET_ENV: &str = "OXY_CAMERAS_TURN_AUTH_SECRET";

/// WebRTC session helper — mints the per-session URL + ephemeral
/// TURN credential the browser uses to dial the box's MediaMTX
/// directly via Tailscale Funnel.
///
/// POST shape: empty body. Returns JSON `WebrtcSessionDto`. The
/// frontend then constructs `new RTCPeerConnection({ iceServers })`
/// with the returned servers and POSTs its SDP offer at
/// `whep_url`.
async fn post_webrtc_session(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceCameraPath {
        workspace_id,
        cam_id,
    }): Path<WorkspaceCameraPath>,
) -> Response {
    let secret = std::env::var(TURN_AUTH_SECRET_ENV).unwrap_or_default();
    if secret.is_empty() {
        return map_err(crate::service::ServiceError::Unavailable(
            "WebRTC session unavailable: OXY_CAMERAS_TURN_AUTH_SECRET is not set",
        ));
    }
    match webrtc::build_session(&db, workspace_id, cam_id, secret.as_bytes()).await {
        Ok(s) => Json(WebrtcSessionDto::from(s)).into_response(),
        Err(e) => map_err(e),
    }
}

/// Query string for [`list_compliance_reports`]. `since` is parsed as
/// RFC 3339 (`2026-05-28T00:00:00Z`); omit to get the most recent
/// `limit` rows regardless of age.
#[derive(serde::Deserialize)]
struct ComplianceListQuery {
    since: Option<String>,
    limit: Option<u32>,
}

/// List compliance reports for one camera. Reads from Airhouse via
/// the broker; service layer enforces workspace ownership on the
/// camera before any pgwire traffic.
async fn list_compliance_reports(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceCameraPath {
        workspace_id,
        cam_id,
    }): Path<WorkspaceCameraPath>,
    Query(ComplianceListQuery { since, limit }): Query<ComplianceListQuery>,
) -> Response {
    let since_dt = match since {
        None => None,
        Some(s) => match chrono::DateTime::parse_from_rfc3339(&s) {
            Ok(dt) => Some(dt.with_timezone(&chrono::Utc)),
            Err(e) => {
                return map_err(crate::service::ServiceError::InvalidInput(format!(
                    "invalid `since` (expected RFC 3339): {e}"
                )));
            }
        },
    };
    let limit = limit.unwrap_or(compliance::DEFAULT_LIST_LIMIT);
    match compliance::list_for_camera(&db, workspace_id, cam_id, since_dt, limit).await {
        Ok(rows) => {
            let dtos: Vec<ComplianceReportDto> = rows.into_iter().map(Into::into).collect();
            Json(dtos).into_response()
        }
        Err(e) => map_err(e),
    }
}

// ── Edge box registration (REMOVED) ────────────────────────────────────────
//
// The legacy `POST /{wid}/cameras/edge-boxes` operator endpoint
// was the bearer-only onboarding path: operator pre-minted
// `(edge_box_id, bearer)` and pasted them into the box's env.
// Removed in favor of `POST /{wid}/fleet/prepared-devices`
// ("Add device" wizard) which mints a per-device identity +
// pre-claim atomically.
//
// The service-layer `registration::register_edge_box` is still
// in use — `service::fleet::bootstrap_device` calls it when a
// pre-claimed device first announces. The DB shape and the test
// helper are unchanged; only the public HTTP surface and UI are
// gone.

// ── Camera zones PATCH ──────────────────────────────────────────────────────

/// URL path carries `workspace_id` and `cam_id` together. Axum
/// extractors `Path<WorkspacePath>` and `Path<CameraIdPath>` can't both
/// be used on the same handler — they fight over the same `Path` slot —
/// so we extract both via one combined type below.
#[derive(serde::Deserialize)]
struct WorkspaceCameraPath {
    workspace_id: Uuid,
    cam_id: Uuid,
}

async fn patch_zones(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceCameraPath {
        workspace_id,
        cam_id,
    }): Path<WorkspaceCameraPath>,
    Json(body): Json<PatchZonesBody>,
) -> Response {
    match config::patch_camera_zones(&db, workspace_id, cam_id, body.zones_json, body.lines_json)
        .await
    {
        Ok(_cam) => axum::http::StatusCode::NO_CONTENT.into_response(),
        Err(e) => map_err(e),
    }
}

/// `PUT /{wid}/cameras/{cam_id}/proposed-geometry` — write a
/// pending proposal. The live `zones_json` / `lines_json` is
/// untouched; the worker keeps operating against it until the
/// operator approves. Same body shape as `PATCH .../zones`
/// (PatchZonesBody) because the wire contract is the same set
/// of fields — operators or upstream callers shouldn't have to
/// learn a second shape just to draft instead of apply.
async fn put_proposed_geometry(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceCameraPath {
        workspace_id,
        cam_id,
    }): Path<WorkspaceCameraPath>,
    Json(body): Json<PatchZonesBody>,
) -> Response {
    match config::propose_geometry(&db, workspace_id, cam_id, body.zones_json, body.lines_json)
        .await
    {
        Ok(cam) => Json(CameraDto::from(cam)).into_response(),
        Err(e) => map_err(e),
    }
}

/// `POST /{wid}/cameras/{cam_id}/proposed-geometry/approve` —
/// copy `proposed_*_json` into the live columns + clear the
/// proposal. Returns the updated camera so the UI doesn't have
/// to re-fetch.
async fn post_approve_proposed_geometry(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceCameraPath {
        workspace_id,
        cam_id,
    }): Path<WorkspaceCameraPath>,
) -> Response {
    match config::approve_proposed_geometry(&db, workspace_id, cam_id).await {
        Ok(cam) => Json(CameraDto::from(cam)).into_response(),
        Err(e) => map_err(e),
    }
}

/// `POST /{wid}/cameras/{cam_id}/proposed-geometry/reject` —
/// drop the pending proposal without touching live geometry.
async fn post_reject_proposed_geometry(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceCameraPath {
        workspace_id,
        cam_id,
    }): Path<WorkspaceCameraPath>,
) -> Response {
    match config::reject_proposed_geometry(&db, workspace_id, cam_id).await {
        Ok(cam) => Json(CameraDto::from(cam)).into_response(),
        Err(e) => map_err(e),
    }
}

/// Flip a camera's analytics-consent flag. The edge worker picks this
/// up on its next config poll (every ~30s) and skips inference / event
/// emission for cameras with consent off — preview still flows.
async fn patch_consent(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceCameraPath {
        workspace_id,
        cam_id,
    }): Path<WorkspaceCameraPath>,
    Json(body): Json<PatchConsentBody>,
) -> Response {
    match config::set_analytics_consent(&db, workspace_id, cam_id, body.analytics_consent).await {
        Ok(_cam) => axum::http::StatusCode::NO_CONTENT.into_response(),
        Err(e) => map_err(e),
    }
}

/// Set the per-camera role (`kitchen` | `prep` | `dining` | `other`
/// or null) used by the edge worker's prompt registry. See
/// [`config::set_role`] for the validation rules.
async fn patch_role(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceCameraPath {
        workspace_id,
        cam_id,
    }): Path<WorkspaceCameraPath>,
    Json(body): Json<PatchRoleBody>,
) -> Response {
    match config::set_role(&db, workspace_id, cam_id, body.role).await {
        Ok(_cam) => axum::http::StatusCode::NO_CONTENT.into_response(),
        Err(e) => map_err(e),
    }
}

// ── Domain pack (operator) ─────────────────────────────────────────────────

/// `GET /{wid}/camera-packs/active` — fetch the active pack for this
/// workspace. Auto-seeds the restaurant starter on first read (see
/// [`packs::get_active`]).
///
/// The UI uses this for two surfaces:
///   1. The role picker (reads `body.roles[]` to render options).
///   2. The pack editor card (renders the full body for editing).
async fn get_active_pack(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
) -> Response {
    match packs::get_active(&db, workspace_id).await {
        Ok(row) => Json(DomainPackDto::from(row)).into_response(),
        Err(e) => map_err(e),
    }
}

/// `PUT /{wid}/camera-packs/{pack_key}/body` — replace a pack's
/// body. Renders every role's prompt against a dummy context
/// before persisting (see [`packs::validate::check_pack_renders`]);
/// a typo or undefined variable returns 400 with the offending
/// role + minijinja error.
///
/// Flips `locally_modified = true` so the future starter-update
/// diff modal can offer "discard local changes" alongside Oxy's
/// upstream updates.
async fn put_pack_body(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePackPath {
        workspace_id,
        pack_key,
    }): Path<WorkspacePackPath>,
    Json(body): Json<PutPackBody>,
) -> Response {
    let parsed = match serde_json::from_value::<packs::body::PackBody>(body.body) {
        Ok(b) => b,
        Err(e) => {
            return map_err(crate::service::ServiceError::InvalidInput(format!(
                "pack body did not parse: {e}"
            )));
        }
    };
    match packs::update_body(&db, workspace_id, &pack_key, parsed).await {
        Ok(row) => Json(DomainPackDto::from(row)).into_response(),
        Err(e) => map_err(e),
    }
}

#[derive(serde::Deserialize)]
struct WorkspacePackPath {
    workspace_id: Uuid,
    pack_key: String,
}

/// `GET /{wid}/camera-packs` — list all packs in this workspace,
/// active + inactive. Used by the switch-pack dialog to render
/// the operator's pack inventory. Body is intentionally omitted —
/// fetch via `GET /camera-packs/active` (or, in a future
/// iteration, `GET /camera-packs/{key}`) when an editor opens.
async fn list_packs(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
) -> Response {
    match packs::list(&db, workspace_id).await {
        Ok(rows) => {
            let dtos: Vec<DomainPackSummaryDto> =
                rows.into_iter().map(DomainPackSummaryDto::from).collect();
            Json(dtos).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `GET /{wid}/camera-packs/starters` — list Oxy's curated starter
/// library. The UI uses this for the fork picker; the workspace
/// scope is decorative here (the library is global) but keeping
/// every camera-pack route under `/{wid}/camera-packs/` keeps the
/// surface area uniform.
async fn list_starters(Path(WorkspacePath { workspace_id: _ }): Path<WorkspacePath>) -> Response {
    let dtos: Vec<StarterPackSummaryDto> = packs::starter::list()
        .iter()
        .map(|s| StarterPackSummaryDto {
            key: s.key.to_string(),
            name: s.name.to_string(),
            description: s.description.to_string(),
            version: s.version.to_string(),
        })
        .collect();
    Json(dtos).into_response()
}

/// `POST /{wid}/camera-packs` — fork a starter into this workspace.
/// Body: `{ "starter_key": "retail_lp" }`. The new pack becomes
/// the active one (transactional in [`packs::create_from_starter`]).
///
/// Conflict on `(workspace_id, pack_key)` — i.e. the operator
/// already forked this starter — surfaces as a SeaORM unique-
/// constraint error mapped to 502 today; UI should pre-check via
/// the list endpoint and offer "activate the existing one"
/// instead. v1: simpler to keep the surface narrow.
async fn post_pack_fork(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    Json(body): Json<PostPackForkBody>,
) -> Response {
    match packs::create_from_starter(&db, workspace_id, &body.starter_key).await {
        Ok(row) => Json(DomainPackDto::from(row)).into_response(),
        Err(e) => map_err(e),
    }
}

/// `GET /{wid}/camera-packs/starter-updates` — list every pack in
/// this workspace that has an upstream update available. Returns
/// an empty list when all packs are at the latest starter version.
/// See [`packs::updates::pending_updates`] for the diff logic.
async fn list_starter_updates(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
) -> Response {
    match packs::updates::pending_updates(&db, workspace_id).await {
        Ok(updates) => Json(updates).into_response(),
        Err(e) => map_err(e),
    }
}

/// `POST /{wid}/camera-packs/{pack_key}/apply-update` — commit the
/// operator's choices from the diff modal. Body:
/// `{ "actions": { "role_key": "take_upstream" | "keep_mine", ... } }`.
/// Missing roles default to `keep_mine`.
///
/// Render-validates the merged body before persisting; a bad
/// merge (e.g. operator took an upstream role whose template
/// references a variable they kept removed) returns 400.
async fn post_apply_update(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePackPath {
        workspace_id,
        pack_key,
    }): Path<WorkspacePackPath>,
    Json(body): Json<ApplyUpdateBody>,
) -> Response {
    match packs::updates::apply_update(&db, workspace_id, &pack_key, body.actions).await {
        Ok(row) => Json(DomainPackDto::from(row)).into_response(),
        Err(e) => map_err(e),
    }
}

/// `POST /{wid}/camera-packs/{pack_key}/activate` — switch which
/// pack is active. Atomically deactivates the previous active
/// pack inside [`packs::set_active`]'s transaction so the
/// partial unique index never sees two active rows in the same
/// workspace.
async fn post_pack_activate(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePackPath {
        workspace_id,
        pack_key,
    }): Path<WorkspacePackPath>,
) -> Response {
    match packs::set_active(&db, workspace_id, &pack_key).await {
        Ok(row) => Json(DomainPackDto::from(row)).into_response(),
        Err(e) => map_err(e),
    }
}

// ── Fleet claims ────────────────────────────────────────────────────────────

// `POST /{wid}/fleet/claims` (claim by code) removed. It existed
// to consume the `claim_code` printed on factory-burned hardware;
// the Add device wizard creates the pending_claim atomically so
// there's no need for a separate "operator types a code" surface.
// `service::fleet::claim_device` is gone too — `prepare_device`
// writes the pending claim directly. See the IoT cleanup commit
// for the audit trail.

/// `POST /{wid}/fleet/claims/bind-existing` — retroactively
/// attach a known `device_id` to an existing `edge_box_id`. Used
/// for boxes that were registered through the bearer-only path
/// before the IoT identity layer existed. The device must
/// already be factory-enrolled (we won't auto-enroll from the
/// operator path — see service-layer doc for the rationale).
async fn post_bind_existing(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    OptionalUserExtractor(user): OptionalUserExtractor,
    Json(body): Json<BindExistingBody>,
) -> Response {
    let actor = user.as_ref().map(|u| u.id);
    let device_id = body.device_id;
    let edge_box_id = body.edge_box_id;
    match fleet::bind_existing(&db, workspace_id, body.device_id, body.edge_box_id, actor).await {
        Ok(out) => {
            audit::record(
                &db,
                workspace_id,
                actor,
                audit::ACTION_BIND_EXISTING,
                Some(device_id),
                serde_json::json!({
                    "claim_id": out.claim_id,
                    "edge_box_id": edge_box_id,
                }),
            )
            .await;
            Json(ClaimDto {
                claim_id: out.claim_id,
                device_id,
                status: "claimed".into(),
            })
            .into_response()
        }
        Err(e) => map_err(e),
    }
}

/// Salt used to derive deterministic `claim_code`s from
/// `device_id`. Mirrors the public IoT route's choice — same
/// default for dev so a freshly-cloned repo works end-to-end,
/// overridable via env for any real deployment.
const CLAIM_SALT_ENV: &str = "OXY_FLEET_CLAIM_SALT";
const DEFAULT_CLAIM_SALT: &str = "oxy-dev-claim-salt-do-not-use-in-prod";

/// `POST /{wid}/fleet/prepared-devices` — "Add device" wizard
/// (Option 3). Mints a per-install device identity + pre-creates
/// the pending claim atomically, returns a copy-pasteable shell
/// snippet the operator runs on the new edge box.
///
/// **Why this exists instead of a long-lived shared install
/// secret.** Industry standard (Tailscale pre-auth, kubeadm
/// bootstrap tokens, AWS IoT JITP) is per-install credentials.
/// A compromised installer machine can only impersonate the one
/// device whose secret is on it; the workspace's other devices
/// are unaffected.
///
/// **Security model.** `device_secret_b64` is shown once in the
/// response and never persisted in a readable form on the
/// server (only the `device_registry` row, which is sensitive
/// per the design doc but not retrievable through any API). The
/// operator copies the install snippet directly into the new
/// box's SSH session.
async fn post_prepare_device(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    OptionalUserExtractor(user): OptionalUserExtractor,
    Json(body): Json<PrepareDeviceBody>,
) -> Response {
    use base64::Engine;

    let salt = std::env::var(CLAIM_SALT_ENV).unwrap_or_else(|_| DEFAULT_CLAIM_SALT.to_string());
    let actor = user.as_ref().map(|u| u.id);

    match fleet::prepare_device(
        &db,
        salt.as_bytes(),
        workspace_id,
        body.site_id,
        body.hardware_label.as_deref(),
        actor,
    )
    .await
    {
        Ok(out) => {
            let secret_b64 =
                base64::engine::general_purpose::STANDARD_NO_PAD.encode(&out.device_secret);
            let install_snippet = render_install_snippet(
                out.device_id,
                &secret_b64,
                &out.sku,
                &out.hardware_revision,
            );
            let install_command = render_install_command(
                out.device_id,
                &secret_b64,
                &out.hardware_revision,
                body.ts_authkey.as_deref(),
            );
            audit::record(
                &db,
                workspace_id,
                actor,
                "device.prepare",
                Some(out.device_id),
                serde_json::json!({
                    "site_id": body.site_id,
                    "claim_id": out.claim_id,
                    "hardware_revision": out.hardware_revision,
                }),
            )
            .await;
            Json(PrepareDeviceResponse {
                device_id: out.device_id,
                device_secret_b64: secret_b64,
                claim_code: out.claim_code,
                claim_id: out.claim_id,
                sku: out.sku,
                hardware_revision: out.hardware_revision,
                install_snippet,
                install_command,
            })
            .into_response()
        }
        Err(e) => map_err(e),
    }
}

/// URL operators get the `install-edge.sh` script from. Set
/// per-deployment so a corporate / offline install can point at
/// an internal mirror; defaults to the public repo for the
/// reference deployment. The wizard renders this verbatim into
/// the `curl | bash` command — keep it stable across releases.
const INSTALL_SCRIPT_URL_ENV: &str = "OXY_EDGE_INSTALL_URL";
const DEFAULT_INSTALL_SCRIPT_URL: &str =
    "https://raw.githubusercontent.com/oxy-hq/oxy/main/video-poc/edge/install-edge.sh";

/// `OXY_PUBLIC_URL` is the operator-facing base URL of the Oxy
/// server itself — what the edge box should call back to. We
/// pass it through to the installer as `--oxy-url`. Defaults
/// to a placeholder that surfaces a clear "you need to set
/// this" error on the box rather than silently calling
/// localhost.
const PUBLIC_URL_ENV: &str = "OXY_PUBLIC_URL";

/// Build the one-line `curl … | sudo bash -s -- …` command.
/// Preferred install path; the JSON snippet stays in the
/// wizard as a fallback for boxes that can't reach the
/// install script (airgapped, dev laptop, etc.).
/// POSIX single-quote-escape: wrap in single quotes, replacing any
/// literal `'` with `'\''` (close-quote, escaped quote, reopen-quote).
/// Required for any operator-supplied string we paste into a shell
/// snippet — `Jim's Box` would otherwise terminate the quoting and
/// run the tail unquoted.
fn sh_squote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

fn render_install_command(
    device_id: Uuid,
    device_secret_b64: &str,
    hardware_label: &str,
    ts_authkey: Option<&str>,
) -> String {
    let install_url = std::env::var(INSTALL_SCRIPT_URL_ENV)
        .unwrap_or_else(|_| DEFAULT_INSTALL_SCRIPT_URL.to_string());
    let oxy_url = std::env::var(PUBLIC_URL_ENV).unwrap_or_else(|_| "<your-oxy-url>".to_string());
    // `-s --` separates bash flags from script args.
    //
    // Hardware label and Tailscale auth-key go through `sh_squote`:
    // operator-typed labels can contain `'` ("Jim's Box"), and even
    // auth-key formats we don't control today could grow shell
    // metachars tomorrow. Naked `'…'` would let `'` close the quoted
    // region and run the tail unquoted.
    let ts_flag = match ts_authkey.map(str::trim).filter(|k| !k.is_empty()) {
        Some(key) => format!(" --ts-authkey {}", sh_squote(key)),
        None => String::new(),
    };
    // WebRTC TURN shared secret. When set on this Oxy, we hand the same
    // value to the install snippet so the box's coturn HMAC-verifies
    // against the same secret Oxy mints credentials with — single source
    // of truth, no copy-paste between two systems. When unset, fall back
    // to `--no-webrtc` so the install brings up the HLS-only stack
    // instead of failing on compose's `${TURN_AUTH_SECRET:?...}`.
    let webrtc_flag = match std::env::var("OXY_CAMERAS_TURN_AUTH_SECRET") {
        Ok(secret) if !secret.trim().is_empty() => {
            format!(" --turn-auth-secret {}", sh_squote(secret.trim()))
        }
        _ => " --no-webrtc".to_string(),
    };
    format!(
        "curl -sSL {install_url} | sudo bash -s -- \
--device-id {device_id} \
--device-secret {device_secret_b64} \
--oxy-url {oxy_url} \
--hardware-label {}{ts_flag}{webrtc_flag}",
        sh_squote(hardware_label)
    )
}

/// Build the shell snippet shown in the wizard's "Run this on
/// the new edge box" step. The JSON payload exactly matches
/// `worker/identity.py::DeviceIdentity.save()` so the worker
/// reads it on first boot without conversion.
///
/// Layout choices:
/// - JSON is pretty-printed (one field per line) so the snippet
///   renders cleanly in the modal's monospace block; one long
///   line would force horizontal scroll on smaller viewports.
/// - `chmod 600` runs immediately after the write so the
///   secret-readable window stays milliseconds, even on a
///   system with a permissive umask.
/// - The docker-compose stack already mounts `/var/lib/oxy` →
///   `/var/lib/oxy:ro` into the worker container (see
///   `video-poc/docker-compose.yml`), so the host-side write
///   is what the worker reads on its next boot — no
///   compose edits required.
fn render_install_snippet(
    device_id: Uuid,
    device_secret_b64: &str,
    sku: &str,
    hardware_revision: &str,
) -> String {
    format!(
        "sudo mkdir -p /var/lib/oxy\n\
sudo tee /var/lib/oxy/device.json >/dev/null <<'EOF'\n\
{{\n  \"device_id\": \"{device_id}\",\n  \"device_secret_b64\": \"{device_secret_b64}\",\n  \"sku\": \"{sku}\",\n  \"hardware_revision\": \"{hardware_revision}\"\n}}\n\
EOF\n\
sudo chmod 600 /var/lib/oxy/device.json"
    )
}

/// `GET /{wid}/fleet/devices` — operator-facing list of devices
/// bound to this workspace (claimed + pending). Drives the Fleet
/// tab in the Cameras settings UI. Revoked rows are excluded —
/// they belong in an audit log we haven't surfaced yet.
async fn list_fleet_devices(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
) -> Response {
    match fleet::list_for_workspace(&db, workspace_id).await {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => map_err(e),
    }
}

// ── UniFi onboarding ────────────────────────────────────────────────────────

/// Preview is read-only on UniFi and writes nothing to Oxy state, but
/// we still scope it to the workspace so the api_key surface follows
/// the same auth as any other write. (A user who's only in workspace A
/// shouldn't get to validate workspace B's UniFi credentials.)
async fn unifi_preview(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    body: Option<Json<UnifiPreviewBody>>,
) -> Response {
    let body_key = body.and_then(|Json(b)| b.api_key);
    let api_key =
        match crate::service::unifi_credentials::resolve_api_key(&db, workspace_id, body_key).await
        {
            Ok(k) => k,
            Err(e) => return map_err(e),
        };
    let client = match UnifiClient::new(api_key) {
        Ok(c) => c,
        Err(_) => {
            return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    match onboarding::preview(&client).await {
        Ok(p) => Json(p).into_response(),
        Err(e) => map_err(e),
    }
}

async fn unifi_import(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    Json(body): Json<UnifiImportBody>,
) -> Response {
    let api_key =
        match crate::service::unifi_credentials::resolve_api_key(&db, workspace_id, body.api_key)
            .await
        {
            Ok(k) => k,
            Err(e) => return map_err(e),
        };
    let client = match UnifiClient::new(api_key) {
        Ok(c) => c,
        Err(_) => {
            return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let input = onboarding::ImportInput {
        workspace_id,
        console_ids: body.console_ids,
        site_filter: body.site_filter,
    };
    match onboarding::import(&db, &client, input).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_err(e),
    }
}

// ── External API surface ────────────────────────────────────────────────────

/// Live-stream subtree for the EXTERNAL API surface: registry list, WHEP
/// signaling, HLS proxy, compliance + alert reads. Reuses the operator
/// handlers above where the full DTO is safe to expose.
pub fn external_stream_routes<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/registry", get(list_stream_registry))
        .route("/compliance-summary", get(get_fleet_compliance_summary))
        .route("/alerts", get(get_recent_alerts))
        .route("/{cam_id}/webrtc-session", post(post_webrtc_session))
        .route("/{cam_id}/hls/{*tail}", get(get_hls))
        .route(
            "/{cam_id}/compliance-reports",
            get(list_stream_compliance_reports),
        )
}

/// Clip-playback URL for the EXTERNAL API surface. Mounted by the app crate
/// at `/{workspace_id}/cameras/clips/{report_id}/url` behind API-key auth, so
/// customer apps (not just the operator UI) can play archived evidence /
/// compliance clips. Reuses [`get_clip_playback_url`] — `clips::mint_get_url`
/// re-checks the `key` is under the workspace's prefix, so an API-key holder
/// can only mint URLs for its own tenant's clips.
pub fn external_clip_routes<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/cameras/clips/{report_id}/url", get(get_clip_playback_url))
}

/// Identity + join-key fields only — not [`CameraSummaryDto`], which would
/// leak `rtsp_url` / `credentials_ref` to API-key holders.
#[derive(serde::Serialize)]
struct StreamRegistryEntryDto {
    id: Uuid,
    name: String,
    site_name: String,
    protect_camera_id: Option<String>,
    mac_address: Option<String>,
    online: Option<bool>,
    edge_box_status: Option<String>,
}

/// Verdict + evidence window only — drops `report_text`, `frame_uri`,
/// `evidence_s3_key`, token counts, and the YOLO bbox envelope.
#[derive(serde::Serialize)]
struct StreamComplianceReportDto {
    report_id: String,
    camera_id: String,
    segment_start: chrono::DateTime<chrono::Utc>,
    segment_end: chrono::DateTime<chrono::Utc>,
    trigger_type: String,
    structured_json: serde_json::Value,
    agreement_status: Option<String>,
    received_at: chrono::DateTime<chrono::Utc>,
}

async fn list_stream_compliance_reports(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspaceCameraPath {
        workspace_id,
        cam_id,
    }): Path<WorkspaceCameraPath>,
    Query(ComplianceListQuery { since, limit }): Query<ComplianceListQuery>,
) -> Response {
    let since_dt = match since {
        None => None,
        Some(s) => match chrono::DateTime::parse_from_rfc3339(&s) {
            Ok(dt) => Some(dt.with_timezone(&chrono::Utc)),
            Err(e) => {
                return map_err(crate::service::ServiceError::InvalidInput(format!(
                    "invalid `since` (expected RFC 3339): {e}"
                )));
            }
        },
    };
    let limit = limit.unwrap_or(compliance::DEFAULT_LIST_LIMIT);
    match compliance::list_for_camera(&db, workspace_id, cam_id, since_dt, limit).await {
        Ok(rows) => Json(
            rows.into_iter()
                .map(|r| StreamComplianceReportDto {
                    report_id: r.report_id,
                    camera_id: r.camera_id,
                    segment_start: r.segment_start,
                    segment_end: r.segment_end,
                    trigger_type: r.trigger_type,
                    structured_json: r.structured_json,
                    agreement_status: r.agreement_status,
                    received_at: r.received_at,
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => map_err(e),
    }
}

async fn list_stream_registry(
    Extension(db): Extension<DatabaseConnection>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
) -> Response {
    match listing::list_cameras(&db, workspace_id).await {
        Ok(rows) => Json(
            rows.into_iter()
                .map(|r| StreamRegistryEntryDto {
                    id: r.camera.id,
                    name: r.camera.name,
                    site_name: r.site_name,
                    protect_camera_id: r.camera.protect_camera_id,
                    mac_address: r.camera.mac_address,
                    online: r.camera.online,
                    edge_box_status: r.edge_box_status,
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => map_err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sh_squote_wraps_plain() {
        assert_eq!(sh_squote("Jetson Orin Nano"), "'Jetson Orin Nano'");
    }

    #[test]
    fn sh_squote_escapes_embedded_single_quote() {
        // `Jim's Box` would otherwise close the quote and run the
        // tail unquoted. Standard POSIX trick: close, escaped-quote,
        // reopen.
        assert_eq!(sh_squote("Jim's Box"), "'Jim'\\''s Box'");
    }

    #[test]
    fn render_install_command_quotes_label_with_apostrophe() {
        let cmd = render_install_command(
            Uuid::nil(),
            "secret-b64",
            "Jim's Box",
            Some("tskey-auth-doesnt-matter"),
        );
        // Whatever else is in the command, the label must be
        // contained inside a fully-closed single-quoted region.
        assert!(cmd.contains("--hardware-label 'Jim'\\''s Box'"));
    }

    #[test]
    fn render_install_command_quotes_ts_authkey_with_metachars() {
        let cmd = render_install_command(
            Uuid::nil(),
            "secret-b64",
            "label",
            Some("tskey'-with-quote"),
        );
        assert!(cmd.contains("--ts-authkey 'tskey'\\''-with-quote'"));
    }
}
