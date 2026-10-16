//! The badge: how many unread notifications a person has.
//!
//! Separate from the inbox page on purpose. The page reads each org's store
//! for what its notifications say; the badge is polled, and a poll must not
//! wake a scale-to-zero tenant database every minute to learn a number Oxy's
//! own database already has.

use axum::Json;
use axum::extract::Query;
use axum::http::StatusCode;
use entity::notifications;
use oxy::database::client::establish_connection;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use serde::Serialize;
use tracing::instrument;

use super::filter::FilterParams;

#[derive(Debug, Serialize)]
pub struct UnreadResponse {
    pub unread: u64,
}

/// `GET /api/notifications/unread` — Oxy's database only.
#[instrument(skip_all)]
pub async fn unread(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Query(q): Query<FilterParams>,
) -> Result<Json<UnreadResponse>, StatusCode> {
    let db = establish_connection()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    // The filter reads catalogues (Oxy's database) for a category, never an
    // org's store.
    let find = q
        .resolve(&db)
        .await?
        .apply(notifications::Entity::find(), &user)
        .filter(notifications::Column::ReadAt.is_null());
    let unread = find
        .count(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(UnreadResponse { unread }))
}
