//! HTTP request/response DTOs.
//!
//! Kept separate from the entities so the wire format can evolve
//! independently of the persistence shape. Each handler converts
//! between DTO and entity on the boundary.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

// `RegisterEdgeBoxBody` / `RegisterEdgeBoxResponse` removed
// alongside the `POST /{wid}/cameras/edge-boxes` operator route.
// Edge boxes are now provisioned via Add device, which routes
// through `prepare_device` + `bootstrap_device`.

// ── Config ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CameraConfigDto {
    pub id: Uuid,
    pub name: String,
    pub rtsp_url: Option<String>,
    pub substream_url: Option<String>,
    pub zones_json: JsonValue,
    pub lines_json: JsonValue,
    pub analytics_consent: bool,
    /// Role tag the worker keys into its prompt registry. `None`
    /// (or unrecognized) falls back to the whole-frame general prompt.
    pub role: Option<String>,
}

/// Top-level response for `GET /control/boxes/{id}/config`. Wraps
/// the camera list and the workspace's active domain pack so the
/// worker fetches both in a single round-trip.
///
/// `domain_pack` is `Option<JsonValue>` rather than a typed
/// `PackBody` here because the wire shape evolves independently
/// of the backend's Rust types — the worker speaks the JSON
/// directly and will gracefully degrade if a field is missing.
#[derive(Debug, Serialize)]
pub struct EdgeConfigDto {
    pub cameras: Vec<CameraConfigDto>,
    /// Active domain pack body. `None` only when the workspace's
    /// pack hasn't been seeded yet (race during initial config
    /// poll); the worker falls back to its built-in prompts in
    /// that window.
    pub domain_pack: Option<JsonValue>,
    /// Workspace ID this edge box is bound to. The worker uses
    /// this for path-partitioning clip uploads in S3 (Tier B
    /// #6); future surfaces may need it too. We thread it
    /// through config rather than asking operators to set
    /// EDGE_WORKSPACE_ID as a separate env — the server already
    /// knows it via the bearer/JWT chain.
    pub workspace_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct PatchZonesBody {
    pub zones_json: Option<JsonValue>,
    pub lines_json: Option<JsonValue>,
}

#[derive(Debug, Deserialize)]
pub struct PatchConsentBody {
    pub analytics_consent: bool,
}

/// Update a camera's role tag (kitchen / prep / dining / other).
///
/// Sending `null` (or omitting `role` with `serde(default)`) clears
/// the role — the worker falls back to its general whole-frame
/// prompt for cameras without a role.
#[derive(Debug, Deserialize)]
pub struct PatchRoleBody {
    #[serde(default)]
    pub role: Option<String>,
}

// ── Domain pack ─────────────────────────────────────────────────────────────

/// Operator-facing snapshot of a domain pack row. Mirrors
/// `entities::domain_packs::Model` but ships through JSON, so the
/// `body` is a raw `JsonValue` rather than the typed `PackBody`.
/// The UI parses the body shape itself for the editor — and the
/// API stays forward-compatible if `PackBody` gains optional
/// fields between deploys.
#[derive(Debug, Serialize)]
pub struct DomainPackDto {
    pub id: Uuid,
    pub pack_key: String,
    pub name: String,
    pub description: Option<String>,
    pub starter_key: String,
    pub starter_version: String,
    pub locally_modified: bool,
    pub is_active: bool,
    pub body: JsonValue,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
}

impl From<crate::entities::domain_packs::Model> for DomainPackDto {
    fn from(m: crate::entities::domain_packs::Model) -> Self {
        Self {
            id: m.id,
            pack_key: m.pack_key,
            name: m.name,
            description: m.description,
            starter_key: m.starter_key,
            starter_version: m.starter_version,
            locally_modified: m.locally_modified,
            is_active: m.is_active,
            body: m.body,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

/// PUT body for `/camera-packs/{pack_key}/body`. The whole body
/// replaces the previous body. Partial updates aren't supported
/// in v1 — operators edit through a UI that round-trips the full
/// structure, so a JSON-Merge-Patch surface buys nothing.
#[derive(Debug, Deserialize)]
pub struct PutPackBody {
    pub body: JsonValue,
}

/// Compact pack row for the list endpoint. Omits `body` to keep
/// the list response cheap — operators fetch the body via the
/// active-pack endpoint (or via a future `GET /{wid}/camera-packs/{key}`)
/// only when they open the editor.
#[derive(Debug, Serialize)]
pub struct DomainPackSummaryDto {
    pub id: Uuid,
    pub pack_key: String,
    pub name: String,
    pub description: Option<String>,
    pub starter_key: String,
    pub starter_version: String,
    pub locally_modified: bool,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
}

impl From<crate::entities::domain_packs::Model> for DomainPackSummaryDto {
    fn from(m: crate::entities::domain_packs::Model) -> Self {
        Self {
            id: m.id,
            pack_key: m.pack_key,
            name: m.name,
            description: m.description,
            starter_key: m.starter_key,
            starter_version: m.starter_version,
            locally_modified: m.locally_modified,
            is_active: m.is_active,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

/// Reflects one entry in [`packs::starter::list`]. The UI uses
/// this for the "fork from starter" picker.
#[derive(Debug, Serialize)]
pub struct StarterPackSummaryDto {
    pub key: String,
    pub name: String,
    pub description: String,
    pub version: String,
}

/// POST body for `/camera-packs` — fork a starter into this
/// workspace. The new pack becomes the active one (atomically
/// deactivates the previous active pack).
#[derive(Debug, Deserialize)]
pub struct PostPackForkBody {
    pub starter_key: String,
}

/// POST body for `/camera-packs/{pack_key}/apply-update`. Each
/// entry maps a role_key (or [`VARIABLES_ACTION_KEY`]) to the
/// operator's choice. Missing entries default to "keep mine."
///
/// [`VARIABLES_ACTION_KEY`]: crate::service::packs::updates::VARIABLES_ACTION_KEY
#[derive(Debug, Deserialize)]
pub struct ApplyUpdateBody {
    pub actions: std::collections::HashMap<String, crate::service::packs::updates::UpdateAction>,
}

// ── Fleet claims (operator-facing) ──────────────────────────────────────────

// `PostClaimBody` and the `POST /{wid}/fleet/claims` route it
// fed were removed alongside the claim-by-code UI flow.
// `ClaimDto` survives because the retroactive bind handler
// (`POST .../bind-existing`) still returns the same shape.

/// Operator-facing summary of a freshly-created claim — returned by
/// the retroactive bind route. `device_id` is included so the UI
/// can show "device ABCD linked."
#[derive(Debug, Serialize)]
pub struct ClaimDto {
    pub claim_id: Uuid,
    pub device_id: Uuid,
    pub status: String,
}

/// POST body for `/{wid}/fleet/claims/bind-existing` — retroactive
/// claim flow for legacy bearer-auth edge boxes. The operator
/// reads the `device_id` from the worker's startup log (it's a
/// UUID v7 the new factory image generates on first boot) and
/// pastes it alongside the existing edge_box_id.
#[derive(Debug, Deserialize)]
pub struct BindExistingBody {
    pub device_id: Uuid,
    pub edge_box_id: Uuid,
}

/// DELETE body for `/{wid}/cameras/fleet/members`. The operator
/// hit Remove on a row in the merged boxes table; the row may
/// carry an `edge_box_id`, a `device_id`, or both. At least one
/// must be set — the service enforces that.
#[derive(Debug, Deserialize)]
pub struct RemoveFleetMemberBody {
    #[serde(default)]
    pub edge_box_id: Option<Uuid>,
    #[serde(default)]
    pub device_id: Option<Uuid>,
}

/// POST body for `/{wid}/fleet/prepared-devices` — the "Add
/// device" wizard backend. Operator picks which site the new
/// box will serve and optionally tags it with a hardware label
/// ("Jetson Orin Nano", "Intel N100", etc.) for their own
/// recordkeeping. No device-side credentials are required here
/// — the server mints them.
#[derive(Debug, Deserialize)]
pub struct PrepareDeviceBody {
    pub site_id: Uuid,
    #[serde(default)]
    pub hardware_label: Option<String>,
    /// Optional Tailscale auth-key (`tskey-auth-…`, tagged
    /// `tag:edge-box`). When set, the rendered install command
    /// includes `--ts-authkey <key>` so the new box joins the
    /// tailnet on first boot and the `tailscale-funnel-init`
    /// sidecar publishes WHEP via Funnel automatically. Never
    /// persisted server-side — pass-through into
    /// `install_command` only.
    #[serde(default)]
    pub ts_authkey: Option<String>,
}

/// Operator-facing payload returned from
/// `/{wid}/fleet/prepared-devices`. The `device_secret_b64` is
/// shown ONCE — the server never returns it again. The
/// `install_snippet` is a copy-pasteable shell command the
/// operator runs on the new box's terminal; running it writes
/// `/var/lib/oxy/device.json` with the identity, after which
/// `docker compose up -d` boots the worker into the normal
/// announce → bootstrap flow against the already-pending claim.
#[derive(Debug, Serialize)]
pub struct PrepareDeviceResponse {
    pub device_id: Uuid,
    pub device_secret_b64: String,
    pub claim_code: String,
    pub claim_id: Uuid,
    pub sku: String,
    pub hardware_revision: String,
    /// One-line `curl | sudo bash` command — fully sets up the
    /// box (docker + identity + compose + systemd unit) in a
    /// single command. Preferred path when the operator can
    /// curl from the box. See `install_snippet` for the
    /// no-network-access fallback.
    pub install_command: String,
    /// Shell command the operator runs on the new edge box. Writes
    /// the identity JSON to disk at 0600 perms.
    pub install_snippet: String,
}

// ── Manual provisioning ─────────────────────────────────────────────────────

/// POST body for `/{wid}/cameras/sites` — operator-created site
/// (no UniFi import). `region` is free-form; useful when a chain
/// operator has multiple regions (US-West, US-East) and wants to
/// roll up reports.
#[derive(Debug, Deserialize)]
pub struct CreateSiteBody {
    pub name: String,
    /// IANA tz like `America/Los_Angeles`. Defaults to `UTC` if
    /// omitted so the validator below doesn't reject a half-filled
    /// "Add site" form.
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default)]
    pub region: Option<String>,
}

fn default_timezone() -> String {
    "UTC".to_string()
}

/// POST body for `/{wid}/cameras` — operator adds a camera by
/// pasting its RTSP URL. The auto-claim path binds the camera to
/// the first edge box at the same site that polls config.
#[derive(Debug, Deserialize)]
pub struct CreateCameraBody {
    pub site_id: Uuid,
    pub name: String,
    pub rtsp_url: String,
    #[serde(default)]
    pub substream_url: Option<String>,
    #[serde(default)]
    pub credentials_ref: Option<String>,
}

/// PATCH body for `/{wid}/cameras/edge-boxes/{id}/target`. Both
/// fields are optional with `serde(default)` so the UI can clear
/// the target by sending `{"target_image_tag": null}` without
/// also clobbering `held_until`.
///
/// `target_image_tag`: e.g. `"ghcr.io/oxy/edge:3.4.2"` — operator
/// sets, device agent pulls.
/// `held_until`: RFC-3339 UTC. If in the future, the device agent
/// skips its pull cycle (don't touch the box during lunch rush).
#[derive(Debug, Deserialize)]
pub struct PatchEdgeBoxTargetBody {
    #[serde(default)]
    pub target_image_tag: Option<String>,
    #[serde(default)]
    pub held_until: Option<chrono::DateTime<chrono::Utc>>,
}

/// PATCH body for `/{wid}/cameras/edge-boxes/bulk/target`. Same
/// semantics as the single-box variant, applied to every
/// `box_ids` entry in one transaction. Empty list returns 0
/// without error.
#[derive(Debug, Deserialize)]
pub struct BulkPatchTargetBody {
    pub box_ids: Vec<Uuid>,
    #[serde(default)]
    pub target_image_tag: Option<String>,
    #[serde(default)]
    pub held_until: Option<chrono::DateTime<chrono::Utc>>,
}

/// Response for the bulk target endpoint. `updated_count` is the
/// rows-affected from the underlying UPDATE — equals
/// `len(unique(box_ids))` on success.
#[derive(Debug, Serialize)]
pub struct BulkPatchTargetResponse {
    pub updated_count: u64,
}

// ── UniFi onboarding ────────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
pub struct UnifiPreviewBody {
    /// Optional — falls back to the workspace-stored UniFi credential
    /// when absent. First-time setup still passes the key here.
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UnifiImportBody {
    /// Optional — falls back to the workspace-stored UniFi credential
    /// when absent.
    #[serde(default)]
    pub api_key: Option<String>,
    /// Optional substring filter on site name; `None` imports all sites
    /// in the customer's UniFi account.
    pub site_filter: Option<String>,
}

/// Workspace id extracted from the URL path. The route layer mounts
/// these handlers under `/{workspace_id}/...` so the value is enforced
/// by the upstream `workspace_middleware` (cloud) or
/// `local_context_middleware` (local). Trust this value; do NOT trust
/// any `workspace_id` that arrives in a request body.
#[derive(Debug, Deserialize)]
pub struct WorkspacePath {
    pub workspace_id: Uuid,
}

// ── Ingest (stubbed — pending step 10) ──────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct EventBatch {
    pub events: Vec<crate::service::ingest::EventPayload>,
}

#[derive(Debug, Deserialize)]
pub struct ComplianceBatch {
    pub reports: Vec<crate::service::compliance::CompliancePayload>,
}

/// Body for `POST /control/boxes/{id}/logs`. Sent by the edge
/// worker's log shipper as a batch (~50 rows or 30s, whichever
/// hits first).
#[derive(Debug, Deserialize)]
pub struct LogBatch {
    pub logs: Vec<crate::service::logs::LogPayload>,
}

/// Body for `POST /control/clips/sign` — worker asks for a
/// presigned PUT URL it can use to upload one compliance-clip
/// fMP4. Includes `content_length` so the presign can bind it,
/// removing the "leaked URL → arbitrary blob upload" risk.
#[derive(Debug, Deserialize)]
pub struct SignClipBody {
    pub report_id: Uuid,
    pub segment_start: chrono::DateTime<chrono::Utc>,
    pub content_length: i64,
}

#[derive(Debug, Deserialize)]
pub struct CameraHealthBody {
    pub fps: Option<f32>,
    pub bitrate_kbps: Option<f32>,
    pub last_frame_at: Option<chrono::DateTime<chrono::Utc>>,
    pub decoder_errors: i32,
    pub reconnect_count: i32,
}

#[derive(Debug, Deserialize)]
pub struct BoxHealthBody {
    pub cpu_pct: Option<f32>,
    pub gpu_pct: Option<f32>,
    pub mem_pct: Option<f32>,
    pub temp_c: Option<f32>,
    pub uptime_s: Option<i64>,
    pub image_tag: Option<String>,
    pub containers_running: Option<i32>,
    /// IoT Phase 2 — populated by the device's update agent after
    /// it attempts to swap to a new image. Values: `applied` |
    /// `reverted` | `in_progress` | `revert_failed` | `stuck_at_broken_target`.
    /// The control plane records this against the edge_box so the
    /// operator UI can show the state of the last update attempt.
    #[serde(default)]
    pub last_update_result: Option<String>,
    /// CI #3 — compatibility manifest baked into the running image
    /// at build time. Worker reads `/opt/oxy-edge/compatibility.json`
    /// on startup and forwards it untouched. Server stores the raw
    /// JSON; the operator UI surfaces `edge_version` and a future
    /// `/fleet/announce` gate can reject too-old workers when we
    /// bump `protocol_version`. Optional so older workers (built
    /// before this column existed) keep posting health successfully.
    #[serde(default)]
    pub compatibility: Option<serde_json::Value>,
}

/// Response to `GET /control/boxes/{id}/target`. The device's
/// update agent polls this every ~5 minutes and decides whether
/// to pull + restart. Mirrors `service::updates::TargetView`.
#[derive(Debug, Serialize)]
pub struct TargetView {
    pub target_image_tag: Option<String>,
    pub current_image_tag: Option<String>,
    pub held_until: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<crate::service::updates::TargetView> for TargetView {
    fn from(v: crate::service::updates::TargetView) -> Self {
        Self {
            target_image_tag: v.target_image_tag,
            current_image_tag: v.current_image_tag,
            held_until: v.held_until,
        }
    }
}

/// `POST /control/boxes/{id}/bandwidth` body. Stamped by the
/// worker every ~5 minutes from `_bandwidth_reporter`, derived
/// from MediaMTX's `bytesSent` deltas across all paths.
#[derive(Debug, Deserialize)]
pub struct BoxBandwidthBody {
    /// Total `bytesSent` delta from MTX across the reporter's
    /// interval. Includes ALL outbound (Funnel + internal tailnet
    /// HLS proxying) — intentionally overcounts since the safer
    /// alarm direction is "too high, look closer."
    pub delta_bytes: u64,
    /// Reporter window length in seconds — useful for computing
    /// per-second throughput rather than relying on a fixed
    /// interval assumption.
    pub interval_s: u32,
}

#[derive(Debug, Serialize)]
pub struct AcceptedResponse {
    pub accepted: usize,
}

// ── List / detail DTOs ──────────────────────────────────────────────────────
//
// Wire-format mirrors of the SeaORM entity models, plus joined display
// fields for the table-row endpoints. Kept as DTOs (not `Serialize`
// on the entities directly) so the wire format can evolve without
// schema migrations.

#[derive(Debug, Serialize)]
pub struct SiteDto {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub timezone: String,
    pub region: Option<String>,
    /// `manual` (operator-created) or `unifi` (imported via the
    /// UniFi onboarding flow). UI shows a badge per row so
    /// operators know which sites refresh on re-import vs. stay
    /// frozen.
    pub source: String,
    /// WAN-facing IP for pre-edge-box compliance — drives the bulk
    /// RTSP-rewrite flow. `null` means no NAT path configured.
    pub public_ip: Option<String>,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
}

impl From<crate::entities::sites::Model> for SiteDto {
    fn from(m: crate::entities::sites::Model) -> Self {
        Self {
            id: m.id,
            workspace_id: m.workspace_id,
            name: m.name,
            timezone: m.timezone,
            region: m.region,
            source: m.source,
            public_ip: m.public_ip,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EdgeBoxDto {
    pub id: Uuid,
    pub site_id: Uuid,
    pub hardware_model: String,
    pub image_tag: String,
    pub cohort: String,
    pub tailscale_ip: Option<String>,
    pub status: String,
    pub unifi_console_id: Option<String>,
    pub unifi_public_ip: Option<String>,
    pub unifi_rtsp_reachable: bool,
    pub registered_at: chrono::DateTime<chrono::FixedOffset>,
    pub last_seen_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    /// Most recent 5-minute outbound byte count from MTX, stamped
    /// via `/control/boxes/{id}/bandwidth`. UI uses this to flag
    /// boxes nearing the per-tailnet Funnel quota.
    pub bandwidth_5min_bytes: Option<i64>,
    pub bandwidth_reported_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    /// IoT Phase 2 — software-update channel state. Operator sets
    /// `target_image_tag`; device reports `current_image_tag` +
    /// `last_update_result` via the health endpoint. The UI shows
    /// pending updates by comparing target ≠ current.
    pub target_image_tag: Option<String>,
    pub current_image_tag: Option<String>,
    pub held_until: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub last_update_result: Option<String>,
    pub last_update_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    /// `'bearer' | 'jwt'`. Stamped on every successful auth by
    /// the middleware. After the 2026-06 cutover JWT-only is the
    /// default; a `bearer` value means the box's next ping will
    /// 401. Operators see this as a per-box badge in the Fleet
    /// tab and re-onboard the affected box.
    pub auth_mode: String,
    /// CI #3 — last compatibility manifest the worker posted. Null
    /// for older workers that don't yet send it. Operator UI uses
    /// the `edge_version` field to render the running version next
    /// to each box.
    pub edge_compatibility_json: Option<serde_json::Value>,
    /// P1 #3 — set when the worker's reported protocol_version is
    /// below `OXY_MIN_EDGE_PROTOCOL_VERSION`. Operator UI shows
    /// an amber "needs upgrade" badge with this string as the
    /// tooltip. NULL when compatible or no manifest posted yet.
    pub incompatible_reason: Option<String>,
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
}

impl From<crate::entities::edge_boxes::Model> for EdgeBoxDto {
    fn from(m: crate::entities::edge_boxes::Model) -> Self {
        Self {
            id: m.id,
            site_id: m.site_id,
            hardware_model: m.hardware_model,
            image_tag: m.image_tag,
            cohort: m.cohort,
            tailscale_ip: m.tailscale_ip,
            status: m.status,
            unifi_console_id: m.unifi_console_id,
            unifi_public_ip: m.unifi_public_ip,
            unifi_rtsp_reachable: m.unifi_rtsp_reachable,
            registered_at: m.registered_at,
            last_seen_at: m.last_seen_at,
            bandwidth_5min_bytes: m.bandwidth_5min_bytes,
            bandwidth_reported_at: m.bandwidth_reported_at,
            target_image_tag: m.target_image_tag,
            current_image_tag: m.current_image_tag,
            held_until: m.held_until,
            last_update_result: m.last_update_result,
            last_update_at: m.last_update_at,
            auth_mode: m.auth_mode,
            edge_compatibility_json: m.edge_compatibility_json,
            incompatible_reason: m.incompatible_reason,
            updated_at: m.updated_at,
        }
    }
}

/// Edge-box row with the human-readable site name folded in — for UI
/// table rendering without a frontend-side N+1 lookup.
#[derive(Debug, Serialize)]
pub struct EdgeBoxSummaryDto {
    #[serde(flatten)]
    pub edge_box: EdgeBoxDto,
    pub site_name: String,
}

impl From<crate::service::listing::EdgeBoxWithSite> for EdgeBoxSummaryDto {
    fn from(v: crate::service::listing::EdgeBoxWithSite) -> Self {
        Self {
            edge_box: v.edge_box.into(),
            site_name: v.site_name,
        }
    }
}

/// One row in the merged fleet view (`GET /{wid}/cameras/fleet`).
/// Combines an edge_box (when bootstrapped) with its IoT identity
/// (when claimed). A pending claim with no edge_box yet emits a
/// row where `edge_box` is None but the identity fields are
/// populated — that's the operator's "this device is on its way
/// but hasn't called home yet" signal.
#[derive(Debug, Serialize)]
pub struct FleetMemberDto {
    pub edge_box: Option<EdgeBoxDto>,
    pub site_id: Uuid,
    pub site_name: String,
    pub device_id: Option<Uuid>,
    /// `legacy` (no identity layer) | `pending_claim` | `claimed`.
    /// The UI uses this to color the row's status badge.
    pub claim_status: String,
    pub claim_code: Option<String>,
    pub sku: Option<String>,
    pub hardware_revision: Option<String>,
    pub last_announce_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

impl From<crate::service::listing::FleetMemberRow> for FleetMemberDto {
    fn from(v: crate::service::listing::FleetMemberRow) -> Self {
        Self {
            edge_box: v.edge_box.map(Into::into),
            site_id: v.site_id,
            site_name: v.site_name,
            device_id: v.device_id,
            claim_status: v.claim_status.as_str().to_string(),
            claim_code: v.claim_code,
            sku: v.sku,
            hardware_revision: v.hardware_revision,
            last_announce_at: v.last_announce_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CameraDto {
    pub id: Uuid,
    pub site_id: Uuid,
    pub edge_box_id: Option<Uuid>,
    pub name: String,
    pub rtsp_url: Option<String>,
    pub substream_url: Option<String>,
    pub credentials_ref: Option<String>,
    pub zones_json: JsonValue,
    pub lines_json: JsonValue,
    pub analytics_consent: bool,
    pub active: bool,
    pub protect_camera_id: Option<String>,
    pub mac_address: Option<String>,
    pub model: Option<String>,
    pub online: Option<bool>,
    /// One of `kitchen` | `prep` | `dining` | `other`, or null if
    /// unset. The edge worker uses this to pick a role-specific VLM
    /// prompt; null falls back to the whole-frame general prompt.
    pub role: Option<String>,
    /// Operator zone-approval workflow: pending proposal slot.
    /// NULL = no proposal. Distinct from `proposed_*_json` being
    /// an empty array — that would CLEAR the live geometry when
    /// the operator approves.
    pub proposed_zones_json: Option<JsonValue>,
    pub proposed_lines_json: Option<JsonValue>,
    pub proposed_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
}

impl From<crate::entities::cameras::Model> for CameraDto {
    fn from(m: crate::entities::cameras::Model) -> Self {
        Self {
            id: m.id,
            site_id: m.site_id,
            edge_box_id: m.edge_box_id,
            name: m.name,
            rtsp_url: m.rtsp_url,
            substream_url: m.substream_url,
            credentials_ref: m.credentials_ref,
            zones_json: m.zones_json,
            lines_json: m.lines_json,
            analytics_consent: m.analytics_consent,
            active: m.active,
            protect_camera_id: m.protect_camera_id,
            mac_address: m.mac_address,
            model: m.model,
            online: m.online,
            role: m.role,
            proposed_zones_json: m.proposed_zones_json,
            proposed_lines_json: m.proposed_lines_json,
            proposed_at: m.proposed_at,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

/// Camera row with site_name + edge_box_name folded in — for the
/// per-row "Camera 1 @ Pokehouse, bound to N100-EDGE-3" rendering.
#[derive(Debug, Serialize)]
pub struct CameraSummaryDto {
    #[serde(flatten)]
    pub camera: CameraDto,
    pub site_name: String,
    /// `None` for cameras imported from inventory but not yet bound to
    /// an edge box (the operator hasn't installed a worker for them).
    pub edge_box_name: Option<String>,
    /// `pending` | `active` | `offline` | `retired`. The UI surfaces
    /// `offline` as a warning on the row — even if the camera itself
    /// reports `online: true`, an offline edge box means snapshot /
    /// HLS / WebRTC proxies will all fail.
    pub edge_box_status: Option<String>,
}

impl From<crate::service::listing::CameraWithJoins> for CameraSummaryDto {
    fn from(v: crate::service::listing::CameraWithJoins) -> Self {
        Self {
            camera: v.camera.into(),
            site_name: v.site_name,
            edge_box_name: v.edge_box_name,
            edge_box_status: v.edge_box_status,
        }
    }
}

/// One compliance report row as the operator UI's Compliance tab
/// consumes it. The Airhouse row's `structured_json` is already
/// decoded into a real `JsonValue` here — the UI can render it
/// without round-tripping through `JSON.parse` on a string column.
#[derive(Debug, Serialize)]
pub struct ComplianceReportDto {
    pub report_id: String,
    pub camera_id: String,
    pub segment_start: chrono::DateTime<chrono::Utc>,
    pub segment_end: chrono::DateTime<chrono::Utc>,
    pub trigger_type: String,
    pub trigger_track_id: Option<String>,
    pub vlm_model: String,
    pub report_text: String,
    pub structured_json: JsonValue,
    pub frame_uri: Option<String>,
    pub tokens_used: Option<i32>,
    /// Tier B #6 — S3 key for the archived dwell-window clip.
    /// Bucket-relative path; the operator UI shows a "View clip"
    /// button when set.
    pub evidence_s3_key: Option<String>,
    pub received_at: chrono::DateTime<chrono::Utc>,
}

impl From<crate::service::compliance::ComplianceReportRow> for ComplianceReportDto {
    fn from(r: crate::service::compliance::ComplianceReportRow) -> Self {
        Self {
            report_id: r.report_id,
            camera_id: r.camera_id,
            segment_start: r.segment_start,
            segment_end: r.segment_end,
            trigger_type: r.trigger_type,
            trigger_track_id: r.trigger_track_id,
            vlm_model: r.vlm_model,
            report_text: r.report_text,
            structured_json: r.structured_json,
            frame_uri: r.frame_uri,
            tokens_used: r.tokens_used,
            evidence_s3_key: r.evidence_s3_key,
            received_at: r.received_at,
        }
    }
}

/// One row of the site-rollup endpoint: per-camera incident counts.
/// Cameras with zero reports still appear so an empty grid means
/// "no cameras at this site," not "no reports for these cameras."
#[derive(Debug, Serialize)]
pub struct CameraComplianceSummaryDto {
    pub camera_id: String,
    pub camera_name: String,
    pub edge_box_name: Option<String>,
    pub total_reports: u64,
    pub violations: u64,
    pub last_received_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<crate::service::compliance::CameraComplianceSummary> for CameraComplianceSummaryDto {
    fn from(s: crate::service::compliance::CameraComplianceSummary) -> Self {
        Self {
            camera_id: s.camera_id,
            camera_name: s.camera_name,
            edge_box_name: s.edge_box_name,
            total_reports: s.total_reports,
            violations: s.violations,
            last_received_at: s.last_received_at,
        }
    }
}

/// Response shape for the WebRTC session endpoint. The browser
/// consumes `whep_url` to POST its SDP offer, and configures
/// `ice_servers` on `RTCPeerConnection` for NAT traversal /
/// TURN-over-TLS via Tailscale Funnel.
#[derive(Debug, Serialize)]
pub struct WebrtcSessionDto {
    pub whep_url: String,
    pub ice_servers: Vec<IceServerDto>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct IceServerDto {
    pub urls: Vec<String>,
    pub username: String,
    pub credential: String,
}

impl From<crate::service::webrtc::WebrtcSession> for WebrtcSessionDto {
    fn from(s: crate::service::webrtc::WebrtcSession) -> Self {
        Self {
            whep_url: s.whep_url,
            ice_servers: s
                .ice_servers
                .into_iter()
                .map(|i| IceServerDto {
                    urls: i.urls,
                    username: i.username,
                    credential: i.credential,
                })
                .collect(),
            expires_at: s.expires_at,
        }
    }
}
