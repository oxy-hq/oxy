//! `/api/admin/app-admins` — OXY_OWNER-managed global "app admin" role.
//!
//! Sits behind the `oxy_owner_guard` middleware so only Oxy staff
//! (members of the `OXY_OWNER` email allow-list) can add or remove
//! global app admins. The role itself grants access to the
//! customer-apps surface plus every registered custom app, replacing
//! the legacy `OXY_APP_ADMINS` env var.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get};
use entity::app_admins;
use entity::prelude::AppAdmins;
use oxy::database::client::establish_connection;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::server::authz::globals::invalidate_admin_cache;
use crate::server::router::AppState;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/app-admins", get(list_app_admins).post(create_app_admin))
        .route("/app-admins/{id}", delete(delete_app_admin))
}

#[derive(Serialize)]
pub struct AppAdminResponse {
    pub id: Uuid,
    pub email: String,
    pub granted_by: Option<Uuid>,
    pub created_at: String,
}

impl From<app_admins::Model> for AppAdminResponse {
    fn from(m: app_admins::Model) -> Self {
        Self {
            id: m.id,
            email: m.email,
            granted_by: m.granted_by,
            created_at: m.created_at.to_rfc3339(),
        }
    }
}

pub async fn list_app_admins() -> Result<Json<Vec<AppAdminResponse>>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("list_app_admins DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let rows = AppAdmins::find()
        .order_by_asc(app_admins::Column::Email)
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("list_app_admins query failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[derive(Deserialize)]
pub struct CreateAppAdminBody {
    pub email: String,
}

pub async fn create_app_admin(
    State(_): State<AppState>,
    AuthenticatedUserExtractor(actor): AuthenticatedUserExtractor,
    Json(body): Json<CreateAppAdminBody>,
) -> Result<Json<AppAdminResponse>, StatusCode> {
    let email = body.email.trim().to_ascii_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(StatusCode::BAD_REQUEST);
    }

    let db = establish_connection().await.map_err(|e| {
        tracing::error!("create_app_admin DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if let Some(existing) = AppAdmins::find()
        .filter(app_admins::Column::Email.eq(email.clone()))
        .one(&db)
        .await
        .map_err(|e| {
            tracing::error!("create_app_admin existence check failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    {
        return Ok(Json(existing.into()));
    }

    let model = app_admins::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        email: ActiveValue::Set(email),
        granted_by: ActiveValue::Set(Some(actor.id)),
        created_at: ActiveValue::NotSet,
    }
    .insert(&db)
    .await
    .map_err(|e| {
        tracing::error!("create_app_admin insert failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    invalidate_admin_cache();
    Ok(Json(model.into()))
}

pub async fn delete_app_admin(Path(id): Path<Uuid>) -> Result<StatusCode, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("delete_app_admin DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let res = AppAdmins::delete_by_id(id).exec(&db).await.map_err(|e| {
        tracing::error!("delete_app_admin failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if res.rows_affected == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    invalidate_admin_cache();
    Ok(StatusCode::NO_CONTENT)
}
