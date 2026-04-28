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
                CREATE TABLE slack_channel_defaults (
                    id UUID PRIMARY KEY,
                    installation_id UUID NOT NULL REFERENCES slack_installations(id) ON DELETE CASCADE,
                    slack_channel_id VARCHAR(32) NOT NULL,
                    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
                    set_by_user_link_id UUID REFERENCES slack_user_links(id) ON DELETE SET NULL,
                    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                    CONSTRAINT uniq_channel_default UNIQUE (installation_id, slack_channel_id)
                );
                "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS slack_channel_defaults CASCADE")
            .await?;
        Ok(())
    }
}
