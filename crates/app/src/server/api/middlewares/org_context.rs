use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use entity::prelude::*;
use oxy::database::client::establish_connection;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use std::future::Future;
use uuid::Uuid;

#[derive(Clone)]
pub struct OrgContext {
    pub org: entity::organizations::Model,
    pub membership: entity::org_members::Model,
}

pub struct OrgContextExtractor(pub OrgContext);

impl<S> FromRequestParts<S> for OrgContextExtractor
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let result = parts
            .extensions
            .get::<OrgContext>()
            .cloned()
            .map(OrgContextExtractor)
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR);

        async move { result }
    }
}

#[derive(serde::Deserialize)]
pub struct OrgPath {
    org_id: Uuid,
}

pub async fn org_middleware(
    Path(OrgPath { org_id }): Path<OrgPath>,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish DB connection: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let org = Organizations::find_by_id(org_id)
        .one(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query organization: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let membership = OrgMembers::find()
        .filter(entity::org_members::Column::OrgId.eq(org_id))
        .filter(entity::org_members::Column::UserId.eq(user.id))
        .one(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query org membership: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::FORBIDDEN)?;

    request
        .extensions_mut()
        .insert(OrgContext { org, membership });

    Ok(next.run(request).await)
}
