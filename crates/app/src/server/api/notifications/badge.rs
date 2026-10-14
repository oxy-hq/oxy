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

use super::handlers::ReadAllQuery;

#[derive(Debug, Serialize)]
pub struct UnreadResponse {
    pub unread: u64,
}

/// `GET /api/notifications/unread` — Oxy's database only.
#[instrument(skip_all)]
pub async fn unread(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Query(q): Query<ReadAllQuery>,
) -> Result<Json<UnreadResponse>, StatusCode> {
    let db = establish_connection()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let mut find = notifications::Entity::find()
        .filter(notifications::Column::UserId.eq(user.id))
        .filter(notifications::Column::ReadAt.is_null());
    if let Some(org) = q.org_id {
        find = find.filter(notifications::Column::OrgId.eq(org));
    }
    let blocked = user.blocked_orgs();
    if !blocked.is_empty() {
        find = find.filter(notifications::Column::OrgId.is_not_in(blocked.iter().copied()));
    }
    let unread = find
        .count(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(UnreadResponse { unread }))
}
