use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ── simulation_runs ───────────────────────────────────────────────────
        // One row per run of a declared world.
        //
        // `spec` is a snapshot, not a pointer. A run is *evidence* — the whole
        // exercise is comparing an estimate against a truth we wrote down — and
        // a run that could only be read alongside the current
        // `.simulation.yml` would silently re-interpret itself the next time
        // someone retuned the world. `truth` is the one place the world's real
        // parameters are allowed to land (see the architecture diagram in the
        // plan): the scorer writes it, and nothing in the loop reads it back.
        manager
            .create_table(
                Table::create()
                    .table(SimulationRuns::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SimulationRuns::RunId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(SimulationRuns::WorkspaceId)
                            .uuid()
                            .not_null(),
                    )
                    .col(ColumnDef::new(SimulationRuns::RevisionId).uuid().null())
                    .col(
                        ColumnDef::new(SimulationRuns::SimulationName)
                            .text()
                            .not_null(),
                    )
                    .col(ColumnDef::new(SimulationRuns::Policy).text().not_null())
                    .col(
                        ColumnDef::new(SimulationRuns::Seed)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(SimulationRuns::Status).text().not_null())
                    .col(
                        ColumnDef::new(SimulationRuns::Spec)
                            .json_binary()
                            .not_null(),
                    )
                    .col(ColumnDef::new(SimulationRuns::Truth).json_binary().null())
                    .col(
                        ColumnDef::new(SimulationRuns::PeriodsPlanned)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SimulationRuns::PeriodsDone)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(SimulationRuns::StartedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SimulationRuns::FinishedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(ColumnDef::new(SimulationRuns::Error).text().null())
                    .to_owned(),
            )
            .await?;

        // Every listing is "this workspace's runs, newest first" — and the
        // workspace filter is a correctness invariant here, not an optimisation.
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS idx_simulation_runs_workspace_started \
                 ON simulation_runs (workspace_id, started_at DESC)",
            )
            .await?;

        // ── simulation_run_periods ────────────────────────────────────────────
        // What the policy did and what it earned, once per decision period.
        // Per-period rather than per-edge, because an action and a profit belong
        // to the period; the fits belong to the edges and live next door.
        manager
            .create_table(
                Table::create()
                    .table(SimulationRunPeriods::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SimulationRunPeriods::RunId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SimulationRunPeriods::Period)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SimulationRunPeriods::MeanSpend)
                            .double()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SimulationRunPeriods::RealizedProfit)
                            .double()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SimulationRunPeriods::CumulativeProfit)
                            .double()
                            .not_null(),
                    )
                    // Per-entity spend, so a `machine+explore` run can be asked
                    // how much variation its jitter actually left behind —
                    // which is the entire question that arm exists to answer.
                    .col(
                        ColumnDef::new(SimulationRunPeriods::Actions)
                            .json_binary()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(SimulationRunPeriods::RunId)
                            .col(SimulationRunPeriods::Period),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_simulation_run_periods_run")
                            .from(SimulationRunPeriods::Table, SimulationRunPeriods::RunId)
                            .to(SimulationRuns::Table, SimulationRuns::RunId)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // ── simulation_run_fits ───────────────────────────────────────────────
        // β̂ against β_true, per edge per period. This table IS the convergence
        // chart and the truth badge; both are queries over it rather than
        // anything recomputed at render time.
        manager
            .create_table(
                Table::create()
                    .table(SimulationRunFits::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(SimulationRunFits::RunId).uuid().not_null())
                    .col(
                        ColumnDef::new(SimulationRunFits::Period)
                            .integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(SimulationRunFits::Edge).text().not_null())
                    // The basis the fitter chose (`linear`, `log-log`, …). A
                    // coefficient is meaningless without it: an elasticity read
                    // as a level slope is wrong by `target / driver`.
                    .col(
                        ColumnDef::new(SimulationRunFits::Form)
                            .text()
                            .not_null()
                            .default("linear"),
                    )
                    // Null exactly when the fit was refused — the refusal string
                    // carries why. Storing a 0.0 here instead would erase the
                    // distinction the whole taxonomy turns on.
                    .col(
                        ColumnDef::new(SimulationRunFits::Coefficient)
                            .double()
                            .null(),
                    )
                    .col(ColumnDef::new(SimulationRunFits::Se).double().null())
                    .col(ColumnDef::new(SimulationRunFits::TStat).double().null())
                    .col(ColumnDef::new(SimulationRunFits::N).integer().not_null())
                    .col(
                        ColumnDef::new(SimulationRunFits::NPanels)
                            .integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(SimulationRunFits::Refusal).text().null())
                    // The true marginal response at the spend the world actually
                    // settled at this period — NOT at the anchor the curve was
                    // calibrated from. Scoring against the anchor books a
                    // modelling difference as estimator bias.
                    .col(
                        ColumnDef::new(SimulationRunFits::TrueLocalSlope)
                            .double()
                            .not_null(),
                    )
                    // `refused` | `converged` | `confidently_wrong`.
                    .col(ColumnDef::new(SimulationRunFits::Outcome).text().not_null())
                    .primary_key(
                        Index::create()
                            .col(SimulationRunFits::RunId)
                            .col(SimulationRunFits::Period)
                            .col(SimulationRunFits::Edge),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_simulation_run_fits_run")
                            .from(SimulationRunFits::Table, SimulationRunFits::RunId)
                            .to(SimulationRuns::Table, SimulationRuns::RunId)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS idx_simulation_runs_workspace_started")
            .await?;
        for table in [
            SimulationRunFits::Table.into_iden(),
            SimulationRunPeriods::Table.into_iden(),
            SimulationRuns::Table.into_iden(),
        ] {
            manager
                .drop_table(Table::drop().table(table).if_exists().to_owned())
                .await?;
        }
        Ok(())
    }
}

#[derive(DeriveIden)]
enum SimulationRuns {
    Table,
    RunId,
    WorkspaceId,
    RevisionId,
    SimulationName,
    Policy,
    Seed,
    Status,
    Spec,
    Truth,
    PeriodsPlanned,
    PeriodsDone,
    StartedAt,
    FinishedAt,
    Error,
}

#[derive(DeriveIden)]
enum SimulationRunPeriods {
    Table,
    RunId,
    Period,
    MeanSpend,
    RealizedProfit,
    CumulativeProfit,
    Actions,
}

#[derive(DeriveIden)]
enum SimulationRunFits {
    Table,
    RunId,
    Period,
    Edge,
    Form,
    Coefficient,
    Se,
    TStat,
    N,
    NPanels,
    Refusal,
    TrueLocalSlope,
    Outcome,
}
