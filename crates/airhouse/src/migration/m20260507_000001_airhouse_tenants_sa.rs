use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Add Airhouse service-account columns to `airhouse_tenants`.
///
/// Each tenant gets exactly one SA (1:1 cardinality), used by the
/// `AirhouseTokenBroker` to mint short-lived ephemeral credentials per oxy
/// user. All columns are nullable so existing rows survive without an SA
/// until the provisioner backfills them on the next provision call.
///
/// `bearer_ciphertext` is AES-GCM-sealed with `OXY_ENCRYPTION_KEY` (the
/// same envelope key used by `org_secrets`). The bearer is shown by the
/// Airhouse Admin API exactly once at create time; if our local copy is
/// lost the only remedy is to revoke the SA and mint a new one.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            ALTER TABLE airhouse_tenants
                ADD COLUMN service_account_id   TEXT,
                ADD COLUMN bearer_ciphertext    BYTEA,
                ADD COLUMN bearer_max_role      VARCHAR(16),
                ADD COLUMN bearer_max_ttl_secs  INTEGER,
                ADD COLUMN sa_created_at        TIMESTAMPTZ,
                ADD COLUMN sa_rotated_at        TIMESTAMPTZ;
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            ALTER TABLE airhouse_tenants
                DROP COLUMN IF EXISTS service_account_id,
                DROP COLUMN IF EXISTS bearer_ciphertext,
                DROP COLUMN IF EXISTS bearer_max_role,
                DROP COLUMN IF EXISTS bearer_max_ttl_secs,
                DROP COLUMN IF EXISTS sa_created_at,
                DROP COLUMN IF EXISTS sa_rotated_at;
        "#,
            )
            .await?;
        Ok(())
    }
}
