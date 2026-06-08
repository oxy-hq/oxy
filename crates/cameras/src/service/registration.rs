//! Edge box registration.
//!
//! Operator pre-provisions an edge_box row for a specific site. The
//! device's identity layer (`device_registry` + `device_claims`)
//! authenticates every subsequent `/control/*` request via per-device
//! JWT — no static bearers.

use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, DatabaseConnection, EntityTrait, Set,
    prelude::Uuid as SeaUuid,
};
use uuid::Uuid;

use crate::entities::{edge_boxes, sites};

use super::{ServiceError, ServiceResult};

/// Operator-supplied parameters for pre-registering an edge box.
#[derive(Debug, Clone)]
pub struct RegisterEdgeBoxInput {
    pub site_id: Uuid,
    pub hardware_model: String,
    /// e.g. `"jetson-orin-nx-16gb"` / `"n100-mini-pc"` / `"unifi-controller"`.
    /// Free-form for now; canonicalize later if we want a fixed taxonomy.
    pub image_tag: Option<String>,
    /// Cohort label for OTA rollouts: `"stable"` (default) or `"canary"`.
    pub cohort: Option<String>,
}

/// Result of a successful registration.
#[derive(Debug)]
pub struct RegisterEdgeBoxOutput {
    pub edge_box_id: Uuid,
}

/// Pre-register an edge box bound to a site. Caller is responsible for
/// authorizing the operator (Workspace.Admin or higher) **and** the
/// route layer is responsible for resolving `workspace_id` from the
/// authenticated session (URL `Path<WorkspacePath>` behind
/// `workspace_middleware`).
///
/// This service fn enforces the cross-aggregate boundary: the site must
/// exist *and* be in `workspace_id`. Otherwise we'd let a caller with
/// access to workspace A pre-register an edge box bound to workspace
/// B's site (a real cross-workspace write leak — `sites.workspace_id`
/// is a loose Uuid column with no FK, so the DB doesn't catch this).
pub async fn register_edge_box(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    input: RegisterEdgeBoxInput,
) -> ServiceResult<RegisterEdgeBoxOutput> {
    if input.hardware_model.trim().is_empty() {
        return Err(ServiceError::InvalidInput(
            "hardware_model is required".into(),
        ));
    }

    // Site must exist and belong to the caller's workspace.
    let site = sites::Entity::find_by_id::<SeaUuid>(input.site_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    if site.workspace_id != workspace_id {
        return Err(ServiceError::Forbidden("site belongs to another workspace"));
    }

    let edge_box_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let cohort = input.cohort.unwrap_or_else(|| "stable".into());
    let image_tag = input.image_tag.unwrap_or_else(|| "unknown".into());

    let edge_box = edge_boxes::ActiveModel {
        id: Set(edge_box_id),
        site_id: Set(input.site_id),
        hardware_model: Set(input.hardware_model),
        image_tag: Set(image_tag),
        cohort: Set(cohort),
        tailscale_ip: NotSet,
        funnel_hostname: NotSet,
        bandwidth_5min_bytes: NotSet,
        bandwidth_reported_at: NotSet,
        target_image_tag: NotSet,
        current_image_tag: NotSet,
        held_until: NotSet,
        last_update_result: NotSet,
        last_update_at: NotSet,
        edge_compatibility_json: NotSet,
        incompatible_reason: NotSet,
        status: Set("pending".into()),
        unifi_console_id: NotSet,
        unifi_public_ip: NotSet,
        unifi_rtsp_reachable: Set(false),
        registered_at: Set(now.into()),
        last_seen_at: NotSet,
        updated_at: Set(now.into()),
    };
    edge_box.insert(db).await?;

    // Camera-intent trigger: registering an edge box is an explicit
    // "this workspace runs camera infrastructure" signal. Eagerly
    // create the `oxy_cam_*` tables so the first event the box sends
    // lands fast. Soft-fail (logged) so a misconfigured Airhouse
    // doesn't break a Postgres registration — the lazy ensure on the
    // ingest path will retry next event.
    if let Err(e) = crate::airhouse::ensure_schema(workspace_id).await {
        tracing::warn!(
            workspace_id = %workspace_id,
            edge_box_id = %edge_box_id,
            error = %e,
            "ensure_schema failed during register_edge_box; lazy ensure will retry on first ingest"
        );
    }

    Ok(RegisterEdgeBoxOutput { edge_box_id })
}
