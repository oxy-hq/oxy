use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ── simulation_definitions (*.simulation.yml) ─────────────────────────
        // Named, not singleton: a workspace declares a *grid* of worlds, one
        // file per point, so the PK is (revision_id, name) the way
        // `airway_pipelines` is.
        //
        // Its own migration rather than an append to
        // `m20260606_000002_create_compile_boundary` — SeaORM skips `up()` for a
        // migration name it has already recorded, so a database that applied the
        // compile-boundary migration would never receive an appended table.
        manager
            .create_table(
                Table::create()
                    .table(SimulationDefinitions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SimulationDefinitions::RevisionId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SimulationDefinitions::Name)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SimulationDefinitions::FilePath)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SimulationDefinitions::Definition)
                            .json_binary()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(SimulationDefinitions::RevisionId)
                            .col(SimulationDefinitions::Name),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_simulation_definitions_revision")
                            .from(
                                SimulationDefinitions::Table,
                                SimulationDefinitions::RevisionId,
                            )
                            .to(Revisions::Table, Revisions::RevisionId)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(SimulationDefinitions::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Revisions {
    Table,
    RevisionId,
}

#[derive(DeriveIden)]
enum SimulationDefinitions {
    Table,
    RevisionId,
    Name,
    FilePath,
    Definition,
}
