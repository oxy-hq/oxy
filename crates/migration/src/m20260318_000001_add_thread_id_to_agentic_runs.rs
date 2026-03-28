use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("agentic_runs"))
                    .add_column(ColumnDef::new(Alias::new("thread_id")).uuid().null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk_agentic_runs_thread_id")
                    .from(Alias::new("agentic_runs"), Alias::new("thread_id"))
                    .to(Alias::new("threads"), Alias::new("id"))
                    .on_delete(ForeignKeyAction::Cascade)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .name("fk_agentic_runs_thread_id")
                    .table(Alias::new("agentic_runs"))
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("agentic_runs"))
                    .drop_column(Alias::new("thread_id"))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
