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
            CREATE TABLE airhouse_tenants (
                id UUID PRIMARY KEY,
                org_id UUID NOT NULL UNIQUE REFERENCES organizations(id) ON DELETE CASCADE,
                airhouse_tenant_id VARCHAR(63) NOT NULL UNIQUE,
                bucket TEXT NOT NULL,
                prefix TEXT,
                status VARCHAR(32) NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            );
            CREATE INDEX idx_airhouse_tenants_org ON airhouse_tenants(org_id);
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS airhouse_tenants CASCADE")
            .await?;
        Ok(())
    }
}
