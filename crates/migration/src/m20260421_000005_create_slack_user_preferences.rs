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
            CREATE TABLE slack_user_preferences (
                id UUID PRIMARY KEY,
                user_link_id UUID NOT NULL UNIQUE REFERENCES slack_user_links(id) ON DELETE CASCADE,
                default_workspace_id UUID REFERENCES workspaces(id) ON DELETE SET NULL,
                default_agent_path TEXT,
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
            );
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS slack_user_preferences CASCADE")
            .await?;
        Ok(())
    }
}
