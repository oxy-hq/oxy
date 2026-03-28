//! SeaORM migrations — run once on startup via `Migrator::up`.

use sea_orm::{DatabaseBackend, Statement};
use sea_orm_migration::prelude::*;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(CreateAgenticTables),
            Box::new(RenameLegacySingularTables),
        ]
    }
}

struct CreateAgenticTables;
struct RenameLegacySingularTables;

impl MigrationName for CreateAgenticTables {
    fn name(&self) -> &str {
        "m20250101_000001_create_agentic_tables"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for CreateAgenticTables {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ── agentic_runs ──────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(AgenticRun::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AgenticRun::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(AgenticRun::AgentId).string().not_null())
                    .col(ColumnDef::new(AgenticRun::Question).text().not_null())
                    .col(
                        ColumnDef::new(AgenticRun::Status)
                            .string()
                            .not_null()
                            .default("running"),
                    )
                    .col(ColumnDef::new(AgenticRun::Answer).text().null())
                    .col(ColumnDef::new(AgenticRun::ErrorMessage).text().null())
                    .col(
                        ColumnDef::new(AgenticRun::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AgenticRun::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // ── agentic_run_events ────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(AgenticRunEvent::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AgenticRunEvent::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(AgenticRunEvent::RunId).string().not_null())
                    .col(
                        ColumnDef::new(AgenticRunEvent::Seq)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AgenticRunEvent::EventType)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AgenticRunEvent::Payload)
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AgenticRunEvent::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(AgenticRunEvent::Table, AgenticRunEvent::RunId)
                            .to(AgenticRun::Table, AgenticRun::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_agentic_run_events_run_id_seq")
                    .table(AgenticRunEvent::Table)
                    .col(AgenticRunEvent::RunId)
                    .col(AgenticRunEvent::Seq)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // ── agentic_run_suspensions ───────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(AgenticRunSuspension::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AgenticRunSuspension::RunId)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(AgenticRunSuspension::Prompt)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AgenticRunSuspension::Suggestions)
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AgenticRunSuspension::ResumeData)
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AgenticRunSuspension::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(AgenticRunSuspension::Table, AgenticRunSuspension::RunId)
                            .to(AgenticRun::Table, AgenticRun::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AgenticRunSuspension::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(AgenticRunEvent::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(AgenticRun::Table).to_owned())
            .await?;
        Ok(())
    }
}

impl MigrationName for RenameLegacySingularTables {
    fn name(&self) -> &str {
        "m20260317_000002_rename_legacy_singular_tables"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for RenameLegacySingularTables {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rename_table_if_needed(manager, "agentic_run_suspension", "agentic_run_suspensions")
            .await?;
        rename_table_if_needed(manager, "agentic_run_event", "agentic_run_events").await?;
        rename_table_if_needed(manager, "agentic_run", "agentic_runs").await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rename_table_if_needed(manager, "agentic_run_suspensions", "agentic_run_suspension")
            .await?;
        rename_table_if_needed(manager, "agentic_run_events", "agentic_run_event").await?;
        rename_table_if_needed(manager, "agentic_runs", "agentic_run").await?;
        Ok(())
    }
}

async fn rename_table_if_needed(
    manager: &SchemaManager<'_>,
    from: &str,
    to: &str,
) -> Result<(), DbErr> {
    if table_exists(manager, to).await? || !table_exists(manager, from).await? {
        return Ok(());
    }

    manager
        .rename_table(
            Table::rename()
                .table(Alias::new(from), Alias::new(to))
                .to_owned(),
        )
        .await
}

async fn table_exists(manager: &SchemaManager<'_>, table: &str) -> Result<bool, DbErr> {
    let stmt = Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ? LIMIT 1",
        [table.into()],
    );

    Ok(manager.get_connection().query_one(stmt).await?.is_some())
}

// ── Iden enums ────────────────────────────────────────────────────────────────

#[derive(Iden)]
enum AgenticRun {
    #[iden = "agentic_runs"]
    Table,
    Id,
    AgentId,
    Question,
    Status,
    Answer,
    ErrorMessage,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
enum AgenticRunEvent {
    #[iden = "agentic_run_events"]
    Table,
    Id,
    RunId,
    Seq,
    EventType,
    Payload,
    CreatedAt,
}

#[derive(Iden)]
enum AgenticRunSuspension {
    #[iden = "agentic_run_suspensions"]
    Table,
    RunId,
    Prompt,
    Suggestions,
    ResumeData,
    CreatedAt,
}
