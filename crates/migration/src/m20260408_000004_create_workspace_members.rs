use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum WorkspaceMembers {
    Table,
    Id,
    WorkspaceId,
    UserId,
    Role,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Workspaces {
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
                    .table(WorkspaceMembers::Table)
                    .if_not_exists()
                    .col(uuid(WorkspaceMembers::Id).primary_key())
                    .col(uuid(WorkspaceMembers::WorkspaceId).not_null())
                    .col(uuid(WorkspaceMembers::UserId).not_null())
                    .col(string(WorkspaceMembers::Role).not_null())
                    .col(
                        timestamp_with_time_zone(WorkspaceMembers::CreatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        timestamp_with_time_zone(WorkspaceMembers::UpdatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_workspace_members_workspace_id")
                            .from(WorkspaceMembers::Table, WorkspaceMembers::WorkspaceId)
                            .to(Workspaces::Table, Workspaces::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_workspace_members_user_id")
                            .from(WorkspaceMembers::Table, WorkspaceMembers::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_workspace_members_ws_user")
                    .table(WorkspaceMembers::Table)
                    .col(WorkspaceMembers::WorkspaceId)
                    .col(WorkspaceMembers::UserId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_workspace_members_ws_user")
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(
                Table::drop()
                    .table(WorkspaceMembers::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
