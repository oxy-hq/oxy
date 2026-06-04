use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use chrono::Utc;
use entity::prelude::*;
use oxy::database::client::establish_connection;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use std::future::Future;
use uuid::Uuid;

use crate::server::api::middlewares::oxy_app_admin_guard::is_oxy_app_admin;
use crate::server::api::middlewares::oxy_owner_guard::is_oxy_owner;

#[derive(Clone, Debug)]
pub struct OrgContext {
    pub org: entity::organizations::Model,
    pub membership: entity::org_members::Model,
    /// `true` when `membership` is a synthetic Owner row injected because
    /// the caller is a Global Owner or Global Admin but NOT a real member
    /// of `org`. Per-org handlers that need to stay strictly within
    /// real-membership semantics (e.g. self-serve billing operations on a
    /// foreign org) should check this and reject. Defaults to `false`.
    pub is_global_override: bool,
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

    let real_membership = OrgMembers::find()
        .filter(entity::org_members::Column::OrgId.eq(org_id))
        .filter(entity::org_members::Column::UserId.eq(user.id))
        .one(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query org membership: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let (membership, is_global_override) = match real_membership {
        Some(m) => (m, false),
        None => {
            // Not a real member — see if the caller is a platform-level
            // operator (Global Owner via OXY_OWNER, Global Admin via the
            // `app_admins` table). If so, synthesize an Owner membership
            // so they can support / triage tenants without being added as
            // a real member. Per-org handlers that must stay
            // member-restricted check `is_global_override` and 403.
            if is_oxy_owner(&user.email) || is_oxy_app_admin(&user.email).await {
                let now = Utc::now().into();
                let synth = entity::org_members::Model {
                    id: Uuid::nil(),
                    org_id,
                    user_id: user.id,
                    role: entity::org_members::OrgRole::Owner,
                    created_at: now,
                    updated_at: now,
                };
                tracing::info!(
                    admin_email = %user.email,
                    org_id = %org_id,
                    "org_context: global override granted (no real membership; user is Global Owner or Global Admin)"
                );
                (synth, true)
            } else {
                return Err(StatusCode::FORBIDDEN);
            }
        }
    };

    request.extensions_mut().insert(OrgContext {
        org,
        membership,
        is_global_override,
    });

    Ok(next.run(request).await)
}
