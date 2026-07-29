use sea_orm_migration::prelude::*;

/// Track what the health sweep last *told Slack*, separately from what it last
/// *observed*. `status`/`reasons` are rewritten by every eval pass, so they can't
/// answer "have we paged about this yet, and how long ago?" — which is what turns
/// a stuck-unhealthy workspace into a repeating reminder instead of one alert
/// that scrolls away forever.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager
            .has_column("workspace_health_state", "last_alerted_at")
            .await?
        {
            manager
                .alter_table(
                    Table::alter()
                        .table(WorkspaceHealthState::Table)
                        .add_column(
                            ColumnDef::new(WorkspaceHealthState::LastAlertedAt)
                                .timestamp_with_time_zone(),
                        )
                        .to_owned(),
                )
                .await?;
        }
        // The failing dimensions the last alert covered, not its reason text —
        // reason strings carry live counts that drift every pass, so diffing them
        // would re-page continuously. `jsonb`, matching the rest of the table.
        if !manager
            .has_column("workspace_health_state", "alerted_failures")
            .await?
        {
            manager
                .alter_table(
                    Table::alter()
                        .table(WorkspaceHealthState::Table)
                        .add_column(
                            ColumnDef::new(WorkspaceHealthState::AlertedFailures).json_binary(),
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
                    .drop_column(WorkspaceHealthState::LastAlertedAt)
                    .drop_column(WorkspaceHealthState::AlertedFailures)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum WorkspaceHealthState {
    Table,
    LastAlertedAt,
    AlertedFailures,
}
