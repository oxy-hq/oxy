//! `/api/{workspace_id}/oxy-access` — toggle "let Oxy build tailored
//! apps on our data" for a workspace.
//!
//! Org owners (resolved to workspace Owner by `workspace_middleware`)
//! can enable or disable the toggle. While enabled, anyone in the
//! `app_admins` table (Oxy staff) can access customer apps for this
//! workspace. Disabling it deletes the row — there's no soft state.
//!
//! See [`crate::server::api::customer_apps_auth`] for how the toggle is
//! consulted on the request path.

use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use entity::prelude::WorkspaceOxyAccess;
use entity::workspace_members::WorkspaceRole;
use entity::workspace_oxy_access;
use oxy::database::client::establish_connection;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::server::api::customer_apps_auth::invalidate_access_cache;
use crate::server::api::middlewares::workspace_context::EffectiveWorkspaceRole;

/// Status payload for the toggle. `enabled` is always present; the
/// audit fields are only meaningful when `enabled` is true.
#[derive(Serialize)]
pub struct OxyAccessStatus {
    pub enabled: bool,
    pub granted_by: Option<Uuid>,
    pub granted_at: Option<String>,
}

impl OxyAccessStatus {
    fn disabled() -> Self {
        Self {
            enabled: false,
            granted_by: None,
            granted_at: None,
        }
    }

    fn from_row(row: workspace_oxy_access::Model) -> Self {
        Self {
            enabled: true,
            granted_by: row.granted_by,
            granted_at: Some(row.created_at.to_rfc3339()),
        }
    }
}

#[derive(Deserialize)]
pub struct WorkspaceIdPath {
    pub workspace_id: Uuid,
}

fn require_owner(role: WorkspaceRole) -> Result<(), StatusCode> {
    if role >= WorkspaceRole::Owner {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

pub async fn get_oxy_access(
    EffectiveWorkspaceRole(role): EffectiveWorkspaceRole,
    Path(WorkspaceIdPath { workspace_id }): Path<WorkspaceIdPath>,
) -> Result<Json<OxyAccessStatus>, StatusCode> {
    require_owner(role)?;
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("get_oxy_access DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let row = WorkspaceOxyAccess::find()
        .filter(workspace_oxy_access::Column::WorkspaceId.eq(workspace_id))
        .one(&db)
        .await
        .map_err(|e| {
            tracing::error!("get_oxy_access query failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(
        row.map(OxyAccessStatus::from_row)
            .unwrap_or_else(OxyAccessStatus::disabled),
    ))
}

pub async fn enable_oxy_access(
    EffectiveWorkspaceRole(role): EffectiveWorkspaceRole,
    Path(WorkspaceIdPath { workspace_id }): Path<WorkspaceIdPath>,
    AuthenticatedUserExtractor(actor): AuthenticatedUserExtractor,
) -> Result<Json<OxyAccessStatus>, StatusCode> {
    require_owner(role)?;
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("enable_oxy_access DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if let Some(existing) = WorkspaceOxyAccess::find()
        .filter(workspace_oxy_access::Column::WorkspaceId.eq(workspace_id))
        .one(&db)
        .await
        .map_err(|e| {
            tracing::error!("enable_oxy_access existence check failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    {
        return Ok(Json(OxyAccessStatus::from_row(existing)));
    }

    let model = workspace_oxy_access::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        workspace_id: ActiveValue::Set(workspace_id),
        granted_by: ActiveValue::Set(Some(actor.id)),
        created_at: ActiveValue::NotSet,
    }
    .insert(&db)
    .await
    .map_err(|e| {
        tracing::error!("enable_oxy_access insert failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    invalidate_access_cache();
    Ok(Json(OxyAccessStatus::from_row(model)))
}

pub async fn disable_oxy_access(
    EffectiveWorkspaceRole(role): EffectiveWorkspaceRole,
    Path(WorkspaceIdPath { workspace_id }): Path<WorkspaceIdPath>,
) -> Result<StatusCode, StatusCode> {
    require_owner(role)?;
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("disable_oxy_access DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    WorkspaceOxyAccess::delete_many()
        .filter(workspace_oxy_access::Column::WorkspaceId.eq(workspace_id))
        .exec(&db)
        .await
        .map_err(|e| {
            tracing::error!("disable_oxy_access failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    invalidate_access_cache();
    Ok(StatusCode::NO_CONTENT)
}
