use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum OrgBilling {
    Table,
    Id,
    OrgId,
    StripeCustomerId,
    StripeSubscriptionId,
    StripeSubscriptionSeatItemId,
    Status,
    CurrentPeriodStart,
    CurrentPeriodEnd,
    GracePeriodEndsAt,
    SeatsPaid,
    PaymentActionUrl,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(OrgBilling::Table)
                    .if_not_exists()
                    .col(uuid(OrgBilling::Id).primary_key())
                    .col(uuid(OrgBilling::OrgId).not_null().unique_key())
                    .col(string_null(OrgBilling::StripeCustomerId))
                    .col(string_null(OrgBilling::StripeSubscriptionId))
                    .col(string_null(OrgBilling::StripeSubscriptionSeatItemId))
                    .col(
                        string_len(OrgBilling::Status, 20)
                            .not_null()
                            .default("incomplete"),
                    )
                    .col(timestamp_with_time_zone_null(
                        OrgBilling::CurrentPeriodStart,
                    ))
                    .col(timestamp_with_time_zone_null(OrgBilling::CurrentPeriodEnd))
                    .col(timestamp_with_time_zone_null(OrgBilling::GracePeriodEndsAt))
                    .col(integer(OrgBilling::SeatsPaid).not_null().default(0))
                    .col(string_null(OrgBilling::PaymentActionUrl))
                    .col(
                        timestamp_with_time_zone(OrgBilling::CreatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        timestamp_with_time_zone(OrgBilling::UpdatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_org_billing_org_id")
                            .from(OrgBilling::Table, OrgBilling::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_org_billing_status")
                    .table(OrgBilling::Table)
                    .col(OrgBilling::Status)
                    .to_owned(),
            )
            .await?;

        manager
            .get_connection()
            .execute_unprepared(
                r#"
                INSERT INTO org_billing (id, org_id, status, seats_paid, created_at, updated_at)
                SELECT gen_random_uuid(), o.id, 'incomplete', 0, now(), now()
                FROM organizations o
                LEFT JOIN org_billing ob ON ob.org_id = o.id
                WHERE ob.org_id IS NULL;
                "#,
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(OrgBilling::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
