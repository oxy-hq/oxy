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
            CREATE TABLE slack_installations (
                id UUID PRIMARY KEY,
                org_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                slack_team_id VARCHAR(32) NOT NULL,
                slack_team_name VARCHAR(255) NOT NULL,
                slack_enterprise_id VARCHAR(32),
                bot_user_id VARCHAR(32) NOT NULL,
                bot_token_secret_id UUID NOT NULL REFERENCES org_secrets(id) ON DELETE RESTRICT,
                bot_scopes TEXT NOT NULL,
                installed_by_user_id UUID NOT NULL REFERENCES users(id),
                installed_by_slack_user_id VARCHAR(32) NOT NULL,
                installed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                revoked_at TIMESTAMPTZ
            );
            CREATE UNIQUE INDEX uniq_slack_installations_team_active
                ON slack_installations(slack_team_id) WHERE revoked_at IS NULL;
            CREATE UNIQUE INDEX uniq_slack_installations_org_active
                ON slack_installations(org_id) WHERE revoked_at IS NULL;
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS slack_installations CASCADE")
            .await?;
        Ok(())
    }
}
