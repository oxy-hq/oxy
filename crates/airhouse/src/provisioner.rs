use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use thiserror::Error;
use tracing::{info, instrument, warn};
use uuid::Uuid;

use crate::admin::{AirhouseAdminClient, AirhouseError, TenantRecord};
use crate::entity::Tenants as AirhouseTenants;
use crate::entity::tenants::{self as airhouse_tenants, TenantStatus};

#[derive(Debug, Error)]
pub enum ProvisionerError {
    #[error("workspace {0} not found")]
    WorkspaceNotFound(Uuid),
    #[error(
        "invalid tenant name {0:?}: must be 1-63 lowercase alphanumeric/hyphen/underscore chars, starting with a letter"
    )]
    InvalidTenantName(String),
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
    #[error("airhouse error: {0}")]
    Airhouse(#[from] AirhouseError),
    #[error("airhouse tenant {0} disappeared after create")]
    MissingAfterAdopt(String),
}

pub struct TenantProvisioner {
    db: DatabaseConnection,
    client: AirhouseAdminClient,
}

impl TenantProvisioner {
    pub fn new(db: DatabaseConnection, client: AirhouseAdminClient) -> Self {
        Self { db, client }
    }

    /// Idempotent: re-running with the same `workspace_id` returns the existing
    /// tenant without contacting Airhouse again.
    #[instrument(skip(self), fields(workspace_id = %workspace_id, tenant_name = %tenant_name))]
    pub async fn provision(
        &self,
        workspace_id: Uuid,
        tenant_name: String,
    ) -> Result<TenantRecord, ProvisionerError> {
        validate_tenant_name(&tenant_name)?;
        info!(tenant_name = %tenant_name, "provisioning airhouse tenant for workspace");

        let existing_local = AirhouseTenants::find()
            .filter(airhouse_tenants::Column::WorkspaceId.eq(workspace_id))
            .one(&self.db)
            .await?;

        match existing_local {
            Some(local) => self.reconcile_existing(&local).await,
            None => self.create_or_adopt(workspace_id, &tenant_name).await,
        }
    }

    /// Idempotent: succeeds whether the local row or the remote tenant exist.
    #[instrument(skip(self), fields(workspace_id = %workspace_id))]
    pub async fn deprovision(&self, workspace_id: Uuid) -> Result<(), ProvisionerError> {
        let local = AirhouseTenants::find()
            .filter(airhouse_tenants::Column::WorkspaceId.eq(workspace_id))
            .one(&self.db)
            .await?;
        let Some(local) = local else {
            info!("no local tenant row; deprovision is a no-op");
            return Ok(());
        };

        // Airhouse delete_tenant is itself idempotent (returns 204 in both cases).
        self.client.delete_tenant(&local.airhouse_tenant_id).await?;
        AirhouseTenants::delete_by_id(local.id)
            .exec(&self.db)
            .await?;
        info!(tenant_id = %local.airhouse_tenant_id, "deprovisioned airhouse tenant");
        Ok(())
    }

    async fn reconcile_existing(
        &self,
        local: &airhouse_tenants::Model,
    ) -> Result<TenantRecord, ProvisionerError> {
        match self.client.get_tenant(&local.airhouse_tenant_id).await? {
            Some(remote) => {
                if local.status != TenantStatus::Active {
                    self.set_status(local.id, TenantStatus::Active).await?;
                }
                Ok(remote)
            }
            None => {
                warn!(
                    tenant_id = %local.airhouse_tenant_id,
                    "remote tenant missing; recreating to match local row"
                );
                let remote = self.client.create_tenant(&local.airhouse_tenant_id).await?;
                self.set_status(local.id, TenantStatus::Active).await?;
                Ok(remote)
            }
        }
    }

    async fn create_or_adopt(
        &self,
        workspace_id: Uuid,
        tenant_name: &str,
    ) -> Result<TenantRecord, ProvisionerError> {
        let create_result = self.client.create_tenant(tenant_name).await;

        match create_result {
            Ok(remote) => {
                self.insert_local_row(workspace_id, &remote, TenantStatus::Active)
                    .await?;
                Ok(remote)
            }
            Err(AirhouseError::AlreadyExists(msg)) => {
                warn!(tenant_name, msg, "airhouse 409 — adopting existing tenant");
                let remote =
                    self.client.get_tenant(tenant_name).await?.ok_or_else(|| {
                        ProvisionerError::MissingAfterAdopt(tenant_name.to_string())
                    })?;
                self.insert_local_row(workspace_id, &remote, TenantStatus::Active)
                    .await?;
                Ok(remote)
            }
            Err(e) => {
                let _ = self
                    .insert_failed_local_row(workspace_id, tenant_name)
                    .await;
                Err(e.into())
            }
        }
    }

    async fn insert_local_row(
        &self,
        workspace_id: Uuid,
        remote: &TenantRecord,
        status: TenantStatus,
    ) -> Result<(), ProvisionerError> {
        let row = airhouse_tenants::ActiveModel {
            id: ActiveValue::Set(Uuid::new_v4()),
            workspace_id: ActiveValue::Set(workspace_id),
            airhouse_tenant_id: ActiveValue::Set(remote.id.clone()),
            bucket: ActiveValue::Set(remote.bucket.clone()),
            prefix: ActiveValue::Set(remote.prefix.clone()),
            status: ActiveValue::Set(status),
            created_at: ActiveValue::Set(Utc::now().fixed_offset()),
        };
        row.insert(&self.db).await?;
        Ok(())
    }

    /// Write a placeholder row when remote provisioning fails before we ever
    /// got a `TenantRecord` back — bucket/prefix are unknown until the server
    /// resolves them, so we leave them empty/null until reconciliation runs.
    async fn insert_failed_local_row(
        &self,
        workspace_id: Uuid,
        tenant_name: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let row = airhouse_tenants::ActiveModel {
            id: ActiveValue::Set(Uuid::new_v4()),
            workspace_id: ActiveValue::Set(workspace_id),
            airhouse_tenant_id: ActiveValue::Set(tenant_name.to_string()),
            bucket: ActiveValue::Set(String::new()),
            prefix: ActiveValue::Set(None),
            status: ActiveValue::Set(TenantStatus::Failed),
            created_at: ActiveValue::Set(Utc::now().fixed_offset()),
        };
        row.insert(&self.db).await.map(|_| ())
    }

    async fn set_status(&self, id: Uuid, status: TenantStatus) -> Result<(), sea_orm::DbErr> {
        let row = airhouse_tenants::ActiveModel {
            id: ActiveValue::Set(id),
            status: ActiveValue::Set(status),
            ..Default::default()
        };
        row.update(&self.db).await.map(|_| ())
    }
}

/// Tenant names must be valid PostgreSQL role identifiers:
/// - 1–63 chars
/// - first char is a lowercase ASCII letter
/// - remaining chars are lowercase ASCII alnum, `-`, or `_`
///
/// Equivalent regex (kept here for reference only; not evaluated):
/// `^[a-z][a-z0-9_-]{0,62}$`
fn validate_tenant_name(name: &str) -> Result<(), ProvisionerError> {
    if name.is_empty() || name.len() > 63 {
        return Err(ProvisionerError::InvalidTenantName(name.to_string()));
    }
    if !name.starts_with(|c: char| c.is_ascii_lowercase()) {
        return Err(ProvisionerError::InvalidTenantName(name.to_string()));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(ProvisionerError::InvalidTenantName(name.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_tenant_names() {
        for name in ["acme", "acme-corp", "my_tenant", "a123", "a1b2c3"] {
            assert!(validate_tenant_name(name).is_ok(), "expected ok for {name}");
        }
    }

    #[test]
    fn invalid_tenant_names() {
        for name in [
            "",
            "1starts-digit",
            "-starts-hyphen",
            "_starts_underscore",
            &"a".repeat(64),
        ] {
            assert!(
                validate_tenant_name(name).is_err(),
                "expected err for {name:?}"
            );
        }
    }

    #[test]
    fn single_char_tenant_name() {
        assert!(validate_tenant_name("a").is_ok());
    }

    #[test]
    fn max_length_tenant_name() {
        let name = format!("a{}", "x".repeat(62));
        assert_eq!(name.len(), 63);
        assert!(validate_tenant_name(&name).is_ok());
    }
}
