use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager
            .has_column("workspace_health_state", "last_smoke_at")
            .await?
        {
            manager
                .alter_table(
                    Table::alter()
                        .table(WorkspaceHealthState::Table)
                        .add_column(
                            ColumnDef::new(WorkspaceHealthState::LastSmokeAt)
                                .timestamp_with_time_zone(),
                        )
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(WorkspaceHealthState::Table)
                    .drop_column(WorkspaceHealthState::LastSmokeAt)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum WorkspaceHealthState {
    Table,
    LastSmokeAt,
}
