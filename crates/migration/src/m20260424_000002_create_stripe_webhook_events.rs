use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum StripeWebhookEvents {
    Table,
    StripeEventId,
    EventType,
    Payload,
    ProcessedAt,
    Status,
    Attempts,
    LastError,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(StripeWebhookEvents::Table)
                    .if_not_exists()
                    .col(
                        string(StripeWebhookEvents::StripeEventId)
                            .not_null()
                            .primary_key(),
                    )
                    .col(string(StripeWebhookEvents::EventType).not_null())
                    .col(json_binary(StripeWebhookEvents::Payload).not_null())
                    .col(
                        timestamp_with_time_zone(StripeWebhookEvents::ProcessedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(string(StripeWebhookEvents::Status).not_null())
                    .col(integer(StripeWebhookEvents::Attempts).not_null().default(0))
                    .col(text_null(StripeWebhookEvents::LastError))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_stripe_webhook_events_type")
                    .table(StripeWebhookEvents::Table)
                    .col(StripeWebhookEvents::EventType)
                    .col(StripeWebhookEvents::ProcessedAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(StripeWebhookEvents::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
