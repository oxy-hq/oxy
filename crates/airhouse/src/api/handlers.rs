//! Per-user Airhouse credential access.
//!
//! - `GET  /airhouse/me/connection`  — connection coordinates, no password.
//! - `GET  /airhouse/me/credentials` — coordinates + the password (decrypted
//!    on every call from `org_secrets`; `password_revealed_at` becomes a
//!    "last revealed" audit timestamp rather than a one-shot gate).
//! - `POST /airhouse/me/provision`   — explicit user-triggered provisioning.
//!   Body: `{ "tenant_name": "<name>" }` (required on first provision; ignored
//!   on re-provision when a local row already exists).
//! - `POST /airhouse/me/rotate-password` — generate a new password, replace
//!    the Airhouse user, and update the local secret.

use axum::extract::{Json, Query};
use axum::http::StatusCode;
use chrono::Utc;
use entity::org_members;
use entity::prelude::{OrgMembers, Workspaces};
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_platform::db::establish_connection;
use oxy_platform::secrets::OrgSecretsService;
use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use tracing::{error, instrument, warn};
use uuid::Uuid;

use crate::config::{provisioner_for, user_provisioner_for, wire_endpoint};
use crate::entity::users::{self as airhouse_users, AirhouseUserStatus};
use crate::entity::{Tenants as AirhouseTenants, Users as AirhouseUsers};
use crate::provisioner::ProvisionerError;
use crate::user_provisioner::UserProvisionerError;

#[derive(Deserialize)]
pub struct CredentialsQuery {
    pub workspace_id: Uuid,
}

#[derive(Deserialize)]
pub struct ProvisionBody {
    pub tenant_name: String,
}

#[derive(Serialize)]
pub struct ConnectionInfoResponse {
    pub host: String,
    pub port: u16,
    pub dbname: String,
    pub username: String,
    /// `true` when the password has not yet been surfaced via the reveal
    /// endpoint (i.e. `password_revealed_at IS NULL`). The password is always
    /// retrievable via `/credentials` regardless of this flag — it stays in
    /// `org_secrets` so rotation and `airhouse_managed` keep working. The UI
    /// uses this to show "first reveal" cues.
    pub password_not_yet_shown: bool,
}

/// Thin handler that returns the user-facing wire-protocol coordinates for the
/// caller.
///
/// - 503 when Airhouse is disabled at the deployment level (no env vars).
/// - 404 when Airhouse is enabled but the caller has no provisioned record
///   in this workspace yet — they should hit `POST /airhouse/me/provision` first.
#[instrument(skip(user, query), fields(user_id = %user.id, workspace_id = %query.workspace_id))]
pub async fn get_connection(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Query(query): Query<CredentialsQuery>,
) -> Result<Json<ConnectionInfoResponse>, StatusCode> {
    let endpoint = wire_endpoint().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let db = establish_connection().await.map_err(|e| {
        error!("DB connection error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let local = AirhouseUsers::find()
        .filter(airhouse_users::Column::WorkspaceId.eq(query.workspace_id))
        .filter(airhouse_users::Column::OxyUserId.eq(user.id))
        .one(&db)
        .await
        .map_err(|e| {
            error!("Failed to fetch local airhouse user: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if local.status != AirhouseUserStatus::Active {
        return Err(StatusCode::NOT_FOUND);
    }

    let tenant = AirhouseTenants::find_by_id(local.tenant_row_id)
        .one(&db)
        .await
        .map_err(|e| {
            error!("Failed to fetch tenant: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(ConnectionInfoResponse {
        host: endpoint.host,
        port: endpoint.port,
        dbname: tenant.airhouse_tenant_id,
        username: local.username,
        password_not_yet_shown: local.password_secret_id.is_some()
            && local.password_revealed_at.is_none(),
    }))
}

#[derive(Serialize)]
pub struct CredentialsResponse {
    pub username: String,
    pub host: String,
    pub port: u16,
    pub dbname: String,
    pub role: String,
    pub status: String,
    /// Plaintext password — present **only on the first call** after provision.
    /// Once shown, the secret is deleted and `password_already_revealed` flips
    /// to true on subsequent calls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    pub password_already_revealed: bool,
}

/// Reveal the caller's Airhouse password. The secret is decrypted and
/// returned on every successful call — passwords stay in `org_secrets` so
/// that rotation, the `airhouse_managed` connector type, and any future
/// programmatic use can keep working. `password_revealed_at` is updated as
/// an audit timestamp ("last revealed at"); it does not gate access.
#[instrument(skip(user, query), fields(user_id = %user.id, workspace_id = %query.workspace_id))]
pub async fn get_credentials(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Query(query): Query<CredentialsQuery>,
) -> Result<Json<CredentialsResponse>, StatusCode> {
    let endpoint = wire_endpoint().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let db = establish_connection().await.map_err(|e| {
        error!("DB connection error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let local = AirhouseUsers::find()
        .filter(airhouse_users::Column::WorkspaceId.eq(query.workspace_id))
        .filter(airhouse_users::Column::OxyUserId.eq(user.id))
        .one(&db)
        .await
        .map_err(|e| {
            error!("Failed to fetch local airhouse user: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if local.status != AirhouseUserStatus::Active {
        return Err(StatusCode::CONFLICT);
    }

    let tenant = AirhouseTenants::find_by_id(local.tenant_row_id)
        .one(&db)
        .await
        .map_err(|e| {
            error!("Failed to fetch tenant: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let secret_id = local
        .password_secret_id
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let pw = OrgSecretsService::get_by_id(secret_id).await.map_err(|e| {
        error!("Failed to decrypt airhouse password secret: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let local_id = local.id;
    let already_revealed = local.password_revealed_at.is_some();
    let mut active: airhouse_users::ActiveModel = local.clone().into();
    active.password_revealed_at = ActiveValue::Set(Some(Utc::now().fixed_offset()));
    if let Err(e) = active.update(&db).await {
        // Audit-stamp failure is non-fatal — surface a warning but still
        // return the password the caller asked for.
        tracing::warn!(local_id = %local_id, "failed to update password_revealed_at: {e}");
    }

    Ok(Json(CredentialsResponse {
        username: local.username,
        host: endpoint.host,
        port: endpoint.port,
        dbname: tenant.airhouse_tenant_id,
        role: local.role.as_str().to_string(),
        status: status_str(&local.status).to_string(),
        password: Some(pw),
        // True iff the password had been revealed BEFORE this call.
        password_already_revealed: already_revealed,
    }))
}

fn status_str(s: &AirhouseUserStatus) -> &'static str {
    match s {
        AirhouseUserStatus::Active => "active",
        AirhouseUserStatus::Failed => "failed",
        AirhouseUserStatus::PendingDelete => "pending_delete",
    }
}

/// Explicit, user-triggered provisioning. Idempotent: re-running for an
/// already-provisioned `(workspace, user)` returns the existing connection info
/// without contacting Airhouse.
///
/// - 503 when Airhouse is disabled at the deployment level.
/// - 403 when the caller is not a member of the org that owns this workspace.
/// - 422 when `tenant_name` fails validation.
/// - 502 when the Airhouse Admin API is reachable but rejects the request.
#[instrument(skip(user, query, body), fields(user_id = %user.id, workspace_id = %query.workspace_id))]
pub async fn provision(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Query(query): Query<CredentialsQuery>,
    Json(body): Json<ProvisionBody>,
) -> Result<Json<ConnectionInfoResponse>, StatusCode> {
    let endpoint = wire_endpoint().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let db = establish_connection().await.map_err(|e| {
        error!("DB connection error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Resolve the org_id for membership check. The local nil workspace is not
    // in the DB — treat it as belonging to the local nil org (seeded at startup).
    let (org_id, org_role) = if query.workspace_id.is_nil() {
        (Uuid::nil(), entity::org_members::OrgRole::Owner)
    } else {
        let workspace = Workspaces::find_by_id(query.workspace_id)
            .one(&db)
            .await
            .map_err(|e| {
                error!("Failed to query workspace: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .ok_or(StatusCode::NOT_FOUND)?;

        let org_id = workspace.org_id.ok_or(StatusCode::FORBIDDEN)?;

        // Caller must actually be a member of the org. Their role determines
        // the Airhouse user role (Owner→admin, Admin→writer, Member→reader).
        let membership = OrgMembers::find()
            .filter(org_members::Column::OrgId.eq(org_id))
            .filter(org_members::Column::UserId.eq(user.id))
            .one(&db)
            .await
            .map_err(|e| {
                error!("Failed to query membership: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .ok_or(StatusCode::FORBIDDEN)?;

        (org_id, membership.role.clone())
    };

    let tenant_prov = provisioner_for(db.clone()).ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let user_prov = user_provisioner_for(db.clone()).ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    tenant_prov
        .provision(query.workspace_id, body.tenant_name)
        .await
        .map_err(|e| match e {
            ProvisionerError::InvalidTenantName(_) => StatusCode::UNPROCESSABLE_ENTITY,
            other => {
                warn!(workspace_id = %query.workspace_id, "tenant provisioning failed: {other}");
                StatusCode::BAD_GATEWAY
            }
        })?;

    let provisioned = user_prov
        .provision_with_org_id(user.id, query.workspace_id, org_id, org_role)
        .await
        .map_err(|e| {
            warn!(
                workspace_id = %query.workspace_id,
                user_id = %user.id,
                "user provisioning failed: {e}"
            );
            StatusCode::BAD_GATEWAY
        })?;

    Ok(Json(ConnectionInfoResponse {
        host: endpoint.host,
        port: endpoint.port,
        dbname: provisioned.tenant_id,
        username: provisioned.username,
        password_not_yet_shown: provisioned.password_not_yet_shown,
    }))
}

/// Generate a new password for the caller's Airhouse user. Calls Airhouse to
/// delete and recreate the user (the Admin API has no password-update),
/// replaces the secret in `org_secrets`, and clears `password_revealed_at`.
///
/// - 503 when Airhouse is disabled at the deployment level.
/// - 404 when the caller has no provisioned record.
/// - 502 when the Airhouse Admin API rejects the request.
#[instrument(skip(user, query), fields(user_id = %user.id, workspace_id = %query.workspace_id))]
pub async fn rotate_password(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Query(query): Query<CredentialsQuery>,
) -> Result<Json<ConnectionInfoResponse>, StatusCode> {
    let endpoint = wire_endpoint().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let db = establish_connection().await.map_err(|e| {
        error!("DB connection error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let user_prov = user_provisioner_for(db.clone()).ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let provisioned = user_prov
        .rotate_password(user.id, query.workspace_id)
        .await
        .map_err(|e| match e {
            UserProvisionerError::NotProvisioned => StatusCode::NOT_FOUND,
            other => {
                warn!(
                    workspace_id = %query.workspace_id,
                    user_id = %user.id,
                    "password rotation failed: {other}"
                );
                StatusCode::BAD_GATEWAY
            }
        })?;

    Ok(Json(ConnectionInfoResponse {
        host: endpoint.host,
        port: endpoint.port,
        dbname: provisioned.tenant_id,
        username: provisioned.username,
        // Rotation cleared password_revealed_at — now NULL.
        password_not_yet_shown: provisioned.password_not_yet_shown,
    }))
}
