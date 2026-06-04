//! Manual site + camera provisioning.
//!
//! Companion to `service::onboarding` (which imports from UniFi).
//! Both write to the same `sites` / `cameras` tables — the `source`
//! column on `sites` tracks where the row came from.

use crate::entities::{cameras, sites};
use crate::service::{ServiceError, ServiceResult};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use uuid::Uuid;

/// Operator-supplied inputs for `POST /{wid}/cameras/sites`. We
/// accept the minimum a site needs: a human name and a timezone
/// (used downstream by dashboards for "convert UTC → site-local").
pub struct CreateSiteInput {
    pub name: String,
    pub timezone: String,
    pub region: Option<String>,
}

/// Create a new manual site. Conflict on `(workspace_id, name)` —
/// we don't enforce uniqueness in the DB (the UniFi importer uses
/// the same `(name)` key for its upsert via deterministic UUIDs),
/// but we reject duplicates at the API boundary to give operators
/// a clean 409 instead of a confusing "two sites with the same
/// name" surface in the dropdown.
pub async fn create_site(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    input: CreateSiteInput,
) -> ServiceResult<sites::Model> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(ServiceError::InvalidInput("name is required".into()));
    }
    if input.timezone.trim().is_empty() {
        return Err(ServiceError::InvalidInput(
            "timezone is required (use 'UTC' if you're not sure)".into(),
        ));
    }

    // Per-workspace name uniqueness check.
    let existing = sites::Entity::find()
        .filter(sites::Column::WorkspaceId.eq(workspace_id))
        .filter(sites::Column::Name.eq(name))
        .one(db)
        .await?;
    if existing.is_some() {
        return Err(ServiceError::Conflict(format!(
            "site `{name}` already exists in this workspace"
        )));
    }

    let now = chrono::Utc::now().fixed_offset();
    let am = sites::ActiveModel {
        id: Set(Uuid::new_v4()),
        workspace_id: Set(workspace_id),
        name: Set(name.to_string()),
        timezone: Set(input.timezone),
        region: Set(input.region),
        source: Set("manual".into()),
        // Operator sets public_ip later via PATCH when they want to
        // expose the site's NAT-ed UniFi controller to the backend
        // for pre-edge-box demos. NULL on create is the right default.
        public_ip: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    };
    Ok(am.insert(db).await?)
}

/// Operator-supplied inputs for `POST /{wid}/cameras` — manual
/// camera entry. The operator provides the RTSP URL directly;
/// the edge box (once it's bound to the same site) will pick the
/// camera up via the existing auto-claim flow on its next config
/// poll.
pub struct CreateCameraInput {
    pub site_id: Uuid,
    pub name: String,
    pub rtsp_url: String,
    pub substream_url: Option<String>,
    /// Reference into the workspace secret store. Used when the
    /// RTSP URL itself doesn't carry credentials (i.e. when the
    /// camera requires a username/password but the operator
    /// doesn't want it embedded in the URL).
    pub credentials_ref: Option<String>,
}

/// Create a manual camera at an operator-owned site. Validates:
///   - site exists and belongs to caller's workspace,
///   - rtsp_url is non-empty,
///   - no other camera in the same site has the same name (matches
///     the (site_id, name) uniqueness the UniFi importer already
///     relies on for its upsert).
pub async fn create_camera(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    input: CreateCameraInput,
) -> ServiceResult<cameras::Model> {
    let name = input.name.trim();
    let rtsp = input.rtsp_url.trim();
    if name.is_empty() {
        return Err(ServiceError::InvalidInput("name is required".into()));
    }
    if rtsp.is_empty() {
        return Err(ServiceError::InvalidInput(
            "rtsp_url is required (e.g. rtsp://user:pass@cam.lan:554/Streaming/Channels/101)"
                .into(),
        ));
    }

    // Site ownership check. Operator pasting a site_id from
    // another workspace would otherwise plant a camera under it.
    let site = sites::Entity::find_by_id(input.site_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    if site.workspace_id != workspace_id {
        return Err(ServiceError::Forbidden("site belongs to another workspace"));
    }

    // Per-site name uniqueness.
    let dup = cameras::Entity::find()
        .filter(cameras::Column::SiteId.eq(input.site_id))
        .filter(cameras::Column::Name.eq(name))
        .one(db)
        .await?;
    if dup.is_some() {
        return Err(ServiceError::Conflict(format!(
            "camera `{name}` already exists at this site"
        )));
    }

    let now = chrono::Utc::now().fixed_offset();
    let am = cameras::ActiveModel {
        id: Set(Uuid::new_v4()),
        site_id: Set(input.site_id),
        // No edge_box yet — the auto-claim path in
        // `service::config::fetch_cameras_for_edge` will bind the
        // camera to whichever edge box at the same site polls
        // config first. Same mechanism the UniFi import relies on.
        edge_box_id: sea_orm::NotSet,
        name: Set(name.to_string()),
        rtsp_url: Set(Some(rtsp.to_string())),
        substream_url: Set(input.substream_url),
        credentials_ref: Set(input.credentials_ref),
        zones_json: Set(serde_json::json!([])),
        lines_json: Set(serde_json::json!([])),
        // Default OFF — privacy-safe. See `service::onboarding`
        // for the full rationale. The Dashboard / Devices
        // banners surface unreviewed cameras and link to the
        // review flow so analytics gets enabled deliberately.
        analytics_consent: Set(false),
        active: Set(true),
        protect_camera_id: Set(None),
        mac_address: Set(None),
        model: Set(None),
        online: Set(None),
        role: Set(None),
        proposed_zones_json: Set(None),
        proposed_lines_json: Set(None),
        proposed_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    };
    Ok(am.insert(db).await?)
}
