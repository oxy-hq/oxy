use sea_orm_migration::prelude::*;

/// `audit_events` becomes append-only **at the database**, not by convention.
///
/// The hash chain makes tampering *evident* only if a verifier ever runs; a
/// `BEFORE UPDATE OR DELETE` trigger makes the common tamper — an operator
/// with a psql session editing or deleting a row — fail outright, and leaves
/// the chain break detectable only for someone who can also drop the trigger
/// (which the anchor job's S3 Object Lock copies then expose).
///
/// One door stays open on purpose: the retention prune deletes rows past the
/// window, so DELETE is permitted when the session has
/// `SET LOCAL oxy.audit_prune = 'on'` — a transaction-scoped flag only
/// `oxy_app_core::audit::prune_older_than` sets. That flag is a plain session
/// GUC any role can set, so the DELETE guard is *advisory* against an
/// operator who has read this file; the S3 anchor is what makes a deletion
/// evident. UPDATE and TRUNCATE are never permitted — TRUNCATE needs its own
/// statement-level trigger, because a row-level one never fires for it.
#[derive(DeriveMigrationName)]
pub struct Migration;

const GUARD_FUNCTION: &str = "audit_events_append_only_guard";
const GUARD_TRIGGER: &str = "audit_events_append_only";
const TRUNCATE_FUNCTION: &str = "audit_events_no_truncate";
const TRUNCATE_TRIGGER: &str = "audit_events_no_truncate";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() != sea_orm::DbBackend::Postgres {
            return Ok(());
        }
        let conn = manager.get_connection();
        conn.execute_unprepared(&format!(
            r#"CREATE OR REPLACE FUNCTION {GUARD_FUNCTION}() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'UPDATE' THEN
    RAISE EXCEPTION 'audit_events is append-only: UPDATE is never permitted (seq %)', OLD.seq
      USING ERRCODE = 'insufficient_privilege';
  END IF;
  IF current_setting('oxy.audit_prune', true) IS DISTINCT FROM 'on' THEN
    RAISE EXCEPTION 'audit_events is append-only: DELETE is permitted only to the retention prune (seq %)', OLD.seq
      USING ERRCODE = 'insufficient_privilege';
  END IF;
  RETURN OLD;
END
$$"#
        ))
        .await?;
        conn.execute_unprepared(&format!(
            "DROP TRIGGER IF EXISTS {GUARD_TRIGGER} ON audit_events"
        ))
        .await?;
        conn.execute_unprepared(&format!(
            "CREATE TRIGGER {GUARD_TRIGGER} BEFORE UPDATE OR DELETE ON audit_events \
             FOR EACH ROW EXECUTE FUNCTION {GUARD_FUNCTION}()"
        ))
        .await?;
        // `OLD` is unassigned in a statement-level trigger, so TRUNCATE gets
        // its own function rather than a confusing plpgsql error.
        conn.execute_unprepared(&format!(
            r#"CREATE OR REPLACE FUNCTION {TRUNCATE_FUNCTION}() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'audit_events is append-only: TRUNCATE is never permitted'
    USING ERRCODE = 'insufficient_privilege';
END
$$"#
        ))
        .await?;
        conn.execute_unprepared(&format!(
            "DROP TRIGGER IF EXISTS {TRUNCATE_TRIGGER} ON audit_events"
        ))
        .await?;
        conn.execute_unprepared(&format!(
            "CREATE TRIGGER {TRUNCATE_TRIGGER} BEFORE TRUNCATE ON audit_events \
             FOR EACH STATEMENT EXECUTE FUNCTION {TRUNCATE_FUNCTION}()"
        ))
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() != sea_orm::DbBackend::Postgres {
            return Ok(());
        }
        let conn = manager.get_connection();
        conn.execute_unprepared(&format!(
            "DROP TRIGGER IF EXISTS {GUARD_TRIGGER} ON audit_events"
        ))
        .await?;
        conn.execute_unprepared(&format!("DROP FUNCTION IF EXISTS {GUARD_FUNCTION}()"))
            .await?;
        conn.execute_unprepared(&format!(
            "DROP TRIGGER IF EXISTS {TRUNCATE_TRIGGER} ON audit_events"
        ))
        .await?;
        conn.execute_unprepared(&format!("DROP FUNCTION IF EXISTS {TRUNCATE_FUNCTION}()"))
            .await?;
        Ok(())
    }
}
