//! Admin endpoints for managing feature flags.
//!
//! Mounted at `/admin/feature-flags`. The `/admin/*` tree is gated by
//! `oxy_owner_guard_middleware`, so handlers here assume the caller is an
//! Oxy owner.

use axum::Json;
use axum::Router;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{cache, registry, store};
use crate::server::router::AppState;

#[derive(Serialize)]
pub struct FeatureFlagDto {
    pub key: &'static str,
    pub description: &'static str,
    pub default: bool,
    pub enabled: bool,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
pub struct UpdateBody {
    pub enabled: bool,
}

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/feature-flags", get(list))
        .route("/feature-flags/{key}", patch(update))
}

async fn list() -> Result<Json<Vec<FeatureFlagDto>>, Response> {
    let db = oxy::database::client::establish_connection()
        .await
        .map_err(|e| {
            tracing::error!(?e, "feature_flags list: db connect failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        })?;
    let rows = store::fetch_all(&db).await.map_err(|e| {
        tracing::error!(?e, "feature_flags list: db fetch failed");
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    })?;

    let mut out = Vec::with_capacity(registry::FLAGS.len());
    for flag in registry::FLAGS {
        let row = rows.iter().find(|r| r.key == flag.key);
        let enabled = row.map(|r| r.enabled).unwrap_or(flag.default_enabled);
        let updated_at = row.map(|r| r.updated_at.with_timezone(&Utc));
        out.push(FeatureFlagDto {
            key: flag.key,
            description: flag.description,
            default: flag.default_enabled,
            enabled,
            updated_at,
        });
    }
    Ok(Json(out))
}

async fn update(
    Path(key): Path<String>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<FeatureFlagDto>, Response> {
    let Some(flag) = registry::get(&key) else {
        return Err((StatusCode::NOT_FOUND, "unknown feature flag").into_response());
    };

    let db = oxy::database::client::establish_connection()
        .await
        .map_err(|e| {
            tracing::error!(?e, "feature_flags update: db connect failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        })?;
    let row = store::upsert(&db, flag.key, body.enabled)
        .await
        .map_err(|e| {
            tracing::error!(?e, key = %flag.key, "feature_flags update: db upsert failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        })?;

    cache::set(flag.key, row.enabled);

    Ok(Json(FeatureFlagDto {
        key: flag.key,
        description: flag.description,
        default: flag.default_enabled,
        enabled: row.enabled,
        updated_at: Some(row.updated_at.with_timezone(&Utc)),
    }))
}
