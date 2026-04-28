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
            CREATE TABLE slack_oauth_states (
                id UUID PRIMARY KEY,
                kind VARCHAR(16) NOT NULL,
                nonce VARCHAR(64) NOT NULL UNIQUE,
                org_id UUID REFERENCES organizations(id) ON DELETE CASCADE,
                slack_team_id VARCHAR(32),
                slack_user_id VARCHAR(32),
                oxy_user_id UUID REFERENCES users(id) ON DELETE CASCADE,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                expires_at TIMESTAMPTZ NOT NULL,
                consumed_at TIMESTAMPTZ
            );
            CREATE INDEX idx_slack_oauth_states_active
                ON slack_oauth_states(expires_at) WHERE consumed_at IS NULL;
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS slack_oauth_states CASCADE")
            .await?;
        Ok(())
    }
}
