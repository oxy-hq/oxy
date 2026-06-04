//! Workspace-scoped UniFi API-key storage.
//!
//! Provides a small wrapper over `OrgSecretsService` so the UniFi
//! preview / import / resync endpoints can read the operator's API
//! key from server-side storage instead of asking for it in every
//! request body. The plaintext itself never sits in this crate's
//! tables — `org_secrets` carries the AES-256-GCM blob and key
//! version; we just track (workspace_id → secret_id) + audit fields.
//!
//! ## Why workspace-scoped (not org-scoped)
//!
//! Operators routinely run multiple customer demos in different
//! workspaces in the same org. Each customer's UniFi account is its
//! own surface, so a per-workspace key isolates "I accidentally
//! resynced the wrong customer" risk and keeps the cred lifecycle
//! tied to the workspace it was set in.
//!
//! ## Backward-compat surface
//!
//! Existing endpoints accept an optional `api_key` in the request
//! body. `resolve_api_key(workspace_id, body_key)` returns the body
//! key when present, else the stored key, else an InvalidInput so
//! the caller can surface a clean "configure UniFi credentials"
//! message in the UI.

use crate::entities::unifi_credentials;
use crate::service::{ServiceError, ServiceResult};
use entity::workspaces;
use oxy_platform::secrets::OrgSecretsService;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
};
use serde::Serialize;
use uuid::Uuid;

const SECRET_NAME_PREFIX: &str = "unifi_api_key";

/// Public status surface returned by `GET /:wid/cameras/unifi-credentials/status`.
/// Deliberately omits the secret_id / org_id — the UI only needs to
/// know whether a key is configured and when it was last touched.
#[derive(Debug, Clone, Serialize)]
pub struct UnifiCredentialStatus {
    pub configured: bool,
    pub set_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub updated_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub set_by_user_id: Option<Uuid>,
}

/// Upsert the UniFi API key for a workspace. The plaintext is
/// encrypted via `OrgSecretsService::upsert` and persisted to
/// `org_secrets`; this row only stores the lookup.
pub async fn set_api_key(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    api_key: &str,
    actor_user_id: Option<Uuid>,
) -> ServiceResult<UnifiCredentialStatus> {
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        return Err(ServiceError::InvalidInput(
            "api_key may not be empty — DELETE the credential to clear".into(),
        ));
    }
    let org_id = resolve_org_id(db, workspace_id).await?;

    // Upsert into org_secrets. The secret name embeds workspace_id
    // so multiple workspaces in the same org get independent keys
    // even though they share one OrgSecrets pool.
    let secret_id = OrgSecretsService::upsert(
        org_id,
        &format!("{SECRET_NAME_PREFIX}:{workspace_id}"),
        trimmed,
    )
    .await
    .map_err(|e| ServiceError::Internal(format!("encrypt + store UniFi key: {e}")))?;

    let now = chrono::Utc::now().fixed_offset();
    let existing = unifi_credentials::Entity::find()
        .filter(unifi_credentials::Column::WorkspaceId.eq(workspace_id))
        .one(db)
        .await?;
    let row = if let Some(row) = existing {
        let mut am: unifi_credentials::ActiveModel = row.into();
        am.secret_id = Set(secret_id);
        am.set_by_user_id = Set(actor_user_id);
        am.updated_at = Set(now);
        am.update(db).await?
    } else {
        unifi_credentials::ActiveModel {
            id: Set(Uuid::new_v4()),
            workspace_id: Set(workspace_id),
            org_id: Set(org_id),
            secret_id: Set(secret_id),
            set_by_user_id: Set(actor_user_id),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await?
    };

    Ok(UnifiCredentialStatus {
        configured: true,
        set_at: Some(row.created_at),
        updated_at: Some(row.updated_at),
        set_by_user_id: row.set_by_user_id,
    })
}

/// Returns the plaintext API key for this workspace, or `None` when
/// no credential is configured. Errors surface only on storage I/O
/// or decryption failures — "not configured" is a normal state and
/// is the resting condition for fresh workspaces.
pub async fn get_api_key(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> ServiceResult<Option<String>> {
    let Some(row) = unifi_credentials::Entity::find()
        .filter(unifi_credentials::Column::WorkspaceId.eq(workspace_id))
        .one(db)
        .await?
    else {
        return Ok(None);
    };
    let plaintext = OrgSecretsService::get_by_id(row.secret_id)
        .await
        .map_err(|e| ServiceError::Internal(format!("decrypt UniFi key: {e}")))?;
    Ok(Some(plaintext))
}

/// Resolve which API key the caller should use. Body wins so the
/// import preview / first-time setup flows keep working before the
/// credential is stored; otherwise fall back to storage. NotConfigured
/// errors translate to InvalidInput so the route layer returns 400
/// with a clear "set the UniFi key first" message.
pub async fn resolve_api_key(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    body_key: Option<String>,
) -> ServiceResult<String> {
    if let Some(k) = body_key.and_then(|s| {
        let trimmed = s.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }) {
        return Ok(k);
    }
    match get_api_key(db, workspace_id).await? {
        Some(k) => Ok(k),
        None => Err(ServiceError::InvalidInput(
            "no UniFi credential configured for this workspace — PUT \
             /cameras/unifi-credentials with an api_key first, or include \
             api_key in this request body"
                .into(),
        )),
    }
}

/// Drop the workspace's UniFi credential. Deletes both the lookup row
/// and the underlying `org_secrets` blob so an exfiltrated DB doesn't
/// carry the key forever after a revoke. No-op when no row exists.
pub async fn delete_api_key(db: &DatabaseConnection, workspace_id: Uuid) -> ServiceResult<()> {
    let Some(row) = unifi_credentials::Entity::find()
        .filter(unifi_credentials::Column::WorkspaceId.eq(workspace_id))
        .one(db)
        .await?
    else {
        return Ok(());
    };
    let (id, secret_id) = (row.id, row.secret_id);
    // Delete the lookup first — if the secrets-store delete fails
    // afterward, we leave a dangling `org_secrets` row but never an
    // orphan `unifi_credentials` row pointing at a deleted secret.
    unifi_credentials::Entity::delete_by_id(id).exec(db).await?;
    OrgSecretsService::delete(secret_id)
        .await
        .map_err(|e| ServiceError::Internal(format!("delete UniFi secret: {e}")))?;
    Ok(())
}

/// Status surface for the UI — never returns the secret itself.
pub async fn get_status(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> ServiceResult<UnifiCredentialStatus> {
    let row = unifi_credentials::Entity::find()
        .filter(unifi_credentials::Column::WorkspaceId.eq(workspace_id))
        .one(db)
        .await?;
    Ok(match row {
        Some(r) => UnifiCredentialStatus {
            configured: true,
            set_at: Some(r.created_at),
            updated_at: Some(r.updated_at),
            set_by_user_id: r.set_by_user_id,
        },
        None => UnifiCredentialStatus {
            configured: false,
            set_at: None,
            updated_at: None,
            set_by_user_id: None,
        },
    })
}

/// Resolve the org_id for a workspace_id. Same pattern as
/// `service::alerts::resolve_slack_destination` — we read from the
/// workspaces aggregate via SeaORM rather than depending on a
/// platform service so the cross-aggregate touch is one SELECT.
async fn resolve_org_id(db: &DatabaseConnection, workspace_id: Uuid) -> ServiceResult<Uuid> {
    let ws = workspaces::Entity::find_by_id(workspace_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    ws.org_id.ok_or_else(|| {
        ServiceError::InvalidInput(
            "workspace has no parent org — UniFi credential storage requires multi-workspace mode"
                .into(),
        )
    })
}
