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
                CREATE TABLE slack_seen_events (
                    event_id VARCHAR(64) PRIMARY KEY,
                    received_at TIMESTAMPTZ NOT NULL DEFAULT now()
                );
                CREATE INDEX idx_slack_seen_events_received_at
                    ON slack_seen_events(received_at);
                "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS slack_seen_events CASCADE")
            .await?;
        Ok(())
    }
}
