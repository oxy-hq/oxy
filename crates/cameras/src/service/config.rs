//! Camera config — read assigned cameras for an edge, PATCH zone /
//! line geometry.
//!
//! Edge-facing reads return the **active** cameras bound to a given
//! edge_box: name, RTSP URL, current zones / lines, and the analytics
//! consent flag (a `false` here is the data-layer enforcement of "do
//! not collect").

use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set, prelude::Expr,
};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::entities::{cameras, edge_boxes, sites};

use super::{ServiceError, ServiceResult};

/// Check that a camera belongs to a site in the caller's workspace.
/// Returns the camera row on success; `ServiceError::NotFound` if the
/// camera doesn't exist; `ServiceError::Forbidden` if it exists but
/// belongs to another workspace.
///
/// Pattern: every operator-side mutation that targets a specific camera
/// runs this guard before touching the row. The cameras → sites join is
/// what enforces the workspace boundary, since `cameras` itself has no
/// `workspace_id` column — that lives on `sites` per the loose
/// cross-aggregate ref (`domain-boundaries.md` P3).
async fn camera_owned_by_workspace(
    db: &DatabaseConnection,
    camera_id: Uuid,
    workspace_id: Uuid,
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

/// All cameras a given edge box is responsible for. Filters out
/// `active = false` rows so a deactivated camera disappears from the
/// edge's config without needing a redeploy.
///
/// **Auto-claim**: atomically binds any camera at this box's site
/// that is either unbound (`edge_box_id IS NULL`) OR bound to a
/// retired box. The retired-box clause heals stale bindings left
/// behind by the old "remove_member didn't unbind cameras" bug:
/// operators who replaced an edge box would see the new live box
/// believe the site had zero cameras because every row still
/// pointed at the now-retired one. Future replacements unbind
/// proactively in `remove_member`, but this clause is the
/// self-healing fallback so a stuck deployment recovers on the
/// next poll without an operator-run cleanup script.
///
/// **Multi-box per site**: first live edge box to poll wins
/// everything (the UPDATE acquires row locks). PoC scale is one
/// active box per site; if a real customer ever needs a per-camera
/// split we add manual rebind UI.
pub async fn fetch_cameras_for_edge(
    db: &DatabaseConnection,
    edge_box_id: Uuid,
) -> ServiceResult<Vec<cameras::Model>> {
    // Look up the box so we know which site to claim from. If the
    // box doesn't exist it's a programmer error — the auth middleware
    // already resolved it before we got here.
    let edge_box = edge_boxes::Entity::find_by_id(edge_box_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;

    // Find the set of "claimable" current bindings at this site:
    // null (never bound) + any retired box's id. Computed as a
    // separate SELECT so the UPDATE filter stays index-friendly
    // (we don't want a correlated subquery on a hot config poll).
    let retired_box_ids: Vec<Uuid> = edge_boxes::Entity::find()
        .filter(edge_boxes::Column::SiteId.eq(edge_box.site_id))
        .filter(edge_boxes::Column::Status.eq("retired"))
        .filter(edge_boxes::Column::Id.ne(edge_box_id))
        .all(db)
        .await?
        .into_iter()
        .map(|b| b.id)
        .collect();

    let now = chrono::Utc::now().fixed_offset();
    // Atomic claim of unbound rows.
    cameras::Entity::update_many()
        .col_expr(cameras::Column::EdgeBoxId, Expr::value(Some(edge_box_id)))
        .col_expr(cameras::Column::UpdatedAt, Expr::value(now))
        .filter(cameras::Column::SiteId.eq(edge_box.site_id))
        .filter(cameras::Column::EdgeBoxId.is_null())
        .exec(db)
        .await?;
    // Atomic claim of rows previously pinned to a retired box.
    // Skipped when there are no retired peers — `is_in([])`
    // would filter to false and waste a round-trip.
    if !retired_box_ids.is_empty() {
        cameras::Entity::update_many()
            .col_expr(cameras::Column::EdgeBoxId, Expr::value(Some(edge_box_id)))
            .col_expr(cameras::Column::UpdatedAt, Expr::value(now))
            .filter(cameras::Column::SiteId.eq(edge_box.site_id))
            .filter(cameras::Column::EdgeBoxId.is_in(retired_box_ids))
            .exec(db)
            .await?;
    }

    Ok(cameras::Entity::find()
        .filter(cameras::Column::EdgeBoxId.eq(edge_box_id))
        .filter(cameras::Column::Active.eq(true))
        .all(db)
        .await?)
}

/// Operator updates the camera's zones and/or lines.
///
/// Caller-supplied `workspace_id` (from `Path<WorkspacePath>` behind
/// `workspace_middleware`) bounds the mutation: cross-workspace targets
/// return Forbidden. At least one of `zones_json` / `lines_json` must
/// be `Some`. Both `None` is rejected as `InvalidInput`.
pub async fn patch_camera_zones(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    camera_id: Uuid,
    zones_json: Option<JsonValue>,
    lines_json: Option<JsonValue>,
) -> ServiceResult<cameras::Model> {
    if zones_json.is_none() && lines_json.is_none() {
        return Err(ServiceError::InvalidInput(
            "must provide at least one of zones_json, lines_json".into(),
        ));
    }
    let cam = camera_owned_by_workspace(db, camera_id, workspace_id).await?;

    let mut am: cameras::ActiveModel = cam.into();
    if let Some(z) = zones_json {
        am.zones_json = Set(z);
    }
    if let Some(l) = lines_json {
        am.lines_json = Set(l);
    }
    am.updated_at = Set(chrono::Utc::now().into());
    Ok(am.update(db).await?)
}

/// Flip the per-camera `analytics_consent` flag. The edge worker
/// respects this on its next config poll (events from cameras with
/// `analytics_consent = false` get dropped at source).
pub async fn set_analytics_consent(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    camera_id: Uuid,
    consent: bool,
) -> ServiceResult<cameras::Model> {
    let cam = camera_owned_by_workspace(db, camera_id, workspace_id).await?;
    let mut am: cameras::ActiveModel = cam.into();
    am.analytics_consent = Set(consent);
    am.updated_at = Set(chrono::Utc::now().into());
    Ok(am.update(db).await?)
}

/// Allowed role values. Kept as a constant rather than a Rust enum
/// because the worker side treats role as an opaque key with a
/// fallback — adding `bar` or `drive_thru` later only needs a new
/// prompt entry on the worker, no schema or backend deploy.
pub const KNOWN_ROLES: &[&str] = &["kitchen", "prep", "dining", "other"];

/// Set the per-camera role used by the worker's prompt registry.
///
/// `role = Some("kitchen")` → worker uses the kitchen prompt next
/// poll. `role = None` (or null in the JSON body) → clear the role
/// and fall back to the general whole-frame prompt.
///
/// Unknown values are rejected with `InvalidInput` so a typo
/// (`"kitch3n"`) silently degrading to the general prompt is
/// surfaced at the API boundary rather than discovered later by
/// looking at compliance output.
pub async fn set_role(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    camera_id: Uuid,
    role: Option<String>,
) -> ServiceResult<cameras::Model> {
    let normalized = role.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if let Some(r) = normalized
        && !KNOWN_ROLES.contains(&r)
    {
        return Err(ServiceError::InvalidInput(format!(
            "role must be one of {:?} or null",
            KNOWN_ROLES
        )));
    }
    let cam = camera_owned_by_workspace(db, camera_id, workspace_id).await?;
    let mut am: cameras::ActiveModel = cam.into();
    am.role = Set(normalized.map(str::to_string));
    am.updated_at = Set(chrono::Utc::now().into());
    Ok(am.update(db).await?)
}

// ── Operator zone approval workflow ─────────────────────────────────────────
//
// An upstream (auto-zone heuristic, agent, or operator hitting
// the dialog directly) writes a proposal via `propose_geometry`.
// The proposal lives alongside the live `zones_json` / `lines_json`
// so the camera keeps operating against the previously-approved
// geometry until a human reviews. `approve` swaps proposal → live;
// `reject` clears the proposal without touching live.
//
// A `None` payload for either side means "no change proposed for
// that field." Sending both as `None` is rejected as
// `InvalidInput`; it would otherwise leave the existing proposal
// untouched and waste a write.

/// Set (or replace) the proposal for one camera. Either or both
/// of zones/lines may be supplied. Stamps `proposed_at = now()`.
pub async fn propose_geometry(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    camera_id: Uuid,
    zones_json: Option<JsonValue>,
    lines_json: Option<JsonValue>,
) -> ServiceResult<cameras::Model> {
    if zones_json.is_none() && lines_json.is_none() {
        return Err(ServiceError::InvalidInput(
            "must provide at least one of zones_json, lines_json".into(),
        ));
    }
    let cam = camera_owned_by_workspace(db, camera_id, workspace_id).await?;
    let mut am: cameras::ActiveModel = cam.into();
    if let Some(z) = zones_json {
        am.proposed_zones_json = Set(Some(z));
    }
    if let Some(l) = lines_json {
        am.proposed_lines_json = Set(Some(l));
    }
    let now = chrono::Utc::now();
    am.proposed_at = Set(Some(now.into()));
    am.updated_at = Set(now.into());
    Ok(am.update(db).await?)
}

/// Approve the pending proposal. Copies `proposed_*_json` → live
/// columns and clears the proposal. Returns `InvalidInput` when
/// there is no proposal pending — the UI shouldn't get here, but
/// erroring is safer than silently writing the existing live
/// geometry back over itself.
pub async fn approve_proposed_geometry(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    camera_id: Uuid,
) -> ServiceResult<cameras::Model> {
    let cam = camera_owned_by_workspace(db, camera_id, workspace_id).await?;
    if cam.proposed_zones_json.is_none() && cam.proposed_lines_json.is_none() {
        return Err(ServiceError::InvalidInput(
            "no proposal pending for this camera".into(),
        ));
    }
    let zones_to_apply = cam.proposed_zones_json.clone();
    let lines_to_apply = cam.proposed_lines_json.clone();
    let mut am: cameras::ActiveModel = cam.into();
    if let Some(z) = zones_to_apply {
        am.zones_json = Set(z);
    }
    if let Some(l) = lines_to_apply {
        am.lines_json = Set(l);
    }
    am.proposed_zones_json = Set(None);
    am.proposed_lines_json = Set(None);
    am.proposed_at = Set(None);
    am.updated_at = Set(chrono::Utc::now().into());
    Ok(am.update(db).await?)
}

/// Clear the pending proposal without touching the live geometry.
/// `InvalidInput` when there's no proposal — same rationale as
/// `approve_proposed_geometry`.
pub async fn reject_proposed_geometry(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    camera_id: Uuid,
) -> ServiceResult<cameras::Model> {
    let cam = camera_owned_by_workspace(db, camera_id, workspace_id).await?;
    if cam.proposed_zones_json.is_none() && cam.proposed_lines_json.is_none() {
        return Err(ServiceError::InvalidInput(
            "no proposal pending for this camera".into(),
        ));
    }
    let mut am: cameras::ActiveModel = cam.into();
    am.proposed_zones_json = Set(None);
    am.proposed_lines_json = Set(None);
    am.proposed_at = Set(None);
    am.updated_at = Set(chrono::Utc::now().into());
    Ok(am.update(db).await?)
}
