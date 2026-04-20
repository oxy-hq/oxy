//! Secret routes and the admin-only middleware that gates them.
//!
//! Kept in its own module because the gating middleware is non-trivial —
//! it reconciles DB-granted roles, `OXY_OWNER`, the local-guest override,
//! and org-scoped org-admin membership.

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::{delete, get, post, put};

use oxy_auth::types::AuthenticatedUser;

use crate::api::secrets;

use super::AppState;

pub(super) fn build_secret_routes(app_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(secrets::list_secrets))
        .route("/", post(secrets::create_secret))
        .route("/bulk", post(secrets::bulk_create_secrets))
        .route("/env", get(secrets::list_env_secrets))
        .route("/{id}", get(secrets::get_secret))
        .route("/{id}", put(secrets::update_secret))
        .route("/{id}", delete(secrets::delete_secret))
        .route("/{id}/value", get(secrets::reveal_secret))
        .layer(middleware::from_fn_with_state(
            app_state,
            secrets_access_middleware,
        ))
}

/// Gates secrets routes to admin users only.
///
/// Checks `OXY_OWNER`; auto-grants access when unset
/// (permissive default for single-user local installs). The built-in local guest
/// (`<local-user@example.com>`) always passes.
async fn secrets_access_middleware(
    State(_app_state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let user = request
        .extensions()
        .get::<AuthenticatedUser>()
        .cloned()
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // OXY_OWNER grants local admin access. Org-scoped admin checks happen below.
    if oxy_auth::is_local_admin_from_env(&user.email) {
        return Ok(next.run(request).await);
    }

    // For multi-tenant mode, check org-scoped admin role via the workspace's org.
    let workspace = request
        .extensions()
        .get::<entity::workspaces::Model>()
        .cloned();

    if let Some(ws) = workspace {
        if let Some(org_id) = ws.org_id {
            use entity::org_members::{Column as OmCol, OrgRole};
            use entity::prelude::OrgMembers;
            use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

            let db = oxy::database::client::establish_connection()
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let membership = OrgMembers::find()
                .filter(OmCol::OrgId.eq(org_id))
                .filter(OmCol::UserId.eq(user.id))
                .one(&db)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            match membership {
                Some(m) if matches!(m.role, OrgRole::Owner | OrgRole::Admin) => {
                    return Ok(next.run(request).await);
                }
                _ => {
                    tracing::warn!("Non-admin user {} attempted to access secrets", user.email);
                    return Err(StatusCode::FORBIDDEN);
                }
            }
        }
    }

    // No workspace or no org_id (legacy mode) — fall back to local admin check.
    tracing::warn!("Non-admin user {} attempted to access secrets", user.email);
    Err(StatusCode::FORBIDDEN)
}
