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
            CREATE TABLE slack_user_links (
                id UUID PRIMARY KEY,
                installation_id UUID NOT NULL REFERENCES slack_installations(id) ON DELETE CASCADE,
                slack_user_id VARCHAR(32) NOT NULL,
                oxy_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                link_method VARCHAR(16) NOT NULL,
                linked_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                CONSTRAINT uniq_slack_user_per_install UNIQUE (installation_id, slack_user_id)
            );
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS slack_user_links CASCADE")
            .await?;
        Ok(())
    }
}
