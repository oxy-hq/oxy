use axum::extract::Path;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use chrono::Utc;
use entity::prelude::*;
use oxy::database::client::establish_connection;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

// `OrgContext` + its extension-reading extractor moved to `oxy-server-authz`
// (state-agnostic authz context, consumed by the role guards that also moved).
// `org_middleware` below still loads and inserts it; re-exported here so the
// original `middlewares::org_context::{OrgContext, OrgContextExtractor}` paths
// keep resolving.
pub use oxy_server_authz::org_context::{OrgContext, OrgContextExtractor};

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
            // Being staff is NOT enough. The operator must have deliberately
            // started an assume-role session for THIS org — otherwise they are a
            // plain non-member and get a 403 like anyone else. This is what turns
            // the old silent, unbounded, unlogged override into an explicit,
            // bounded, audited one. See `api::admin::assume`.
            //
            // TWO populations can act: Oxy staff (any org) and a partner (an
            // assigned client, with `develop_apps`). `may_act_as` decides which —
            // and it is re-checked here on every request, so revoking a partner's
            // data-plane capability kills a live session's reach at once rather
            // than at expiry.
            use crate::server::api::admin::assume;
            let live = assume::is_session_live(&db, user.id, org_id).await;
            let authority = if live {
                assume::may_act_as(&db, user.id, &user.email, org_id).await
            } else {
                None
            };

            if let Some(authority) = authority {
                let now = Utc::now().into();
                let role = authority.org_role();
                let role_label = role.as_str();
                let synth = entity::org_members::Model {
                    id: Uuid::nil(),
                    org_id,
                    user_id: user.id,
                    role,
                    created_at: now,
                    updated_at: now,
                };
                tracing::info!(
                    actor_email = %user.email,
                    org_id = %org_id,
                    ?authority,
                    role = %role_label,
                    "org_context: assume-role session active"
                );
                (synth, true)
            } else {
                if live {
                    tracing::warn!(
                        actor_email = %user.email,
                        org_id = %org_id,
                        "org_context: live session but no authority — denying (capability revoked?)"
                    );
                }
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
