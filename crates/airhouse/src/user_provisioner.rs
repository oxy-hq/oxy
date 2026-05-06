use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Utc;
use entity::org_members::OrgRole;
use entity::prelude::Workspaces;
use entity::users;
use oxy_platform::secrets::OrgSecretsService;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use thiserror::Error;
use tracing::{info, instrument, warn};
use uuid::Uuid;

use crate::admin::{AirhouseAdminClient, AirhouseError, UserRecord, UserRole};
use crate::entity::tenants as airhouse_tenants;
use crate::entity::users::{self as airhouse_users, AirhouseUserRole, AirhouseUserStatus};
use crate::entity::{Tenants as AirhouseTenants, Users as AirhouseUsers};

const MAX_USERNAME_LEN: usize = 63;
const PASSWORD_BYTES: usize = 32;

#[derive(Debug, Error)]
pub enum UserProvisionerError {
    #[error("oxy user {0} not found")]
    OxyUserNotFound(Uuid),
    #[error("airhouse tenant for workspace {0} not found — provision the tenant first")]
    TenantNotFound(Uuid),
    #[error("workspace {0} not found")]
    WorkspaceNotFound(Uuid),
    #[error("airhouse user has not been provisioned")]
    NotProvisioned,
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
    #[error("airhouse error: {0}")]
    Airhouse(#[from] AirhouseError),
    #[error("secret store error: {0}")]
    Secret(String),
}

pub struct UserProvisioner {
    db: DatabaseConnection,
    client: AirhouseAdminClient,
}

/// Result of `provision`. The local row is always written when this succeeds.
pub struct ProvisionedUser {
    pub airhouse_user_id: Uuid,
    pub username: String,
    pub role: AirhouseUserRole,
    pub tenant_id: String,
    /// `true` if the password has never been surfaced via the reveal endpoint
    /// (i.e. `password_revealed_at IS NULL` on the local row). The password is
    /// always retrievable; this flag is just used to drive UI cues like
    /// "first reveal" badges.
    pub password_not_yet_shown: bool,
}

impl UserProvisioner {
    pub fn new(db: DatabaseConnection, client: AirhouseAdminClient) -> Self {
        Self { db, client }
    }

    /// Idempotent: re-running for the same `(oxy_user_id, workspace_id)` returns
    /// the existing local row without contacting Airhouse.
    ///
    /// Resolves `org_id` from the workspace internally. Handlers that already
    /// have a resolved `org_id` should call [`Self::provision_with_org_id`]
    /// directly to avoid a redundant workspace lookup.
    pub async fn provision(
        &self,
        oxy_user_id: Uuid,
        workspace_id: Uuid,
        oxy_role: OrgRole,
    ) -> Result<ProvisionedUser, UserProvisionerError> {
        let org_id = self.resolve_org_id(workspace_id).await?;
        self.provision_with_org_id(oxy_user_id, workspace_id, org_id, oxy_role)
            .await
    }

    /// Same as [`Self::provision`] but accepts a pre-resolved `org_id`.
    #[instrument(skip(self), fields(workspace_id = %workspace_id, org_id = %org_id, oxy_user_id = %oxy_user_id))]
    pub async fn provision_with_org_id(
        &self,
        oxy_user_id: Uuid,
        workspace_id: Uuid,
        org_id: Uuid,
        oxy_role: OrgRole,
    ) -> Result<ProvisionedUser, UserProvisionerError> {
        if let Some(existing) = AirhouseUsers::find()
            .filter(airhouse_users::Column::WorkspaceId.eq(workspace_id))
            .filter(airhouse_users::Column::OxyUserId.eq(oxy_user_id))
            .one(&self.db)
            .await?
        {
            if existing.status == AirhouseUserStatus::Active {
                let password_not_yet_shown = existing.password_secret_id.is_some()
                    && existing.password_revealed_at.is_none();
                return Ok(ProvisionedUser {
                    airhouse_user_id: existing.id,
                    username: existing.username,
                    role: existing.role,
                    tenant_id: load_tenant_id(&self.db, existing.tenant_row_id).await?,
                    password_not_yet_shown,
                });
            }
            // Failed row — delete it so we can retry cleanly below.
            AirhouseUsers::delete_by_id(existing.id)
                .exec(&self.db)
                .await?;
        }

        let tenant = AirhouseTenants::find()
            .filter(airhouse_tenants::Column::WorkspaceId.eq(workspace_id))
            .one(&self.db)
            .await?
            .ok_or(UserProvisionerError::TenantNotFound(workspace_id))?;

        let user = users::Entity::find_by_id(oxy_user_id)
            .one(&self.db)
            .await?
            .ok_or(UserProvisionerError::OxyUserNotFound(oxy_user_id))?;

        let username = derive_username(&user.email, oxy_user_id);
        let role = map_role(&oxy_role);
        let password = generate_password();

        info!(username = %username, role = ?role, tenant_id = %tenant.airhouse_tenant_id, "creating airhouse user");

        let create_result = self
            .client
            .create_user(
                &tenant.airhouse_tenant_id,
                &username,
                &password,
                airhouse_role_to_client(&role),
            )
            .await;

        let (remote, password) = match create_result {
            Ok(remote) => (remote, password),
            Err(AirhouseError::AlreadyExists(_)) => {
                // User exists in Airhouse (previous partial provision) — rotate
                // to a fresh password so we can store a known credential.
                warn!(
                    username = %username,
                    tenant_id = %tenant.airhouse_tenant_id,
                    "airhouse user already exists; deleting and recreating with fresh password"
                );
                self.client
                    .delete_user(&tenant.airhouse_tenant_id, &username)
                    .await?;
                let new_password = generate_password();
                let remote = self
                    .client
                    .create_user(
                        &tenant.airhouse_tenant_id,
                        &username,
                        &new_password,
                        airhouse_role_to_client(&role),
                    )
                    .await?;
                (remote, new_password)
            }
            Err(e) => {
                let _ = self
                    .insert_failed_row(workspace_id, oxy_user_id, &tenant, &username, role.clone())
                    .await;
                return Err(e.into());
            }
        };

        self.persist_success(
            workspace_id,
            org_id,
            oxy_user_id,
            &tenant,
            &remote,
            role.clone(),
            &password,
        )
        .await
        .map(|airhouse_user_id| ProvisionedUser {
            airhouse_user_id,
            username: remote.username,
            role,
            tenant_id: tenant.airhouse_tenant_id,
            // Freshly persisted: password_revealed_at is NULL.
            password_not_yet_shown: true,
        })
    }

    /// Generate a new password for an existing user. Airhouse has no
    /// password-update endpoint, so this is delete-then-create against the
    /// upstream API. The local row is preserved (same id, same username);
    /// only the secret is replaced and `password_revealed_at` is cleared.
    #[instrument(skip(self), fields(workspace_id = %workspace_id, oxy_user_id = %oxy_user_id))]
    pub async fn rotate_password(
        &self,
        oxy_user_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<ProvisionedUser, UserProvisionerError> {
        let local = AirhouseUsers::find()
            .filter(airhouse_users::Column::WorkspaceId.eq(workspace_id))
            .filter(airhouse_users::Column::OxyUserId.eq(oxy_user_id))
            .one(&self.db)
            .await?
            .ok_or(UserProvisionerError::NotProvisioned)?;
        let tenant = AirhouseTenants::find_by_id(local.tenant_row_id)
            .one(&self.db)
            .await?
            .ok_or(UserProvisionerError::TenantNotFound(workspace_id))?;

        let new_password = generate_password();
        info!(username = %local.username, "rotating airhouse user password");

        // Delete-then-create. The 404 on delete (already gone) is not an error
        // — `AirhouseAdminClient::delete_user` returns Ok(false) in that case.
        self.client
            .delete_user(&tenant.airhouse_tenant_id, &local.username)
            .await?;
        let remote = self
            .client
            .create_user(
                &tenant.airhouse_tenant_id,
                &local.username,
                &new_password,
                airhouse_role_to_client(&local.role),
            )
            .await?;

        // Replace the password in `org_secrets` (upsert by name) and clear
        // the audit timestamp so the UI shows "first reveal" cues again.
        let org_id = self.resolve_org_id(workspace_id).await?;
        let secret_name = secret_name_for(workspace_id, oxy_user_id);
        let secret_id = OrgSecretsService::upsert(org_id, &secret_name, &new_password)
            .await
            .map_err(|e| UserProvisionerError::Secret(e.to_string()))?;

        let local_id = local.id;
        let role_for_response = local.role.clone();
        let mut active: airhouse_users::ActiveModel = local.into();
        active.password_secret_id = ActiveValue::Set(Some(secret_id));
        active.password_revealed_at = ActiveValue::Set(None);
        active.update(&self.db).await?;

        Ok(ProvisionedUser {
            airhouse_user_id: local_id,
            username: remote.username,
            role: role_for_response,
            tenant_id: tenant.airhouse_tenant_id,
            // Rotation just cleared password_revealed_at.
            password_not_yet_shown: true,
        })
    }

    /// Idempotent: succeeds whether the local row or remote user exist.
    #[instrument(skip(self), fields(workspace_id = %workspace_id, oxy_user_id = %oxy_user_id))]
    pub async fn deprovision(
        &self,
        oxy_user_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<(), UserProvisionerError> {
        let local = AirhouseUsers::find()
            .filter(airhouse_users::Column::WorkspaceId.eq(workspace_id))
            .filter(airhouse_users::Column::OxyUserId.eq(oxy_user_id))
            .one(&self.db)
            .await?;
        let Some(local) = local else {
            info!("no local airhouse user row; deprovision is a no-op");
            return Ok(());
        };

        let tenant = AirhouseTenants::find_by_id(local.tenant_row_id)
            .one(&self.db)
            .await?;

        if let Some(tenant) = tenant {
            // Returns true on 204, false on 404 — both are fine for us.
            self.client
                .delete_user(&tenant.airhouse_tenant_id, &local.username)
                .await?;
        }

        if let Some(secret_id) = local.password_secret_id {
            // Best-effort: secret deletion failures don't block local cleanup.
            if let Err(e) = OrgSecretsService::delete(secret_id).await {
                warn!(%secret_id, "failed to delete airhouse password secret: {e}");
            }
        }

        AirhouseUsers::delete_by_id(local.id).exec(&self.db).await?;
        info!(username = %local.username, "deprovisioned airhouse user");
        Ok(())
    }

    async fn persist_success(
        &self,
        workspace_id: Uuid,
        org_id: Uuid,
        oxy_user_id: Uuid,
        tenant: &airhouse_tenants::Model,
        remote: &UserRecord,
        role: AirhouseUserRole,
        password: &str,
    ) -> Result<Uuid, UserProvisionerError> {
        let secret_name = secret_name_for(workspace_id, oxy_user_id);
        let secret_id = OrgSecretsService::upsert(org_id, &secret_name, password)
            .await
            .map_err(|e| UserProvisionerError::Secret(e.to_string()))?;

        let id = Uuid::new_v4();
        airhouse_users::ActiveModel {
            id: ActiveValue::Set(id),
            tenant_row_id: ActiveValue::Set(tenant.id),
            workspace_id: ActiveValue::Set(workspace_id),
            oxy_user_id: ActiveValue::Set(oxy_user_id),
            username: ActiveValue::Set(remote.username.clone()),
            role: ActiveValue::Set(role),
            password_secret_id: ActiveValue::Set(Some(secret_id)),
            password_revealed_at: ActiveValue::Set(None),
            status: ActiveValue::Set(AirhouseUserStatus::Active),
            created_at: ActiveValue::Set(Utc::now().fixed_offset()),
        }
        .insert(&self.db)
        .await?;
        Ok(id)
    }

    async fn insert_failed_row(
        &self,
        workspace_id: Uuid,
        oxy_user_id: Uuid,
        tenant: &airhouse_tenants::Model,
        username: &str,
        role: AirhouseUserRole,
    ) -> Result<(), sea_orm::DbErr> {
        airhouse_users::ActiveModel {
            id: ActiveValue::Set(Uuid::new_v4()),
            tenant_row_id: ActiveValue::Set(tenant.id),
            workspace_id: ActiveValue::Set(workspace_id),
            oxy_user_id: ActiveValue::Set(oxy_user_id),
            username: ActiveValue::Set(username.to_string()),
            role: ActiveValue::Set(role),
            password_secret_id: ActiveValue::Set(None),
            password_revealed_at: ActiveValue::Set(None),
            status: ActiveValue::Set(AirhouseUserStatus::Failed),
            created_at: ActiveValue::Set(Utc::now().fixed_offset()),
        }
        .insert(&self.db)
        .await
        .map(|_| ())
    }

    /// Resolve the org_id to use for `OrgSecretsService`.
    ///
    /// The local nil workspace is not a DB row; treat it as belonging to the
    /// local nil org. Real workspaces carry `org_id` from their DB row.
    async fn resolve_org_id(&self, workspace_id: Uuid) -> Result<Uuid, UserProvisionerError> {
        if workspace_id.is_nil() {
            return Ok(Uuid::nil());
        }
        let ws = Workspaces::find_by_id(workspace_id)
            .one(&self.db)
            .await?
            .ok_or(UserProvisionerError::WorkspaceNotFound(workspace_id))?;
        Ok(ws.org_id.unwrap_or(Uuid::nil()))
    }
}

async fn load_tenant_id(
    db: &DatabaseConnection,
    tenant_row_id: Uuid,
) -> Result<String, UserProvisionerError> {
    let tenant = AirhouseTenants::find_by_id(tenant_row_id)
        .one(db)
        .await?
        .ok_or_else(|| UserProvisionerError::TenantNotFound(Uuid::nil()))?;
    Ok(tenant.airhouse_tenant_id)
}

/// Secret name scoped to both workspace and user to avoid collisions when the
/// same Oxy user accesses Airhouse from multiple workspaces in the same org.
pub fn secret_name_for(workspace_id: Uuid, oxy_user_id: Uuid) -> String {
    format!("airhouse_user_password:{workspace_id}:{oxy_user_id}")
}

fn map_role(role: &OrgRole) -> AirhouseUserRole {
    match role {
        OrgRole::Owner => AirhouseUserRole::Admin,
        OrgRole::Admin => AirhouseUserRole::Writer,
        OrgRole::Member => AirhouseUserRole::Reader,
    }
}

fn airhouse_role_to_client(role: &AirhouseUserRole) -> UserRole {
    match role {
        AirhouseUserRole::Reader => UserRole::Reader,
        AirhouseUserRole::Writer => UserRole::Writer,
        AirhouseUserRole::Admin => UserRole::Admin,
    }
}

/// Derive a tenant-unique Airhouse username from an oxy user's email.
/// Format: `<sanitized-localpart>-<6-char-uuid>` with overall length ≤63.
fn derive_username(email: &str, oxy_user_id: Uuid) -> String {
    let localpart = email.split('@').next().unwrap_or(email);
    let mut sanitized = String::with_capacity(localpart.len());
    let mut prev_underscore = false;
    for ch in localpart.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() || ch == '-' {
            sanitized.push(ch);
            prev_underscore = false;
        } else if !prev_underscore && !sanitized.is_empty() {
            sanitized.push('_');
            prev_underscore = true;
        }
    }
    while sanitized.ends_with('_') || sanitized.ends_with('-') {
        sanitized.pop();
    }
    if sanitized.is_empty() {
        sanitized.push_str("user");
    }
    let suffix = &oxy_user_id.simple().to_string()[..6];
    let budget = MAX_USERNAME_LEN - 1 - suffix.len();
    if sanitized.len() > budget {
        sanitized.truncate(budget);
        while sanitized.ends_with('_') || sanitized.ends_with('-') {
            sanitized.pop();
        }
    }
    format!("{sanitized}-{suffix}")
}

fn generate_password() -> String {
    let bytes: [u8; PASSWORD_BYTES] = rand::random();
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_uses_email_localpart() {
        let u = derive_username("alice@example.com", Uuid::nil());
        assert!(u.starts_with("alice-"), "got {u}");
        assert_eq!(u.len(), "alice-".len() + 6);
    }

    #[test]
    fn username_collapses_unsafe_chars() {
        let u = derive_username("Tay+filter@example.com", Uuid::nil());
        assert!(u.starts_with("tay_filter-"), "got {u}");
    }

    #[test]
    fn username_truncates_long_localparts() {
        let email = format!("{}@example.com", "a".repeat(200));
        let u = derive_username(&email, Uuid::nil());
        assert!(u.len() <= MAX_USERNAME_LEN);
    }

    #[test]
    fn username_falls_back_when_localpart_strips_to_empty() {
        let u = derive_username("@example.com", Uuid::nil());
        assert!(u.starts_with("user-"));
    }

    #[test]
    fn username_unique_across_users_with_same_localpart() {
        let a = derive_username("alice@a.com", Uuid::new_v4());
        let b = derive_username("alice@b.com", Uuid::new_v4());
        assert_ne!(a, b);
    }

    #[test]
    fn role_map() {
        assert_eq!(map_role(&OrgRole::Owner), AirhouseUserRole::Admin);
        assert_eq!(map_role(&OrgRole::Admin), AirhouseUserRole::Writer);
        assert_eq!(map_role(&OrgRole::Member), AirhouseUserRole::Reader);
    }

    #[test]
    fn password_is_url_safe_and_long_enough() {
        let p = generate_password();
        // 32 raw bytes → 43 chars in url-safe base64 no-pad.
        assert_eq!(p.len(), 43);
        assert!(
            p.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
    }

    #[test]
    fn passwords_differ_per_call() {
        assert_ne!(generate_password(), generate_password());
    }

    #[test]
    fn secret_name_includes_workspace_and_user_id() {
        let ws = Uuid::new_v4();
        let uid = Uuid::new_v4();
        let name = secret_name_for(ws, uid);
        assert!(name.contains(&ws.to_string()));
        assert!(name.contains(&uid.to_string()));
    }
}
