//! SeaORM migrations for the orchestrator runtime.
//!
//! Uses a **separate tracking table** (`seaql_migrations_orchestrator`) so this
//! migrator is fully independent of the central `crates/migration` migrator.

use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
use sea_orm_migration::prelude::*;

pub struct RuntimeMigrator;

#[async_trait::async_trait]
impl MigratorTrait for RuntimeMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(CreateAgenticTables),
            Box::new(RenameLegacySingularTables),
            Box::new(AddExtensibilityColumns),
            Box::new(DropLegacyDomainColumns),
            Box::new(AddTaskTreeColumns),
            Box::new(CreateTaskOutcomesTable),
            Box::new(AddAttemptColumn),
            Box::new(AddEventAttemptColumn),
            Box::new(CreateTaskQueueTable),
            Box::new(RationalizeStatusModel),
        ]
    }

    fn migration_table_name() -> sea_orm::DynIden {
        Alias::new("seaql_migrations_orchestrator").into_iden()
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

async fn table_exists(manager: &SchemaManager<'_>, table: &str) -> Result<bool, DbErr> {
    let stmt = Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "SELECT 1 FROM information_schema.tables WHERE table_name = $1 LIMIT 1",
        [table.into()],
    );
    Ok(manager.get_connection().query_one(stmt).await?.is_some())
}

async fn column_exists(
    manager: &SchemaManager<'_>,
    table: &str,
    column: &str,
) -> Result<bool, DbErr> {
    let stmt = Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "SELECT 1 FROM information_schema.columns WHERE table_name = $1 AND column_name = $2 LIMIT 1",
        [table.into(), column.into()],
    );
    Ok(manager.get_connection().query_one(stmt).await?.is_some())
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

// ── Iden enums ───────────────────────────────────────────────────────────────

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
    ParentRunId,
    TaskStatus,
    TaskMetadata,
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

// ── Migration 1: Create tables ──────────────────────────────────────────────

struct CreateAgenticTables;

impl MigrationName for CreateAgenticTables {
    fn name(&self) -> &str {
        "m20260317_000001_create_agentic_tables"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for CreateAgenticTables {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // agentic_runs
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

        // agentic_run_events
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
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        // agentic_run_suspensions
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

// ── Migration 2: Rename legacy singular table names ─────────────────────────

struct RenameLegacySingularTables;

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

// ── Migration 3: Add extensibility columns ──────────────────────────────────

struct AddExtensibilityColumns;

impl MigrationName for AddExtensibilityColumns {
    fn name(&self) -> &str {
        "m20260407_000001_add_extensibility_columns"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddExtensibilityColumns {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // Add source_type column if not exists.
        if !column_exists(manager, "agentic_runs", "source_type").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(AgenticRun::Table)
                        .add_column(ColumnDef::new(Alias::new("source_type")).string().null())
                        .to_owned(),
                )
                .await?;
        }

        // Add metadata column if not exists.
        if !column_exists(manager, "agentic_runs", "metadata").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(AgenticRun::Table)
                        .add_column(ColumnDef::new(Alias::new("metadata")).json_binary().null())
                        .to_owned(),
                )
                .await?;
        }

        // Backfill source_type from agent_id for existing rows.
        db.execute_unprepared(
            "UPDATE agentic_runs SET source_type = CASE \
                 WHEN agent_id = '__builder__' THEN 'builder' \
                 ELSE 'analytics' \
             END \
             WHERE source_type IS NULL",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if column_exists(manager, "agentic_runs", "metadata").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(AgenticRun::Table)
                        .drop_column(Alias::new("metadata"))
                        .to_owned(),
                )
                .await?;
        }
        if column_exists(manager, "agentic_runs", "source_type").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(AgenticRun::Table)
                        .drop_column(Alias::new("source_type"))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}

// ── Migration 4: Drop legacy domain-specific columns ────────────────────────

struct DropLegacyDomainColumns;

impl MigrationName for DropLegacyDomainColumns {
    fn name(&self) -> &str {
        "m20260408_000001_drop_legacy_domain_columns"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for DropLegacyDomainColumns {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // These columns have been migrated to analytics_run_extensions.
        // Use IF EXISTS for idempotency.
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE agentic_runs DROP COLUMN IF EXISTS agent_id; \
             ALTER TABLE agentic_runs DROP COLUMN IF EXISTS spec_hint; \
             ALTER TABLE agentic_runs DROP COLUMN IF EXISTS thinking_mode;",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Re-add the columns if needed for rollback.
        let db = manager.get_connection();
        if !column_exists(manager, "agentic_runs", "agent_id").await? {
            db.execute_unprepared(
                "ALTER TABLE agentic_runs ADD COLUMN agent_id TEXT NOT NULL DEFAULT ''",
            )
            .await?;
        }
        if !column_exists(manager, "agentic_runs", "spec_hint").await? {
            db.execute_unprepared("ALTER TABLE agentic_runs ADD COLUMN spec_hint JSONB")
                .await?;
        }
        if !column_exists(manager, "agentic_runs", "thinking_mode").await? {
            db.execute_unprepared("ALTER TABLE agentic_runs ADD COLUMN thinking_mode TEXT")
                .await?;
        }
        Ok(())
    }
}

// ── Migration 5: Add task tree columns ─────────────────────────────────────

struct AddTaskTreeColumns;

impl MigrationName for AddTaskTreeColumns {
    fn name(&self) -> &str {
        "m20260412_000001_add_task_tree_columns"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddTaskTreeColumns {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // parent_run_id: self-referential FK for task tree.
        if !column_exists(manager, "agentic_runs", "parent_run_id").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(AgenticRun::Table)
                        .add_column(ColumnDef::new(AgenticRun::ParentRunId).string().null())
                        .add_foreign_key(
                            TableForeignKey::new()
                                .name("fk_agentic_runs_parent_run_id")
                                .from_tbl(AgenticRun::Table)
                                .from_col(AgenticRun::ParentRunId)
                                .to_tbl(AgenticRun::Table)
                                .to_col(AgenticRun::Id)
                                .on_delete(ForeignKeyAction::Cascade),
                        )
                        .to_owned(),
                )
                .await?;

            manager
                .create_index(
                    Index::create()
                        .name("idx_agentic_runs_parent_run_id")
                        .table(AgenticRun::Table)
                        .col(AgenticRun::ParentRunId)
                        .if_not_exists()
                        .to_owned(),
                )
                .await?;
        }

        // task_status: coordinator's internal status (running, suspended_human,
        // waiting_on_child, done, failed). Distinct from user-facing `status`.
        if !column_exists(manager, "agentic_runs", "task_status").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(AgenticRun::Table)
                        .add_column(ColumnDef::new(AgenticRun::TaskStatus).string().null())
                        .to_owned(),
                )
                .await?;
        }

        // task_metadata: extensible JSONB for coordinator state (child_task_ids, etc.).
        if !column_exists(manager, "agentic_runs", "task_metadata").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(AgenticRun::Table)
                        .add_column(
                            ColumnDef::new(AgenticRun::TaskMetadata)
                                .json_binary()
                                .null(),
                        )
                        .to_owned(),
                )
                .await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE agentic_runs \
             DROP COLUMN IF EXISTS task_metadata, \
             DROP COLUMN IF EXISTS task_status, \
             DROP CONSTRAINT IF EXISTS fk_agentic_runs_parent_run_id, \
             DROP COLUMN IF EXISTS parent_run_id;",
        )
        .await?;
        Ok(())
    }
}

// ── Migration 6: Create task outcomes table ────────────────────────────────
//
// Single source of truth for child→parent result handoff. Written atomically
// before updating parent metadata, closing the crash-consistency window.

#[derive(Iden)]
enum AgenticTaskOutcome {
    #[iden = "agentic_task_outcomes"]
    Table,
    ChildId,
    ParentId,
    Status,
    Answer,
    CreatedAt,
}

struct CreateTaskOutcomesTable;

impl MigrationName for CreateTaskOutcomesTable {
    fn name(&self) -> &str {
        "m20260413_000001_create_task_outcomes_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for CreateTaskOutcomesTable {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if table_exists(manager, "agentic_task_outcomes").await? {
            return Ok(());
        }

        manager
            .create_table(
                Table::create()
                    .table(AgenticTaskOutcome::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AgenticTaskOutcome::ChildId)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(AgenticTaskOutcome::ParentId)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AgenticTaskOutcome::Status)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(AgenticTaskOutcome::Answer).text().null())
                    .col(
                        ColumnDef::new(AgenticTaskOutcome::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(AgenticTaskOutcome::Table, AgenticTaskOutcome::ChildId)
                            .to(AgenticRun::Table, AgenticRun::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_agentic_task_outcomes_parent_id")
                    .table(AgenticTaskOutcome::Table)
                    .col(AgenticTaskOutcome::ParentId)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(AgenticTaskOutcome::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

// ── Migration 7: Add attempt column ───────────────────────────────────────
//
// Tracks recovery attempts. 0 = original run, incremented on each recovery.
// Allows navigating between attempts in the UI.

struct AddAttemptColumn;

impl MigrationName for AddAttemptColumn {
    fn name(&self) -> &str {
        "m20260413_000002_add_attempt_column"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddAttemptColumn {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !column_exists(manager, "agentic_runs", "attempt").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(AgenticRun::Table)
                        .add_column(
                            ColumnDef::new(Alias::new("attempt"))
                                .integer()
                                .not_null()
                                .default(0),
                        )
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if column_exists(manager, "agentic_runs", "attempt").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(AgenticRun::Table)
                        .drop_column(Alias::new("attempt"))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}

// ── Migration 8: Add attempt column to events ─────────────────────────────
//
// Tags each event with its recovery attempt number so the frontend can
// distinguish events from different attempts.

struct AddEventAttemptColumn;

impl MigrationName for AddEventAttemptColumn {
    fn name(&self) -> &str {
        "m20260415_000001_add_event_attempt_column"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddEventAttemptColumn {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !column_exists(manager, "agentic_run_events", "attempt").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(AgenticRunEvent::Table)
                        .add_column(
                            ColumnDef::new(Alias::new("attempt"))
                                .integer()
                                .not_null()
                                .default(0),
                        )
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if column_exists(manager, "agentic_run_events", "attempt").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(AgenticRunEvent::Table)
                        .drop_column(Alias::new("attempt"))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}

// ── Migration 9: Create task queue table ────────────────────────────────────
//
// Durable task queue inspired by Temporal. Assignments are persisted before
// dispatch; workers poll the table. Survives process crashes.

#[derive(Iden)]
enum AgenticTaskQueue {
    #[iden = "agentic_task_queue"]
    Table,
    TaskId,
    RunId,
    ParentTaskId,
    QueueStatus,
    Spec,
    Policy,
    WorkerId,
    LastHeartbeat,
    ClaimedAt,
    VisibilityTimeoutSecs,
    ClaimCount,
    MaxClaims,
    CreatedAt,
    UpdatedAt,
}

struct CreateTaskQueueTable;

impl MigrationName for CreateTaskQueueTable {
    fn name(&self) -> &str {
        "m20260415_000002_create_task_queue_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for CreateTaskQueueTable {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if table_exists(manager, "agentic_task_queue").await? {
            return Ok(());
        }

        manager
            .create_table(
                Table::create()
                    .table(AgenticTaskQueue::Table)
                    .col(
                        ColumnDef::new(AgenticTaskQueue::TaskId)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(AgenticTaskQueue::RunId).string().not_null())
                    .col(
                        ColumnDef::new(AgenticTaskQueue::ParentTaskId)
                            .string()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AgenticTaskQueue::QueueStatus)
                            .string()
                            .not_null()
                            .default("queued"),
                    )
                    .col(
                        ColumnDef::new(AgenticTaskQueue::Spec)
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AgenticTaskQueue::Policy)
                            .json_binary()
                            .null(),
                    )
                    .col(ColumnDef::new(AgenticTaskQueue::WorkerId).string().null())
                    .col(
                        ColumnDef::new(AgenticTaskQueue::LastHeartbeat)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AgenticTaskQueue::ClaimedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AgenticTaskQueue::VisibilityTimeoutSecs)
                            .integer()
                            .not_null()
                            .default(60),
                    )
                    .col(
                        ColumnDef::new(AgenticTaskQueue::ClaimCount)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(AgenticTaskQueue::MaxClaims)
                            .integer()
                            .not_null()
                            .default(3),
                    )
                    .col(
                        ColumnDef::new(AgenticTaskQueue::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AgenticTaskQueue::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(AgenticTaskQueue::Table, AgenticTaskQueue::RunId)
                            .to(AgenticRun::Table, AgenticRun::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Partial index for polling: only queued tasks, ordered by created_at.
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX idx_task_queue_poll \
                 ON agentic_task_queue (created_at) \
                 WHERE queue_status = 'queued'",
            )
            .await?;

        // Partial index for reaper: only claimed tasks, ordered by last_heartbeat.
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX idx_task_queue_reap \
                 ON agentic_task_queue (last_heartbeat) \
                 WHERE queue_status = 'claimed'",
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(AgenticTaskQueue::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

// ── Migration 10: Rationalize status model ──────────────────────────────────
//
// Drop the redundant `status` column (now derived from `task_status` at the API
// layer). Add `recovery_requested_at` column to replace `needs_resume`/`shutdown`
// task_status values. Rename task_status values to Temporal-inspired names.

struct RationalizeStatusModel;

impl MigrationName for RationalizeStatusModel {
    fn name(&self) -> &str {
        "m20260415_000003_rationalize_status_model"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for RationalizeStatusModel {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // Drop the redundant `status` column.
        if column_exists(manager, "agentic_runs", "status").await? {
            db.execute_unprepared("ALTER TABLE agentic_runs DROP COLUMN status")
                .await?;
        }

        // Add recovery_requested_at column.
        if !column_exists(manager, "agentic_runs", "recovery_requested_at").await? {
            db.execute_unprepared(
                "ALTER TABLE agentic_runs ADD COLUMN recovery_requested_at TIMESTAMPTZ",
            )
            .await?;
        }

        // Ensure thread_id column exists (may have been added by central migrator
        // or may be missing in test databases that only run runtime migrations).
        if !column_exists(manager, "agentic_runs", "thread_id").await? {
            db.execute_unprepared("ALTER TABLE agentic_runs ADD COLUMN thread_id UUID")
                .await?;
        }

        // Rename task_status values to new names (idempotent).
        db.execute_unprepared(
            "UPDATE agentic_runs SET task_status = 'awaiting_input' WHERE task_status = 'suspended_human'; \
             UPDATE agentic_runs SET task_status = 'delegating' WHERE task_status IN ('waiting_on_child', 'waiting_on_children'); \
             UPDATE agentic_runs SET recovery_requested_at = updated_at WHERE task_status IN ('needs_resume', 'shutdown'); \
             UPDATE agentic_runs SET task_status = 'running' WHERE task_status IN ('needs_resume', 'shutdown');",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // Re-add status column.
        if !column_exists(manager, "agentic_runs", "status").await? {
            db.execute_unprepared(
                "ALTER TABLE agentic_runs ADD COLUMN status TEXT NOT NULL DEFAULT 'running'",
            )
            .await?;
        }

        // Drop recovery column.
        if column_exists(manager, "agentic_runs", "recovery_requested_at").await? {
            db.execute_unprepared("ALTER TABLE agentic_runs DROP COLUMN recovery_requested_at")
                .await?;
        }

        // Revert task_status renames.
        db.execute_unprepared(
            "UPDATE agentic_runs SET task_status = 'suspended_human' WHERE task_status = 'awaiting_input'; \
             UPDATE agentic_runs SET task_status = 'waiting_on_children' WHERE task_status = 'delegating';",
        )
        .await?;

        Ok(())
    }
}
