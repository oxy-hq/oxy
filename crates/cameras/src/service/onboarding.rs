//! UniFi-specific onboarding.
//!
//! Rust port of `video-poc/inference-prototype/onboard_unifi_inventory.py`.
//! Two surfaces:
//!
//! - [`preview`] — read-only enumeration of what would be imported.
//!   Validates the API key, calls the cloud, returns a typed summary.
//!   Use case: customer pastes their key in the Oxy UI; we render
//!   "we'd create 17 sites and 109 cameras" before they commit.
//! - [`import`] — runs the full inventory pull and upserts the
//!   `sites` / `edge_boxes` / `cameras` rows. Idempotent via
//!   `uuid::Uuid::new_v5` on stable UniFi identifiers — re-running
//!   doesn't create duplicates, and renames / IP changes flow through
//!   as updates.

use sea_orm::{ActiveValue::NotSet, DatabaseConnection, EntityTrait, Set, sea_query::OnConflict};
use serde::Serialize;
use uuid::Uuid;

use crate::entities::{cameras, edge_boxes, sites};
use oxy_unifi::UnifiClient;
use oxy_unifi::devices::Device;
use oxy_unifi::hosts::Host;

use super::ServiceResult;

// ── Preview ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct OnboardingPreview {
    pub sites: Vec<SitePreview>,
    pub total_sites: usize,
    pub total_cameras: usize,
}

#[derive(Debug, Serialize)]
pub struct SitePreview {
    pub unifi_console_id: String,
    pub name: String,
    pub public_ip: Option<String>,
    pub camera_count: usize,
    pub online_camera_count: usize,
    pub hardware_model: Option<String>,
}

/// Read-only: enumerate the UniFi fleet without writing anything.
/// Useful for showing the operator what would be imported before they
/// commit.
pub async fn preview(unifi: &UnifiClient) -> ServiceResult<OnboardingPreview> {
    let hosts = unifi.list_hosts().await?;
    let device_groups = unifi.list_devices().await?;

    let mut sites = Vec::with_capacity(hosts.len());
    let mut total_cameras = 0usize;

    for h in &hosts {
        let devs = device_groups
            .iter()
            .find(|g| g.host_id == h.id)
            .map(|g| g.devices.as_slice())
            .unwrap_or(&[]);
        let cams: Vec<&Device> = devs.iter().filter(|d| d.is_camera()).collect();
        let online = cams
            .iter()
            .filter(|d| d.status.as_deref() == Some("online"))
            .count();
        total_cameras += cams.len();
        sites.push(SitePreview {
            unifi_console_id: h.id.clone(),
            name: site_name_from_host(h),
            public_ip: h.ip_address.clone(),
            camera_count: cams.len(),
            online_camera_count: online,
            hardware_model: h
                .reported_state
                .as_ref()
                .and_then(|rs| rs.hardware.as_ref())
                .and_then(|hw| hw.shortname.clone()),
        });
    }

    let total_sites = sites.len();
    Ok(OnboardingPreview {
        sites,
        total_sites,
        total_cameras,
    })
}

// ── Import ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ImportInput {
    /// Workspace the imported sites are scoped to (loose UUID column).
    pub workspace_id: Uuid,
    /// Substring filter for `host_name`. `None` imports the entire fleet.
    pub site_filter: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct ImportResult {
    pub sites_upserted: usize,
    pub edge_boxes_upserted: usize,
    pub cameras_upserted: usize,
    pub skipped_no_workspace: usize,
}

/// Run the full inventory pull and upsert into Postgres. Idempotent:
/// re-running updates names / IPs / online status but doesn't create
/// duplicates.
///
/// Caller-supplied `workspace_id` is the loose cross-aggregate ref —
/// every imported site gets it. (Multi-workspace partitioning across
/// the same UniFi account isn't supported yet; would need a host_id →
/// workspace mapping in the UI.)
pub async fn import(
    db: &DatabaseConnection,
    unifi: &UnifiClient,
    input: ImportInput,
) -> ServiceResult<ImportResult> {
    let mut result = ImportResult::default();

    let hosts = unifi.list_hosts().await?;
    let device_groups = unifi.list_devices().await?;

    for h in hosts {
        let host_name = site_name_from_host(&h);
        if let Some(filter) = &input.site_filter
            && !host_name.to_lowercase().contains(&filter.to_lowercase())
        {
            continue;
        }

        // ── Upsert site ──────────────────────────────────────────────
        let site_id = deterministic_uuid("site", &h.id);
        let detail = unifi.get_host(&h.id).await.ok();
        let public_ip = h
            .ip_address
            .clone()
            .or_else(|| detail.as_ref().and_then(|d| d.ip_address.clone()));
        let timezone = detail
            .as_ref()
            .and_then(|d| d.reported_state.as_ref())
            .and_then(|rs| rs.timezone.clone())
            .unwrap_or_else(|| "UTC".into());

        let now = chrono::Utc::now();
        sites::Entity::insert(sites::ActiveModel {
            id: Set(site_id),
            workspace_id: Set(input.workspace_id),
            name: Set(host_name.clone()),
            timezone: Set(timezone.clone()),
            region: NotSet,
            // Tag every UniFi-imported site so re-import never
            // clobbers a manual site that happens to share the
            // deterministic id (in practice impossible — the id
            // is hash(unifi_console_id) — but the source filter
            // is the contract).
            source: Set("unifi".to_string()),
            // Mirror the UniFi controller's WAN IP onto the site so
            // operators can run the pre-edge-box bulk-RTSP-rewrite
            // without re-entering an address they already have.
            // Re-imports also refresh this — see the OnConflict
            // update list below — so an ISP IP rotation flows through
            // the next sync automatically. NULL when UniFi didn't
            // report one (rare, but possible on a controller that's
            // offline at sync time).
            public_ip: Set(public_ip.clone()),
            created_at: Set(now.into()),
            updated_at: Set(now.into()),
        })
        .on_conflict(
            OnConflict::column(sites::Column::Id)
                .update_columns([
                    sites::Column::Name,
                    sites::Column::Timezone,
                    sites::Column::PublicIp,
                    sites::Column::UpdatedAt,
                ])
                .to_owned(),
        )
        .exec(db)
        .await?;
        result.sites_upserted += 1;

        // ── Upsert edge_box (one per console) ────────────────────────
        let edge_box_id = deterministic_uuid("edge_box", &h.id);
        edge_boxes::Entity::insert(edge_boxes::ActiveModel {
            id: Set(edge_box_id),
            site_id: Set(site_id),
            hardware_model: Set("unifi-controller".into()),
            image_tag: Set("cloud".into()),
            cohort: Set("stable".into()),
            tailscale_ip: NotSet,
            funnel_hostname: NotSet,
            bandwidth_5min_bytes: NotSet,
            bandwidth_reported_at: NotSet,
            target_image_tag: NotSet,
            current_image_tag: NotSet,
            held_until: NotSet,
            last_update_result: NotSet,
            last_update_at: NotSet,
            auth_mode: NotSet,
            edge_compatibility_json: NotSet,
            incompatible_reason: NotSet,
            status: Set("active".into()),
            unifi_console_id: Set(Some(h.id.clone())),
            unifi_public_ip: Set(public_ip.clone()),
            unifi_rtsp_reachable: Set(false),
            registered_at: Set(now.into()),
            last_seen_at: NotSet,
            updated_at: Set(now.into()),
        })
        .on_conflict(
            OnConflict::column(edge_boxes::Column::Id)
                .update_columns([
                    edge_boxes::Column::SiteId,
                    edge_boxes::Column::UnifiConsoleId,
                    edge_boxes::Column::UnifiPublicIp,
                    edge_boxes::Column::Status,
                    edge_boxes::Column::UpdatedAt,
                ])
                .to_owned(),
        )
        .exec(db)
        .await?;
        result.edge_boxes_upserted += 1;

        // ── Upsert cameras for this console ──────────────────────────
        let devs = device_groups
            .iter()
            .find(|g| g.host_id == h.id)
            .map(|g| g.devices.as_slice())
            .unwrap_or(&[]);
        for dv in devs.iter().filter(|d| d.is_camera()) {
            let cam_id = deterministic_uuid("camera", &dv.id);
            let camera_name = dv.name.clone().unwrap_or_else(|| dv.id.clone());

            // Use the PRIMARY KEY for conflict detection. `cam_id` is
            // `deterministic_uuid("camera", &dv.id)` — derived from
            // the stable UniFi device id — so re-imports of the same
            // UniFi device land on the same row.
            //
            // Why not `(site_id, name)`: UniFi customers routinely run
            // multiple cameras with the same display name (a row of
            // "Cooler" cameras pointed at four walk-ins). The legacy
            // `(site_id, name)` unique index silently rerouted the
            // second INSERT into an UPDATE against the first row's
            // PK, losing the additional cameras. The
            // `DropCamerasSiteIdNameUnique` migration drops that
            // index; this OnConflict change makes the new behavior
            // explicit.
            //
            // `edge_box_id` is intentionally NULL on import: the
            // controller-shaped edge_box (above) exists only to stash
            // `unifi_public_ip` per site, NOT to own the cameras.
            // Ownership is what `fetch_cameras_for_edge`'s auto-claim
            // assigns when the operator's real edge box first calls
            // home. Pre-binding to the controller here would defeat
            // the auto-claim entirely (the cameras would never look
            // unbound, so a freshly-registered edge box would see an
            // empty config — exactly the bug we hit during testing).
            //
            // Re-imports must NOT clobber `edge_box_id` either: a
            // camera that's already been auto-claimed by a real edge
            // box should stay bound across re-imports.
            cameras::Entity::insert(cameras::ActiveModel {
                id: Set(cam_id),
                site_id: Set(site_id),
                edge_box_id: NotSet,
                name: Set(camera_name),
                rtsp_url: NotSet,
                substream_url: NotSet,
                credentials_ref: NotSet,
                zones_json: Set(serde_json::json!([])),
                lines_json: Set(serde_json::json!([])),
                // Default OFF. This is the privacy-safe starting
                // point: a freshly imported camera doesn't fire
                // VLM calls until the operator has reviewed it
                // (set zones, confirmed consent). The Dashboard
                // and Devices banners surface the unreviewed
                // count and link to the review surface — see
                // `SetupNeededBanner` and `CamerasTable`'s
                // `?review=1` mode. Re-imports preserve any
                // operator opt-in via the OnConflict update
                // list, which intentionally does NOT include
                // `analytics_consent`.
                analytics_consent: Set(false),
                active: Set(true),
                protect_camera_id: Set(Some(dv.id.clone())),
                mac_address: Set(dv.mac.clone()),
                model: Set(dv.shortname.clone()),
                online: Set(Some(dv.status.as_deref() == Some("online"))),
                role: NotSet,
                proposed_zones_json: NotSet,
                proposed_lines_json: NotSet,
                proposed_at: NotSet,
                created_at: Set(now.into()),
                updated_at: Set(now.into()),
            })
            .on_conflict(
                OnConflict::column(cameras::Column::Id)
                    .update_columns([
                        // Keep `Name` refreshable so a UniFi-side
                        // rename flows through. Manual edits at the
                        // Oxy UI level are NOT clobbered today because
                        // there's no manual rename surface yet — when
                        // one lands, we'll either gate this with a
                        // `name_locked` flag or drop Name from the
                        // update list.
                        cameras::Column::Name,
                        cameras::Column::ProtectCameraId,
                        cameras::Column::MacAddress,
                        cameras::Column::Model,
                        cameras::Column::Online,
                        // EdgeBoxId intentionally NOT updated — see
                        // comment on the insert above.
                        cameras::Column::UpdatedAt,
                    ])
                    .to_owned(),
            )
            .exec(db)
            .await?;
            result.cameras_upserted += 1;
        }
    }

    // Camera-intent trigger: a UniFi import that landed at least one
    // site signals the user wants to use the camera fleet for this
    // workspace. Eagerly create the `oxy_cam_*` tables in their
    // Airhouse tenant so the first event lands fast. Soft-fail: if
    // Airhouse isn't configured yet the lazy ensure on the ingest
    // path will retry — we log and continue rather than rolling back
    // the Postgres import. UI flow should guide users to set up
    // Airhouse before UniFi so this rarely needs to fire late.
    if result.sites_upserted > 0
        && let Err(e) = crate::airhouse::ensure_schema(input.workspace_id).await
    {
        tracing::warn!(
            workspace_id = %input.workspace_id,
            sites_upserted = result.sites_upserted,
            error = %e,
            "ensure_schema failed during UniFi import; lazy ensure will retry on first ingest"
        );
    }

    Ok(result)
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Derive a stable UUID from a UniFi identifier so re-runs are
/// idempotent. Matches the Python script's `uuid5(NAMESPACE_URL,
/// "<kind>:<id>")` pattern.
fn deterministic_uuid(kind: &str, key: &str) -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("{kind}:{key}").as_bytes())
}

/// Site name fallback chain: `reportedState.name` → host hostname →
/// console id (truncated). Matches the Python script's behavior.
fn site_name_from_host(h: &Host) -> String {
    if let Some(rs) = h.reported_state.as_ref() {
        if let Some(name) = rs.name.clone() {
            return name;
        }
        if let Some(hostname) = rs.hostname.clone() {
            return hostname;
        }
    }
    format!("unifi-console-{}", h.id.chars().take(8).collect::<String>())
}
