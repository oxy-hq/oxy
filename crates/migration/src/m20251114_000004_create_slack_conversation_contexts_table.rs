use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(SlackConversationContexts::Table)
                    .if_not_exists()
                    .col(uuid(SlackConversationContexts::Id).primary_key())
                    .col(string(SlackConversationContexts::SlackTeamId))
                    .col(string(SlackConversationContexts::SlackChannelId))
                    .col(string(SlackConversationContexts::SlackThreadTs))
                    .col(uuid(SlackConversationContexts::OxySessionId))
                    .col(string_null(SlackConversationContexts::LastSlackMessageTs))
                    .col(
                        timestamp_with_time_zone(SlackConversationContexts::CreatedAt)
                            .extra("DEFAULT CURRENT_TIMESTAMP".to_string()),
                    )
                    .col(
                        timestamp_with_time_zone(SlackConversationContexts::UpdatedAt)
                            .extra("DEFAULT CURRENT_TIMESTAMP".to_string()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(
                                SlackConversationContexts::Table,
                                SlackConversationContexts::OxySessionId,
                            )
                            .to(Threads::Table, Threads::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create unique index on (slack_team_id, slack_channel_id, slack_thread_ts)
        manager
            .create_index(
                Index::create()
                    .table(SlackConversationContexts::Table)
                    .name("idx_slack_conversation_contexts_unique")
                    .col(SlackConversationContexts::SlackTeamId)
                    .col(SlackConversationContexts::SlackChannelId)
                    .col(SlackConversationContexts::SlackThreadTs)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Create index on oxy_session_id for reverse lookup
        manager
            .create_index(
                Index::create()
                    .table(SlackConversationContexts::Table)
                    .name("idx_slack_conversation_contexts_session")
                    .col(SlackConversationContexts::OxySessionId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(SlackConversationContexts::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum SlackConversationContexts {
    Table,
    Id,
    SlackTeamId,
    SlackChannelId,
    SlackThreadTs,
    OxySessionId,
    LastSlackMessageTs,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Threads {
    Table,
    Id,
}
