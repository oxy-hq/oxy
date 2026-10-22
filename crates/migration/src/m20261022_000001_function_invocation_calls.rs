//! `app_function_invocation_calls` — the call trace of one Oxy Function
//! invocation: every `ctx.*` host call it made, as a timed record, for the
//! console's waterfall (`GET /api/customer-apps/{id}/invocations/{id}/calls`).
//!
//! One row per invocation that reached tenant code, written once after the run
//! settles and never updated. `calls` is `{"v":1,"calls":[…]}`: per call its
//! registry op, start offset, duration, status, error class and the
//! identifiers its F2 span already records (verb, table, fetch host) — never
//! SQL, values, URL paths, bodies or messages. Control-plane data, pruned after
//! `OXY_APP_CALL_TRACE_RETENTION_DAYS` (3) by the custom-app tracking cleanup
//! loop; the `created_at` index is that prune's.
//!
//! The foreign key cascades from the invocation row, which cascades from its
//! build, so draft-build retention and app deletion clean traces too.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const UP_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS app_function_invocation_calls (
    invocation_id UUID PRIMARY KEY
        REFERENCES app_function_invocations(id) ON DELETE CASCADE,
    app_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    init_ms INTEGER,
    total_calls INTEGER NOT NULL,
    dropped INTEGER NOT NULL DEFAULT 0,
    unfinished INTEGER NOT NULL DEFAULT 0,
    calls JSONB NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_app_function_invocation_calls_created_at
    ON app_function_invocation_calls (created_at);
"#;

const DOWN_SQL: &str = "DROP TABLE IF EXISTS app_function_invocation_calls;";

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
