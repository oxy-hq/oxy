use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(SlackChannelBindings::Table)
                    .if_not_exists()
                    .col(uuid(SlackChannelBindings::Id).primary_key())
                    .col(string(SlackChannelBindings::SlackTeamId))
                    .col(string(SlackChannelBindings::SlackChannelId))
                    .col(uuid(SlackChannelBindings::OxyProjectId))
                    .col(string(SlackChannelBindings::DefaultAgentId))
                    .col(string(SlackChannelBindings::CreatedBySlackUserId))
                    .col(
                        timestamp_with_time_zone(SlackChannelBindings::CreatedAt)
                            .extra("DEFAULT CURRENT_TIMESTAMP".to_string()),
                    )
                    .to_owned(),
            )
            .await?;

        // Create unique index on (slack_team_id, slack_channel_id)
        manager
            .create_index(
                Index::create()
                    .table(SlackChannelBindings::Table)
                    .name("idx_slack_channel_bindings_unique")
                    .col(SlackChannelBindings::SlackTeamId)
                    .col(SlackChannelBindings::SlackChannelId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SlackChannelBindings::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum SlackChannelBindings {
    Table,
    Id,
    SlackTeamId,
    SlackChannelId,
    OxyProjectId,
    DefaultAgentId,
    CreatedBySlackUserId,
    CreatedAt,
}
