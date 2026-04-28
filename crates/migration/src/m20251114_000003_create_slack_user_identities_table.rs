use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(SlackUserIdentities::Table)
                    .if_not_exists()
                    .col(uuid(SlackUserIdentities::Id).primary_key())
                    .col(string(SlackUserIdentities::SlackTeamId))
                    .col(string(SlackUserIdentities::SlackUserId))
                    .col(uuid(SlackUserIdentities::OxyUserId))
                    .col(
                        timestamp_with_time_zone(SlackUserIdentities::LinkedAt)
                            .extra("DEFAULT CURRENT_TIMESTAMP".to_string()),
                    )
                    .col(
                        timestamp_with_time_zone(SlackUserIdentities::LastSeenAt)
                            .extra("DEFAULT CURRENT_TIMESTAMP".to_string()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(SlackUserIdentities::Table, SlackUserIdentities::OxyUserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create unique index on (slack_team_id, slack_user_id)
        manager
            .create_index(
                Index::create()
                    .table(SlackUserIdentities::Table)
                    .name("idx_slack_user_identities_unique")
                    .col(SlackUserIdentities::SlackTeamId)
                    .col(SlackUserIdentities::SlackUserId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SlackUserIdentities::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum SlackUserIdentities {
    Table,
    Id,
    SlackTeamId,
    SlackUserId,
    OxyUserId,
    LinkedAt,
    LastSeenAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
