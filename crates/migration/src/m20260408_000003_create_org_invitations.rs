use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum OrgInvitations {
    Table,
    Id,
    OrgId,
    Email,
    Role,
    InvitedBy,
    Token,
    Status,
    ExpiresAt,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(OrgInvitations::Table)
                    .if_not_exists()
                    .col(uuid(OrgInvitations::Id).primary_key())
                    .col(uuid(OrgInvitations::OrgId).not_null())
                    .col(string(OrgInvitations::Email).not_null())
                    .col(string(OrgInvitations::Role).not_null())
                    .col(uuid(OrgInvitations::InvitedBy).not_null())
                    .col(string(OrgInvitations::Token).not_null().unique_key())
                    .col(string(OrgInvitations::Status).not_null().default("pending"))
                    .col(timestamp_with_time_zone(OrgInvitations::ExpiresAt).not_null())
                    .col(
                        timestamp_with_time_zone(OrgInvitations::CreatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_org_invitations_org_id")
                            .from(OrgInvitations::Table, OrgInvitations::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_org_invitations_invited_by")
                            .from(OrgInvitations::Table, OrgInvitations::InvitedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(OrgInvitations::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
