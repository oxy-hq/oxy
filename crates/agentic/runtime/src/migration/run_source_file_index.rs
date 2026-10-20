//! Index for the run history of one automation or pipeline file.
//!
//! oxy-app groups a source's failed runs into a streak, so a pipeline failing
//! every hour notifies once and then daily (`notifications/platform/run_streak.rs`).
//! That reads one file's root runs in one workspace, both when a run fails and
//! when an inbox page renders a run notice. The only index on those columns
//! was `idx_agentic_runs_workspace_status`, so the read visited every finished
//! run of the workspace, chat runs and subruns included, to find the few that
//! ran the file. This index holds root automation and Airway runs only, keyed
//! by the file each ran, so the read costs that file's runs and nothing else.
//!
//! **Frozen DDL, coupled to a query.** Postgres uses an expression index only
//! when the query writes the same expression and implies the predicate. The
//! streak query's tests assert both against [`RUN_SOURCE_FILE_INDEX_SQL`];
//! when they disagree, the fix is a NEW migration, never an edit to this one.

use sea_orm_migration::prelude::*;

/// The shipped DDL. The key's `CASE` is the file a run ran: `pipeline_ref` for
/// an Airway run, `workflow_ref` for an automation run.
pub const RUN_SOURCE_FILE_INDEX_SQL: &str = "CREATE INDEX IF NOT EXISTS idx_agentic_runs_root_source_file \
     ON agentic_runs (workspace_id, source_type, \
       (CASE source_type WHEN 'airway' THEN metadata->>'pipeline_ref' WHEN 'workflow' THEN metadata->>'workflow_ref' END), \
       created_at) \
     WHERE parent_run_id IS NULL AND source_type IN ('airway', 'workflow')";

pub struct AddRunSourceFileIndex;

/// Named by hand: `DeriveMigrationName` takes the module path (see
/// `AddCompileTaskBackoffIndex`).
impl MigrationName for AddRunSourceFileIndex {
    fn name(&self) -> &str {
        "m20261020_000001_add_run_source_file_index"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddRunSourceFileIndex {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(RUN_SOURCE_FILE_INDEX_SQL)
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS idx_agentic_runs_root_source_file")
            .await?;
        Ok(())
    }
}
