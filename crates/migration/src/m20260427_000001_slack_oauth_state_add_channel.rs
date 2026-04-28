use sea_orm_migration::prelude::*;

/// Add `slack_channel_id` and `slack_thread_ts` to `slack_oauth_states`.
///
/// These nullable columns let the confirm-link handler (`POST /api/slack/link/confirm`)
/// dispatch a "✅ You're connected!" ephemeral back to the channel where
/// the user originally asked, closing the auth loop visibly.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                ALTER TABLE slack_oauth_states
                    ADD COLUMN IF NOT EXISTS slack_channel_id VARCHAR(32),
                    ADD COLUMN IF NOT EXISTS slack_thread_ts  VARCHAR(64);
                "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                ALTER TABLE slack_oauth_states
                    DROP COLUMN IF EXISTS slack_channel_id,
                    DROP COLUMN IF EXISTS slack_thread_ts;
                "#,
            )
            .await?;
        Ok(())
    }
}
