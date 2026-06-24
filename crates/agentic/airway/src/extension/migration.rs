//! Airway extension migrations.
//!
//! Uses a separate tracking table (`seaql_migrations_airway`) so this
//! migrator evolves independently of the runtime, automation, and
//! analytics migrators. Run in the standard startup order after the
//! runtime migrator (which owns `agentic_runs`, the target of
//! `airway_run_extensions.run_id`'s FK).

use sea_orm_migration::prelude::*;

pub struct AirwayMigrator;

#[async_trait::async_trait]
impl MigratorTrait for AirwayMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(CreatePipelineState),
            Box::new(CreateLoadAudit),
            Box::new(CreateRunExtensions),
            Box::new(AddPartialToLoadAudit),
        ]
    }

    fn migration_table_name() -> sea_orm::DynIden {
        Alias::new("seaql_migrations_airway").into_iden()
    }
}

// ── Iden enums ───────────────────────────────────────────────────────────────

#[derive(Iden)]
enum AirwayPipelineState {
    #[iden = "airway_pipeline_state"]
    Table,
    PipelineName,
    State,
    SchemaJson,
    Version,
    UpdatedAt,
}

#[derive(Iden)]
enum AirwayLoadAudit {
    #[iden = "airway_load_audit"]
    Table,
    LoadId,
    PipelineName,
    SchemaHash,
    Status,
    ErrorMessage,
    Partial,
    StartedAt,
    FinishedAt,
}

#[derive(Iden)]
enum AirwayRunExtensions {
    #[iden = "airway_run_extensions"]
    Table,
    RunId,
    PipelineName,
    PipelineRef,
    LoadId,
    Concurrency,
    Resources,
}

/// Mirror of the runtime's `agentic_runs` table — only the `Id`
/// column is referenced for the FK target on `airway_run_extensions`.
#[derive(Iden)]
enum AgenticRun {
    #[iden = "agentic_runs"]
    Table,
    Id,
}

// ── Migration 1: airway_pipeline_state ──────────────────────────────────────

struct CreatePipelineState;

impl MigrationName for CreatePipelineState {
    fn name(&self) -> &str {
        "m20260520_000001_create_airway_pipeline_state"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for CreatePipelineState {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(AirwayPipelineState::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AirwayPipelineState::PipelineName)
                            .text()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(AirwayPipelineState::State)
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AirwayPipelineState::SchemaJson)
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AirwayPipelineState::Version)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(AirwayPipelineState::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(AirwayPipelineState::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

// ── Migration 2: airway_load_audit ──────────────────────────────────────────

struct CreateLoadAudit;

impl MigrationName for CreateLoadAudit {
    fn name(&self) -> &str {
        "m20260520_000002_create_airway_load_audit"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for CreateLoadAudit {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(AirwayLoadAudit::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AirwayLoadAudit::LoadId)
                            .text()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(AirwayLoadAudit::PipelineName)
                            .text()
                            .not_null(),
                    )
                    // Nullable: airway's `Schema.version_hash` is `Option<String>`.
                    // On the first-ever load for a pipeline the state store
                    // hasn't seen a schema yet, so there's no hash to record.
                    // NULL distinguishes "no prior schema" from "ran with an
                    // empty schema."
                    .col(ColumnDef::new(AirwayLoadAudit::SchemaHash).text().null())
                    .col(
                        ColumnDef::new(AirwayLoadAudit::Status)
                            .text()
                            .not_null()
                            .default("in_progress"),
                    )
                    .col(ColumnDef::new(AirwayLoadAudit::ErrorMessage).text().null())
                    .col(
                        ColumnDef::new(AirwayLoadAudit::StartedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(AirwayLoadAudit::FinishedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;

        // Common query: "audit history for this pipeline, newest first".
        manager
            .create_index(
                Index::create()
                    .name("idx_airway_load_audit_pipeline_name")
                    .table(AirwayLoadAudit::Table)
                    .col(AirwayLoadAudit::PipelineName)
                    .col(AirwayLoadAudit::StartedAt)
                    .if_not_exists()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(AirwayLoadAudit::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

// ── Migration 3: airway_run_extensions ──────────────────────────────────────

struct CreateRunExtensions;

impl MigrationName for CreateRunExtensions {
    fn name(&self) -> &str {
        "m20260520_000003_create_airway_run_extensions"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for CreateRunExtensions {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(AirwayRunExtensions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AirwayRunExtensions::RunId)
                            .text()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(AirwayRunExtensions::PipelineName)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AirwayRunExtensions::PipelineRef)
                            .text()
                            .null(),
                    )
                    .col(ColumnDef::new(AirwayRunExtensions::LoadId).text().null())
                    .col(
                        ColumnDef::new(AirwayRunExtensions::Concurrency)
                            .integer()
                            .not_null()
                            .default(1),
                    )
                    .col(
                        ColumnDef::new(AirwayRunExtensions::Resources)
                            .json_binary()
                            .not_null()
                            .default("[]"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(AirwayRunExtensions::Table, AirwayRunExtensions::RunId)
                            .to(AgenticRun::Table, AgenticRun::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(AirwayRunExtensions::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

// ── Migration 4: airway_load_audit.partial ──────────────────────────────────

/// Adds the `partial` flag distinguishing a clean load from a
/// completed-with-errors one (streaming per-batch commit). Separate
/// migration — never edit a shipped one — so both fresh and existing
/// databases converge.
struct AddPartialToLoadAudit;

impl MigrationName for AddPartialToLoadAudit {
    fn name(&self) -> &str {
        "m20260520_000004_add_partial_to_airway_load_audit"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddPartialToLoadAudit {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(AirwayLoadAudit::Table)
                    .add_column(
                        ColumnDef::new(AirwayLoadAudit::Partial)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(AirwayLoadAudit::Table)
                    .drop_column(AirwayLoadAudit::Partial)
                    .to_owned(),
            )
            .await
    }
}
