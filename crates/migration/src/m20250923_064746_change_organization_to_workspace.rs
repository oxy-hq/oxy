use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

// --- Define identifiers ---
#[derive(Iden)]
enum Organizations {
    Table,
    #[allow(dead_code)]
    OrganizationId,
}

#[derive(Iden)]
enum Workspaces {
    Table,
    #[allow(dead_code)]
    WorkspaceId,
}

#[derive(Iden)]
enum OrganizationUsers {
    Table,
    OrganizationId,
}

#[derive(Iden)]
enum WorkspaceUsers {
    Table,
    WorkspaceId,
}

#[derive(Iden)]
enum Projects {
    Table,
    OrganizationId,
    WorkspaceId,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .rename_table(
                Table::rename()
                    .table(Organizations::Table, Workspaces::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .rename_table(
                Table::rename()
                    .table(OrganizationUsers::Table, WorkspaceUsers::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Projects::Table)
                    .rename_column(Projects::OrganizationId, Projects::WorkspaceId)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(WorkspaceUsers::Table)
                    .rename_column(
                        OrganizationUsers::OrganizationId,
                        WorkspaceUsers::WorkspaceId,
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .rename_table(
                Table::rename()
                    .table(Workspaces::Table, Organizations::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .rename_table(
                Table::rename()
                    .table(WorkspaceUsers::Table, OrganizationUsers::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Projects::Table)
                    .rename_column(Projects::WorkspaceId, Projects::OrganizationId)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(OrganizationUsers::Table)
                    .rename_column(
                        WorkspaceUsers::WorkspaceId,
                        OrganizationUsers::OrganizationId,
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
