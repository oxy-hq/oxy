use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ReconcileConfigs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ReconcileConfigs::RevisionId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(ReconcileConfigs::Definition)
                            .json_binary()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_reconcile_configs_revision")
                            .from(ReconcileConfigs::Table, ReconcileConfigs::RevisionId)
                            .to(Revisions::Table, Revisions::RevisionId)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ReconcileConfigs::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum ReconcileConfigs {
    Table,
    RevisionId,
    Definition,
}

#[derive(DeriveIden)]
enum Revisions {
    Table,
    RevisionId,
}
