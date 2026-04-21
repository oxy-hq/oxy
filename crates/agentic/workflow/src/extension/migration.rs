//! Workflow extension migrations.
//!
//! Uses a separate tracking table (`seaql_migrations_workflow`) so this
//! migrator is independent of the runtime and analytics migrators.

use sea_orm_migration::prelude::*;

pub struct WorkflowMigrator;

#[async_trait::async_trait]
impl MigratorTrait for WorkflowMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(CreateWorkflowState)]
    }

    fn migration_table_name() -> sea_orm::DynIden {
        Alias::new("seaql_migrations_workflow").into_iden()
    }
}

// ── Iden ─────────────────────────────────────────────────────────────────────

#[derive(Iden)]
enum WorkflowState {
    #[iden = "agentic_workflow_state"]
    Table,
    RunId,
    WorkflowYamlHash,
    WorkflowConfig,
    WorkflowContext,
    Variables,
    TraceId,
    CurrentStep,
    Results,
    RenderContext,
    PendingChildren,
    DecisionVersion,
    UpdatedAt,
}

#[derive(Iden)]
enum AgenticRun {
    #[iden = "agentic_runs"]
    Table,
    Id,
}

// ── Migration 1: Create agentic_workflow_state ────────────────────────────────

struct CreateWorkflowState;

impl MigrationName for CreateWorkflowState {
    fn name(&self) -> &str {
        "m20260416_000001_create_agentic_workflow_state"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for CreateWorkflowState {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(WorkflowState::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(WorkflowState::RunId)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(WorkflowState::WorkflowYamlHash)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorkflowState::WorkflowConfig)
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorkflowState::WorkflowContext)
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorkflowState::Variables)
                            .json_binary()
                            .null(),
                    )
                    .col(ColumnDef::new(WorkflowState::TraceId).string().not_null())
                    .col(
                        ColumnDef::new(WorkflowState::CurrentStep)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(WorkflowState::Results)
                            .json_binary()
                            .not_null()
                            .default("{}"),
                    )
                    .col(
                        ColumnDef::new(WorkflowState::RenderContext)
                            .json_binary()
                            .not_null()
                            .default("{}"),
                    )
                    .col(
                        ColumnDef::new(WorkflowState::PendingChildren)
                            .json_binary()
                            .not_null()
                            .default("{}"),
                    )
                    .col(
                        ColumnDef::new(WorkflowState::DecisionVersion)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(WorkflowState::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(WorkflowState::Table, WorkflowState::RunId)
                            .to(AgenticRun::Table, AgenticRun::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(WorkflowState::Table).to_owned())
            .await
    }
}
