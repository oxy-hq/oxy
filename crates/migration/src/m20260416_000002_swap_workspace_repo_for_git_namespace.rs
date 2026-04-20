use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Workspaces {
    Table,
    GitNamespaceId,
    GitRemoteUrl,
    ProjectRepoId,
    ActiveBranchId,
}

#[derive(DeriveIden)]
enum GitNamespaces {
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .add_column(uuid_null(Workspaces::GitNamespaceId).null())
                    .add_column(text_null(Workspaces::GitRemoteUrl).null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk_workspaces_git_namespace_id")
                    .from(Workspaces::Table, Workspaces::GitNamespaceId)
                    .to(GitNamespaces::Table, GitNamespaces::Id)
                    .on_delete(ForeignKeyAction::SetNull)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .drop_column(Workspaces::ProjectRepoId)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .drop_column(Workspaces::ActiveBranchId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .add_column(
                        ColumnDef::new(Workspaces::ActiveBranchId)
                            .uuid()
                            .not_null()
                            .default(Expr::cust("'00000000-0000-0000-0000-000000000000'::uuid")),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .add_column(uuid_null(Workspaces::ProjectRepoId).null())
                    .to_owned(),
            )
            .await?;

        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .table(Workspaces::Table)
                    .name("fk_workspaces_git_namespace_id")
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .drop_column(Workspaces::GitRemoteUrl)
                    .drop_column(Workspaces::GitNamespaceId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
