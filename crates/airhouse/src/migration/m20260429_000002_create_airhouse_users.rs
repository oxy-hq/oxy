use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE airhouse_users (
                id UUID PRIMARY KEY,
                tenant_row_id UUID NOT NULL REFERENCES airhouse_tenants(id) ON DELETE CASCADE,
                org_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                oxy_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                username VARCHAR(63) NOT NULL,
                role VARCHAR(16) NOT NULL,
                password_secret_id UUID,
                password_revealed_at TIMESTAMPTZ,
                status VARCHAR(32) NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                CONSTRAINT uniq_airhouse_users_org_user UNIQUE (org_id, oxy_user_id),
                CONSTRAINT uniq_airhouse_users_tenant_username UNIQUE (tenant_row_id, username)
            );
            CREATE INDEX idx_airhouse_users_oxy_user ON airhouse_users(oxy_user_id);
            CREATE INDEX idx_airhouse_users_org ON airhouse_users(org_id);
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS airhouse_users CASCADE")
            .await?;
        Ok(())
    }
}
