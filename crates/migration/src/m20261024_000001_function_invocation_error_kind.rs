//! `app_function_invocations.error_kind` — how an invocation failed, stored
//! with the failure instead of re-read from its text.
//!
//! Until now the kind (`threw`, `internal`, `exceeded_memory`, `platform`,
//! `timeout`, `http_5xx`, `host_call`) was recovered from `error` by reading the
//! start of the message (`failure_signal::Failure::of`), because the run outcome
//! carried only the text even though the host already held a typed
//! `HostError`. This column keeps the kind beside the text. The text is
//! unchanged and stays the contract.
//!
//! NULL for a run that did not fail and for every row written before this
//! column, which readers still classify from the text.
//!
//! Expand-only: one nullable column, no default, no index, no constraint, no
//! backfill. A binary one release back never reads it and writes it as NULL, so
//! during a revert new failures carry no kind and are read from their text, as
//! they were before.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP_SQL: &str =
    "ALTER TABLE app_function_invocations ADD COLUMN IF NOT EXISTS error_kind TEXT;";

const DOWN_SQL: &str = "ALTER TABLE app_function_invocations DROP COLUMN IF EXISTS error_kind;";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(UP_SQL).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(DOWN_SQL)
            .await?;
        Ok(())
    }
}
