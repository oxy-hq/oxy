use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Rename agent to source
        manager
            .alter_table(
                Table::alter()
                    .table(Threads::Table)
                    .rename_column(Threads::Agent, Threads::Source)
                    .to_owned(),
            )
            .await?;

        // Rename answer to output
        manager
            .alter_table(
                Table::alter()
                    .table(Threads::Table)
                    .rename_column(Threads::Answer, Threads::Output)
                    .to_owned(),
            )
            .await?;

        // Rename question to input
        manager
            .alter_table(
                Table::alter()
                    .table(Threads::Table)
                    .rename_column(Threads::Question, Threads::Input)
                    .to_owned(),
            )
            .await?;

        // Add type column
        manager
            .alter_table(
                Table::alter()
                    .table(Threads::Table)
                    .add_column(string(Threads::SourceType).default(""))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop type column
        manager
            .alter_table(
                Table::alter()
                    .table(Threads::Table)
                    .drop_column(Threads::SourceType)
                    .to_owned(),
            )
            .await?;

        // Rename source back to agent
        manager
            .alter_table(
                Table::alter()
                    .table(Threads::Table)
                    .rename_column(Threads::Source, Threads::Agent)
                    .to_owned(),
            )
            .await?;

        // Rename output back to answer
        manager
            .alter_table(
                Table::alter()
                    .table(Threads::Table)
                    .rename_column(Threads::Output, Threads::Answer)
                    .to_owned(),
            )
            .await?;

        // Rename input back to question
        manager
            .alter_table(
                Table::alter()
                    .table(Threads::Table)
                    .rename_column(Threads::Input, Threads::Question)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Threads {
    Table,
    Agent,
    SourceType,
    Answer,
    Output,
    Question,
    Input,
    Source,
}
