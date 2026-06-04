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
            CREATE TABLE quickbooks_oauth_states (
                id UUID PRIMARY KEY,
                nonce VARCHAR(64) NOT NULL UNIQUE,
                project_id UUID NOT NULL,
                client_id TEXT NOT NULL,
                client_secret_var TEXT NOT NULL,
                refresh_token_var TEXT NOT NULL,
                redirect_uri TEXT NOT NULL,
                mode VARCHAR(16) NOT NULL,
                return_path TEXT,
                created_by UUID,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                expires_at TIMESTAMPTZ NOT NULL,
                consumed_at TIMESTAMPTZ
            );
            CREATE INDEX idx_quickbooks_oauth_states_active
                ON quickbooks_oauth_states(expires_at) WHERE consumed_at IS NULL;
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS quickbooks_oauth_states CASCADE")
            .await?;
        Ok(())
    }
}
