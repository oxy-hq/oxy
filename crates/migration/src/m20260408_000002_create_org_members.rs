use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum OrgMembers {
    Table,
    Id,
    OrgId,
    UserId,
    Role,
    CreatedAt,
    UpdatedAt,
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
                    .table(OrgMembers::Table)
                    .if_not_exists()
                    .col(uuid(OrgMembers::Id).primary_key())
                    .col(uuid(OrgMembers::OrgId).not_null())
                    .col(uuid(OrgMembers::UserId).not_null())
                    .col(string(OrgMembers::Role).not_null())
                    .col(
                        timestamp_with_time_zone(OrgMembers::CreatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        timestamp_with_time_zone(OrgMembers::UpdatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_org_members_org_id")
                            .from(OrgMembers::Table, OrgMembers::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_org_members_user_id")
                            .from(OrgMembers::Table, OrgMembers::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_org_members_org_user")
                    .table(OrgMembers::Table)
                    .col(OrgMembers::OrgId)
                    .col(OrgMembers::UserId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(Index::drop().name("idx_org_members_org_user").to_owned())
            .await?;

        manager
            .drop_table(
                Table::drop()
                    .table(OrgMembers::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
