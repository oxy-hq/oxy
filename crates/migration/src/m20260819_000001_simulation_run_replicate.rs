use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Which draw of a world this run is.
        //
        // A world declares `replicates:` because how many seeds it takes before
        // a cell of the outcome map can be called is a property of how noisy the
        // world is. One seed on a marginal world classifies the draw, not the
        // world — the 6-panel corner returns a sign-flipped estimate, and a
        // single run there would report `confidently_wrong` as if it meant
        // something.
        //
        // The seed itself is already stored, and every replicate carries its own
        // spec snapshot with the derived seed in it. This column exists so
        // "group by (simulation, policy) and aggregate across draws" is a query
        // rather than an inference from seed arithmetic.
        manager
            .alter_table(
                Table::alter()
                    .table(SimulationRuns::Table)
                    .add_column_if_not_exists(
                        ColumnDef::new(SimulationRuns::Replicate)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(SimulationRuns::Table)
                    .drop_column(SimulationRuns::Replicate)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum SimulationRuns {
    Table,
    Replicate,
}
