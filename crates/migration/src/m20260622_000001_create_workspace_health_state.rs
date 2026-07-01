use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum WorkspaceHealthState {
    Table,
    WorkspaceId,
    Status,
    Reasons,
    ChangedAt,
    UpdatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(WorkspaceHealthState::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(WorkspaceHealthState::WorkspaceId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(WorkspaceHealthState::Status)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorkspaceHealthState::Reasons)
                            .json_binary()
                            .not_null()
                            .default(Expr::cust("'[]'::jsonb")),
                    )
                    .col(
                        ColumnDef::new(WorkspaceHealthState::ChangedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(WorkspaceHealthState::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(WorkspaceHealthState::Table).to_owned())
            .await
    }
}
