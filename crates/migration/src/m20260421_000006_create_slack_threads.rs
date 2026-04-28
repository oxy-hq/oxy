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
            CREATE TABLE slack_threads (
                id UUID PRIMARY KEY,
                installation_id UUID NOT NULL REFERENCES slack_installations(id) ON DELETE CASCADE,
                slack_channel_id VARCHAR(32) NOT NULL,
                slack_thread_ts VARCHAR(64) NOT NULL,
                workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
                agent_path TEXT NOT NULL,
                oxy_thread_id UUID NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
                initiated_by_user_link_id UUID REFERENCES slack_user_links(id) ON DELETE SET NULL,
                last_slack_message_ts VARCHAR(64),
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                CONSTRAINT uniq_thread UNIQUE (installation_id, slack_channel_id, slack_thread_ts)
            );
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS slack_threads CASCADE")
            .await?;
        Ok(())
    }
}
