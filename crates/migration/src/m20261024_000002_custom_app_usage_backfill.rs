//! `custom_app_usage_backfill` — one row per workspace: how far copying its
//! custom-app usage history from Oxy's Postgres into its own Airhouse
//! (`oxy_custom_app_usage`) has got.
//!
//! Control-plane data: timestamps, a count and a resume position, no usage
//! content. The row is what the Activity tab and the weekly usage report ask
//! (`oxy_custom_apps::usage_reads`): a workspace whose `finished_at` is set
//! reads Airhouse; every other workspace keeps reading Postgres.
//!
//! - `started_at` — when the first pass began. Set once, on insert.
//! - `finished_at` — set when a pass reached the end of both tables. Unset
//!   until then; nothing unsets it.
//! - `rows_copied` — rows read from Postgres and handed to the sink's
//!   idempotent insert, over every pass. A row the insert skipped because
//!   Airhouse already held its event id still counts: this is progress, not a
//!   count of new Airhouse rows.
//! - `cursor_kind`, `cursor_app_id`, `cursor_at`, `cursor_id` — the last row
//!   handed over (`view` rows first, then `event` rows; within a kind by app,
//!   then time, then id). A pass that stops early resumes after it.
//! - `updated_at` — the last time a pass was queued or wrote the row. The sweep
//!   tries the workspace with the oldest one first, so a workspace whose passes
//!   keep failing does not crowd out the rest.
//!
//! Additive: a new table nothing else references, so the previous release
//! ignores it. Rolling the release back does not undo `finished_at`, and the
//! previous release does not mirror SDK events to Airhouse: reset the table
//! before rolling forward again (`internal-docs/data-placement.md`).

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS custom_app_usage_backfill (
    workspace_id UUID PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ,
    rows_copied BIGINT NOT NULL DEFAULT 0,
    cursor_kind TEXT,
    cursor_app_id UUID,
    cursor_at TIMESTAMPTZ,
    cursor_id UUID,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
"#;

const DOWN_SQL: &str = "DROP TABLE IF EXISTS custom_app_usage_backfill;";

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
