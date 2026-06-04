//! Workspace-scoped read endpoints feeding the UI dashboard.
//!
//! Every list / detail-get filters through `sites.workspace_id` —
//! that's the cross-aggregate boundary. Detail-get returns
//! `ServiceError::Forbidden` (not 404) when the row exists but
//! belongs to another workspace; route layer maps that to 403.
//! Symmetric with the write-side guards in [`super::registration`]
//! and [`super::config`].
//!
//! Joined fields (site_name on edge boxes, edge_box_name on cameras)
//! come from SeaORM's `find_also_related`. One JOIN, one round-trip
//! per list call — no N+1.

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::entities::{cameras, device_claims, device_registry, edge_boxes, sites};

use super::{ServiceError, ServiceResult};

// ── Sites ───────────────────────────────────────────────────────────────────

pub async fn list_sites(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> ServiceResult<Vec<sites::Model>> {
    Ok(sites::Entity::find()
        .filter(sites::Column::WorkspaceId.eq(workspace_id))
        .all(db)
        .await?)
}

pub async fn get_site(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    site_id: Uuid,
) -> ServiceResult<sites::Model> {
    let site = sites::Entity::find_by_id(site_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    if site.workspace_id != workspace_id {
        return Err(ServiceError::Forbidden("site belongs to another workspace"));
    }
    Ok(site)
}

// ── Edge boxes ──────────────────────────────────────────────────────────────

/// One edge box plus the name of its site — for UI tables that show
/// "EDGE-BOX-01 at Pokehouse Almaden" without the frontend doing N+1.
#[derive(Debug, Clone)]
pub struct EdgeBoxWithSite {
    pub edge_box: edge_boxes::Model,
    pub site_name: String,
}

/// Magic `hardware_model` value the UniFi import flow uses to stash
/// `unifi_public_ip` + `unifi_console_id` per site. These rows aren't
/// real edge boxes — they don't run the worker, they don't take
/// bearers — they exist purely as a place for the import to write
/// the per-site controller metadata.
///
/// The cleaner data model is "store the public_ip on `sites`," but
/// that's a migration we haven't done. For now: filter these out of
/// every operator-facing read.
pub const UNIFI_CONTROLLER_MODEL: &str = "unifi-controller";

/// Fleet-wide rollup of `edge_boxes.auth_mode` for the operator
/// dashboard. Post-cutover the summary still lets operators see at
/// a glance which boxes are about to 401 (bearer) vs healthy (jwt)
/// vs never-booted (unknown).
///
/// Three buckets:
///   - `jwt` — box has authenticated via Phase 3 JWT at least once.
///     Healthy under the JWT-only default.
///   - `bearer` — box is still on the legacy static bearer. Will
///     401 on next ping (set `OXY_ALLOW_LEGACY_BEARER=true` as an
///     emergency lever during a rolling upgrade if needed).
///   - `unknown` — box exists but has never authenticated. Treated
///     as "needs attention" so freshly-prepared devices that
///     haven't booted yet don't get lost.
///
/// UniFi controller rows are excluded — they aren't real boxes
/// and don't speak the device-token protocol.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuthModeSummary {
    pub jwt: u32,
    pub bearer: u32,
    pub unknown: u32,
}

pub async fn auth_mode_summary(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> ServiceResult<AuthModeSummary> {
    let rows = edge_boxes::Entity::find()
        .find_also_related(sites::Entity)
        .filter(sites::Column::WorkspaceId.eq(workspace_id))
        .filter(edge_boxes::Column::HardwareModel.ne(UNIFI_CONTROLLER_MODEL))
        .all(db)
        .await?;

    let mut summary = AuthModeSummary {
        jwt: 0,
        bearer: 0,
        unknown: 0,
    };
    for (eb, _site) in rows {
        match eb.auth_mode.as_str() {
            "jwt" => summary.jwt += 1,
            "bearer" => summary.bearer += 1,
            _ => summary.unknown += 1,
        }
    }
    Ok(summary)
}

pub async fn list_edge_boxes(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> ServiceResult<Vec<EdgeBoxWithSite>> {
    // `find_also_related` does its own LEFT JOIN on `edge_boxes.site_id
    // = sites.id` and lets us filter on either side. We FK-cascade on
    // site delete, so the `None` arm in `(edge_box, Option<site>)`
    // shouldn't fire in practice — the `filter_map` is defensive.
    //
    // Retired rows are excluded — every consumer of this endpoint
    // (cameras-table filter dropdown, rollout cohort picker,
    // topology graph) wants active-only. Operators who need to see
    // retired boxes for audit use the merged Fleet view
    // (`list_fleet_members`), which intentionally includes them.
    let rows = edge_boxes::Entity::find()
        .find_also_related(sites::Entity)
        .filter(sites::Column::WorkspaceId.eq(workspace_id))
        .filter(edge_boxes::Column::HardwareModel.ne(UNIFI_CONTROLLER_MODEL))
        .filter(edge_boxes::Column::Status.ne("retired"))
        .all(db)
        .await?;

    Ok(rows
        .into_iter()
        .filter_map(|(eb, site)| {
            site.map(|s| EdgeBoxWithSite {
                edge_box: eb,
                site_name: s.name,
            })
        })
        .collect())
}

/// Merged "physical boxes" view — every device the operator
/// owns regardless of which lifecycle stage it's in.
///
/// **Why merged.** `edge_boxes` is the runtime view (the worker
/// is running on this Docker host); `device_registry +
/// device_claims` is the identity view (this physical device
/// has an HMAC secret + is bound to this workspace). For
/// 99% of operator workflows they're the same object — once
/// the device bootstraps the two timelines collapse onto one
/// row. The brief window where they diverge is between
/// `Add device` (creates a pending_claim with no edge_box) and
/// the device's first `/control/*` call (materializes the
/// edge_box and flips the claim to `claimed`).
///
/// We emit one row per physical box:
///   - **Real edge_box** with no claim: legacy bearer-auth box
///     registered before the IoT identity layer existed. The
///     identity columns are `None`.
///   - **Real edge_box** with a claimed device_claim: the
///     post-Phase-3 case. All columns populated.
///   - **Pending claim** with no edge_box: a freshly-added
///     device that hasn't bootstrapped yet. Operational
///     columns are `None`; the identity columns show what the
///     operator needs to onboard.
///
/// UniFi controller rows are filtered out the same way
/// [`list_edge_boxes`] does — they aren't real boxes.
#[derive(Debug, Clone)]
pub struct FleetMemberRow {
    pub edge_box: Option<edge_boxes::Model>,
    pub site_id: Uuid,
    pub site_name: String,
    pub device_id: Option<Uuid>,
    pub claim_status: ClaimStatus,
    pub claim_code: Option<String>,
    pub sku: Option<String>,
    pub hardware_revision: Option<String>,
    pub last_announce_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

/// What state the device-identity layer thinks this box is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimStatus {
    /// No `device_claims` row — pre-IoT box on the bearer path.
    /// `device_id` and friends are `None`. Operator can use the
    /// retroactive-claim flow to attach an identity.
    Legacy,
    /// Operator pre-claimed; device hasn't bootstrapped yet.
    /// `edge_box` is `None`.
    Pending,
    /// Fully claimed and bootstrapped. Both `edge_box` and
    /// `device_id` are set.
    Claimed,
}

impl ClaimStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Pending => "pending_claim",
            Self::Claimed => "claimed",
        }
    }
}

pub async fn list_fleet_members(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> ServiceResult<Vec<FleetMemberRow>> {
    // Two queries, joined client-side:
    //   1. edge_boxes JOIN sites for this workspace
    //   2. device_claims (pending|claimed) JOIN device_registry for this workspace
    // Then we walk both lists and emit one merged row per
    // physical box. The DB-side joins are bounded by workspace
    // and small, so this is cheap.
    let edge_box_rows: Vec<(edge_boxes::Model, Option<sites::Model>)> = edge_boxes::Entity::find()
        .find_also_related(sites::Entity)
        .filter(sites::Column::WorkspaceId.eq(workspace_id))
        .filter(edge_boxes::Column::HardwareModel.ne(UNIFI_CONTROLLER_MODEL))
        // Retired boxes have been operator-removed; they
        // disappear from the merged list (the row + its
        // history remain in the DB for compliance / audit).
        .filter(edge_boxes::Column::Status.ne("retired"))
        .all(db)
        .await?;

    let claims: Vec<device_claims::Model> = device_claims::Entity::find()
        .filter(device_claims::Column::WorkspaceId.eq(workspace_id))
        // Filter to active claims only; revoked rows are
        // audit-trail material, not operator-visible.
        .filter(device_claims::Column::Status.is_in(["pending_claim", "claimed"]))
        .all(db)
        .await?;

    // Index by edge_box_id and by device_id so the merge loop is O(n).
    use std::collections::HashMap;
    let claim_by_edge_box: HashMap<Uuid, &device_claims::Model> = claims
        .iter()
        .filter_map(|c| c.edge_box_id.map(|eb| (eb, c)))
        .collect();

    // Bulk-fetch the device_registry rows the claims point at so
    // we can surface claim_code / hardware_revision / last_announce
    // without N+1.
    let device_ids: Vec<Uuid> = claims.iter().map(|c| c.device_id).collect();
    let devices: HashMap<Uuid, device_registry::Model> = if device_ids.is_empty() {
        HashMap::new()
    } else {
        device_registry::Entity::find()
            .filter(device_registry::Column::DeviceId.is_in(device_ids))
            .all(db)
            .await?
            .into_iter()
            .map(|d| (d.device_id, d))
            .collect()
    };

    let mut out: Vec<FleetMemberRow> = Vec::with_capacity(edge_box_rows.len() + claims.len());

    // Pass 1 — every real edge_box, with claim info merged in if
    // a matching claim exists.
    for (eb, site_opt) in edge_box_rows {
        let Some(site) = site_opt else {
            continue;
        };
        let site_id = site.id;
        let site_name = site.name;
        if let Some(claim) = claim_by_edge_box.get(&eb.id) {
            let dev = devices.get(&claim.device_id);
            out.push(FleetMemberRow {
                edge_box: Some(eb),
                site_id,
                site_name,
                device_id: Some(claim.device_id),
                claim_status: match claim.status.as_str() {
                    "pending_claim" => ClaimStatus::Pending,
                    _ => ClaimStatus::Claimed,
                },
                claim_code: dev.map(|d| d.claim_code.clone()),
                sku: dev.map(|d| d.sku.clone()),
                hardware_revision: dev.map(|d| d.hardware_revision.clone()),
                last_announce_at: dev.and_then(|d| d.last_announce_at),
            });
        } else {
            out.push(FleetMemberRow {
                edge_box: Some(eb),
                site_id,
                site_name,
                device_id: None,
                claim_status: ClaimStatus::Legacy,
                claim_code: None,
                sku: None,
                hardware_revision: None,
                last_announce_at: None,
            });
        }
    }

    // Pass 2 — pending claims that haven't materialized an
    // edge_box yet. The site for the row comes from the claim
    // itself (`device_claims.site_id`); look it up. Tiny query
    // since pending counts are O(in-flight installs).
    let pending_with_no_box: Vec<&device_claims::Model> = claims
        .iter()
        .filter(|c| c.edge_box_id.is_none() && c.status == "pending_claim")
        .collect();
    if !pending_with_no_box.is_empty() {
        let site_ids: Vec<Uuid> = pending_with_no_box
            .iter()
            .filter_map(|c| c.site_id)
            .collect();
        let site_names: HashMap<Uuid, (Uuid, String)> = if site_ids.is_empty() {
            HashMap::new()
        } else {
            sites::Entity::find()
                .filter(sites::Column::Id.is_in(site_ids))
                .all(db)
                .await?
                .into_iter()
                .map(|s| (s.id, (s.id, s.name)))
                .collect()
        };
        for claim in pending_with_no_box {
            let Some(site_id) = claim.site_id else {
                continue;
            };
            let Some((site_id_resolved, site_name)) = site_names.get(&site_id).cloned() else {
                // Site deleted out from under the pending claim
                // — skip the row rather than show a "missing
                // site" placeholder operators can't act on.
                continue;
            };
            let dev = devices.get(&claim.device_id);
            out.push(FleetMemberRow {
                edge_box: None,
                site_id: site_id_resolved,
                site_name,
                device_id: Some(claim.device_id),
                claim_status: ClaimStatus::Pending,
                claim_code: dev.map(|d| d.claim_code.clone()),
                sku: dev.map(|d| d.sku.clone()),
                hardware_revision: dev.map(|d| d.hardware_revision.clone()),
                last_announce_at: dev.and_then(|d| d.last_announce_at),
            });
        }
    }

    Ok(out)
}

pub async fn get_edge_box(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    edge_box_id: Uuid,
) -> ServiceResult<edge_boxes::Model> {
    let eb = edge_boxes::Entity::find_by_id(edge_box_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    // UniFi-controller rows aren't real edge boxes — pretend they
    // don't exist at the operator API layer.
    if eb.hardware_model == UNIFI_CONTROLLER_MODEL {
        return Err(ServiceError::NotFound);
    }
    let site = sites::Entity::find_by_id(eb.site_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    if site.workspace_id != workspace_id {
        return Err(ServiceError::Forbidden(
            "edge box belongs to another workspace",
        ));
    }
    Ok(eb)
}

// ── Cameras ─────────────────────────────────────────────────────────────────

/// One camera plus the human-readable names + reachability for its
/// site and bound edge box. `edge_box_name` and `edge_box_status`
/// are both `None` for cameras imported via UniFi inventory but
/// not yet bound to a worker.
#[derive(Debug, Clone)]
pub struct CameraWithJoins {
    pub camera: cameras::Model,
    pub site_name: String,
    pub edge_box_name: Option<String>,
    /// Box status string — `pending` | `active` | `offline` |
    /// `retired`. The `stale_checker` flips `active` → `offline`
    /// when `last_seen_at` goes silent past the threshold, and the
    /// auth middleware flips it back to `active` on the next
    /// `/control/*` call. UI uses this to warn operators when a
    /// camera's edge isn't reachable even if the camera itself is
    /// `online` per UniFi (the snapshot / HLS / WebRTC proxies all
    /// dial the box back, so an offline box means broken preview).
    pub edge_box_status: Option<String>,
}

pub async fn list_cameras(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> ServiceResult<Vec<CameraWithJoins>> {
    let with_site = cameras::Entity::find()
        .find_also_related(sites::Entity)
        .filter(sites::Column::WorkspaceId.eq(workspace_id))
        .all(db)
        .await?;

    // Edge-box details come from a second batch lookup so the JOIN
    // above stays a single inner-join on sites (the workspace
    // boundary). `cameras.edge_box_id` is nullable per the SET NULL
    // on edge_box delete, so we can't INNER JOIN it without dropping
    // unbound rows.
    let edge_box_ids: Vec<Uuid> = with_site
        .iter()
        .filter_map(|(cam, _)| cam.edge_box_id)
        .collect();
    let edge_box_meta: std::collections::HashMap<Uuid, (String, String)> =
        if edge_box_ids.is_empty() {
            std::collections::HashMap::new()
        } else {
            edge_boxes::Entity::find()
                .filter(edge_boxes::Column::Id.is_in(edge_box_ids))
                .all(db)
                .await?
                .into_iter()
                .map(|eb| {
                    let display = format!("{} ({})", eb.hardware_model, eb.cohort);
                    (eb.id, (display, eb.status))
                })
                .collect()
        };

    Ok(with_site
        .into_iter()
        .filter_map(|(cam, site)| {
            site.map(|s| {
                let (edge_box_name, edge_box_status) = match cam.edge_box_id {
                    Some(id) => edge_box_meta
                        .get(&id)
                        .map(|(n, s)| (Some(n.clone()), Some(s.clone())))
                        .unwrap_or((None, None)),
                    None => (None, None),
                };
                CameraWithJoins {
                    camera: cam,
                    site_name: s.name,
                    edge_box_name,
                    edge_box_status,
                }
            })
        })
        .collect())
}

pub async fn get_camera(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    camera_id: Uuid,
) -> ServiceResult<cameras::Model> {
    let cam = cameras::Entity::find_by_id(camera_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    let site = sites::Entity::find_by_id(cam.site_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    if site.workspace_id != workspace_id {
        return Err(ServiceError::Forbidden(
            "camera belongs to another workspace",
        ));
    }
    Ok(cam)
}
