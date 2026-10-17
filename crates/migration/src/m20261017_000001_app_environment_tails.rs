use sea_orm_migration::prelude::*;

/// Who is tailing which custom-app environment right now: one row per open
/// `GET /api/customer-apps/{id}/environments/{name}/tail` stream
/// (`server::api::custom_apps_tail`).
///
/// # Why a table
///
/// A function's log lines are produced on whichever replica or worker ran it,
/// and the stream is held by whichever replica took the `GET`. Producers send
/// lines over Postgres `NOTIFY`, and only for an `(app, environment)` someone
/// is watching — an ungated `NOTIFY` per production invocation would serialise
/// commits on Postgres's notify lock. This table is how a producer on another
/// process learns that someone is: each process reads the live pairs at most
/// once a second. A row per stream, not per pair, so one stream closing never
/// unwatches another's.
///
/// Control plane: ids, an environment name and a lease time — no content. A
/// stream renews `expires_at` while it lives and deletes its row when it ends;
/// a row a dead process left behind stops counting at `expires_at`, and the
/// next registration sweeps it.
///
/// # Rollback
///
/// Additive: a new table the previous deploy never reads. On a re-pinned
/// binary nothing registers or reads a row, and there is no tail route.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS app_environment_tails (\
               id UUID PRIMARY KEY, \
               app_id UUID NOT NULL REFERENCES apps(id) ON DELETE CASCADE, \
               environment TEXT NOT NULL, \
               expires_at TIMESTAMPTZ NOT NULL, \
               created_at TIMESTAMPTZ NOT NULL DEFAULT now()\
             )",
        )
        .await?;
        // The producers' once-a-second read is `WHERE expires_at > now()`.
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_app_environment_tails_expires \
             ON app_environment_tails (expires_at)",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS app_environment_tails")
            .await?;
        Ok(())
    }
}
