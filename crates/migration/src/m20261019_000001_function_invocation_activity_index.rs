use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, Statement};

/// A partial index on `app_function_invocations (app_id, environment,
/// created_at DESC)` over the invocations that count as a sandbox's activity:
/// every mode but a scheduled fire and a webhook delivery.
///
/// A sandbox's idle clock is its newest invocation **that someone made** —
/// a scheduled fire or a webhook delivery would keep a sandbox with its
/// schedules switch on alive forever (environments design §4.5). The three
/// activity queries (`custom_apps_sandboxes::activity`) therefore filter
/// `mode NOT IN ('schedule', 'webhook')`. On the plain
/// `idx_app_function_invocations_app_env_created` that `max(created_at)` walks
/// back through every fire since the last manual call, fetching each row to
/// read its mode — and it is asked on every webhook delivery to a sandbox and
/// every queued fire there (`is_past_idle_expiry`), on an anonymous route. With
/// this index it stops at the first entry again. The predicate is
/// [`ACTIVITY_PREDICATE`], which those queries state verbatim (a test holds
/// them to it), so the planner can prove it applies.
///
/// **Concurrently, outside a transaction**, for the reason
/// `m20261001_000002_function_invocation_environment_index` gives: the table
/// takes a row per invocation. A failed concurrent build leaves an `INVALID`
/// index that `IF NOT EXISTS` would skip forever, so a leftover one is dropped
/// first.
///
/// **Rollback-safe**: an index the previous deploy ignores.
#[derive(DeriveMigrationName)]
pub struct Migration;

const INDEX: &str = "idx_app_function_invocations_activity";

/// The rows the index holds — the activity filter, as the queries spell it.
pub const ACTIVITY_PREDICATE: &str = "mode NOT IN ('schedule', 'webhook')";

async fn drop_if_invalid(manager: &SchemaManager<'_>, index: &str) -> Result<(), DbErr> {
    let db = manager.get_connection();
    let invalid = db
        .query_one_raw(Statement::from_string(
            manager.get_database_backend(),
            format!(
                "SELECT 1 FROM pg_index i JOIN pg_class c ON c.oid = i.indexrelid \
                 WHERE c.relname = '{index}' AND pg_table_is_visible(c.oid) \
                   AND NOT i.indisvalid"
            ),
        ))
        .await?;
    if invalid.is_some() {
        db.execute_unprepared(&format!("DROP INDEX CONCURRENTLY IF EXISTS {index}"))
            .await?;
    }
    Ok(())
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    fn use_transaction(&self) -> Option<bool> {
        Some(false)
    }

    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_if_invalid(manager, INDEX).await?;
        manager
            .get_connection()
            .execute_unprepared(&format!(
                "CREATE INDEX CONCURRENTLY IF NOT EXISTS {INDEX} \
                 ON app_function_invocations (app_id, environment, created_at DESC) \
                 WHERE {ACTIVITY_PREDICATE}"
            ))
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(&format!("DROP INDEX CONCURRENTLY IF EXISTS {INDEX}"))
            .await?;
        Ok(())
    }
}
