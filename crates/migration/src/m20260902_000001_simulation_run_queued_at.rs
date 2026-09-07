use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // When the run was enqueued, as distinct from when a worker claimed it.
        //
        // `started_at` used to be stamped by the handler at enqueue time and
        // never touched again, so `finished_at - started_at` was queue wait
        // plus runtime, and a run that sat behind a busy fleet for ten minutes
        // read as a ten-minute run. Now `queued_at` is the handler's clock and
        // `started_at` is the worker's — equal until a claim, which is what
        // keeps `started_at` NOT NULL for the listing index that orders on it.
        manager
            .alter_table(
                Table::alter()
                    .table(SimulationRuns::Table)
                    .add_column_if_not_exists(
                        ColumnDef::new(SimulationRuns::QueuedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // Backfill from the column that WAS the enqueue time. The default above
        // stamps every existing row with the migration's own clock, which is
        // later than any of their `started_at`s — so the predicate only
        // touches backfilled rows and the statement is safe to re-run.
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE simulation_runs SET queued_at = started_at \
                 WHERE queued_at > started_at",
            )
            .await?;

        // Listings now order on `queued_at`; the `started_at` index stays for
        // anything still reading "most recently claimed".
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS idx_simulation_runs_workspace_queued \
                 ON simulation_runs (workspace_id, queued_at DESC)",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS idx_simulation_runs_workspace_queued")
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(SimulationRuns::Table)
                    .drop_column(SimulationRuns::QueuedAt)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum SimulationRuns {
    Table,
    QueuedAt,
}
