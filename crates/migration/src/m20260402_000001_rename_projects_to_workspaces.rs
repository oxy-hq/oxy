use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Rename project_repos → workspace_repos first (no FK from projects points here by name)
        manager
            .rename_table(
                Table::rename()
                    .table(Alias::new("project_repos"), Alias::new("workspace_repos"))
                    .to_owned(),
            )
            .await?;

        // Rename projects → workspaces
        manager
            .rename_table(
                Table::rename()
                    .table(Alias::new("projects"), Alias::new("workspaces"))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .rename_table(
                Table::rename()
                    .table(Alias::new("workspaces"), Alias::new("projects"))
                    .to_owned(),
            )
            .await?;

        manager
            .rename_table(
                Table::rename()
                    .table(Alias::new("workspace_repos"), Alias::new("project_repos"))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
