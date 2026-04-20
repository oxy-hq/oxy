use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Workspaces {
    Table,
    OrgId,
    #[sea_orm(iden = "workspace_id")]
    WorkspaceId,
}

#[derive(DeriveIden)]
enum GitNamespaces {
    Table,
    OrgId,
    #[sea_orm(iden = "user_id")]
    UserId,
    #[sea_orm(iden = "created_by")]
    CreatedBy,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    #[sea_orm(iden = "role")]
    Role,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 1. Add org_id to workspaces (nullable UUID)
        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .add_column(ColumnDef::new(Workspaces::OrgId).uuid().null().to_owned())
                    .to_owned(),
            )
            .await?;

        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk_workspaces_org_id")
                    .from(Workspaces::Table, Workspaces::OrgId)
                    .to(Organizations::Table, Organizations::Id)
                    .on_delete(ForeignKeyAction::Cascade)
                    .to_owned(),
            )
            .await?;

        // 2. Drop workspace_id from workspaces (legacy unused field, always Uuid::nil())
        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .drop_column(Workspaces::WorkspaceId)
                    .to_owned(),
            )
            .await?;

        // 3. Add org_id to git_namespaces (nullable UUID)
        manager
            .alter_table(
                Table::alter()
                    .table(GitNamespaces::Table)
                    .add_column(
                        ColumnDef::new(GitNamespaces::OrgId)
                            .uuid()
                            .null()
                            .to_owned(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk_git_namespaces_org_id")
                    .from(GitNamespaces::Table, GitNamespaces::OrgId)
                    .to(Organizations::Table, Organizations::Id)
                    .on_delete(ForeignKeyAction::Cascade)
                    .to_owned(),
            )
            .await?;

        // 4. Rename user_id → created_by in git_namespaces
        manager
            .alter_table(
                Table::alter()
                    .table(GitNamespaces::Table)
                    .rename_column(GitNamespaces::UserId, GitNamespaces::CreatedBy)
                    .to_owned(),
            )
            .await?;

        // 5. Drop role from users (roles moved to org_members)
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .drop_column(Users::Role)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 5. Restore role column on users
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column(ColumnDef::new(Users::Role).string().null().to_owned())
                    .to_owned(),
            )
            .await?;

        // 4. Rename created_by → user_id in git_namespaces
        manager
            .alter_table(
                Table::alter()
                    .table(GitNamespaces::Table)
                    .rename_column(GitNamespaces::CreatedBy, GitNamespaces::UserId)
                    .to_owned(),
            )
            .await?;

        // 3. Drop org_id FK and column from git_namespaces
        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .name("fk_git_namespaces_org_id")
                    .table(GitNamespaces::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(GitNamespaces::Table)
                    .drop_column(GitNamespaces::OrgId)
                    .to_owned(),
            )
            .await?;

        // 2. Restore workspace_id on workspaces
        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .add_column(
                        ColumnDef::new(Workspaces::WorkspaceId)
                            .uuid()
                            .null()
                            .to_owned(),
                    )
                    .to_owned(),
            )
            .await?;

        // 1. Drop org_id FK and column from workspaces
        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .name("fk_workspaces_org_id")
                    .table(Workspaces::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .drop_column(Workspaces::OrgId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
