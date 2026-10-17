use sea_orm_migration::prelude::*;

/// Deep notification settings: on/off per channel and a level floor at every
/// node of a source's tree, a person's pause and quiet hours, and per-device
/// settings and OS state.
///
/// # Why
///
/// Until now a person could only turn a channel *off*, in three tables (one
/// per node shape). Settings now live at four levels — the person, each node
/// of the tree (source, category path, kind), each device, and `critical`,
/// which overrides the first two — and the nearest explicit setting wins, so
/// a kind can be turned back *on* under a category that is off. That needs an
/// explicit "on", which a mute table cannot hold
/// (`internal-docs/custom-app-notifications.md`, "Settings").
///
/// - `notification_node_settings` — one row per (person, source, node):
///   `push` / `email` / `slack` each `true`, `false` or `NULL` (inherit), and
///   `min_level` (`NULL` inherits). `app_id` is `NULL` for Oxygen, so the
///   uniqueness rules are two partial indexes, as for
///   `notification_scope_mutes`. `node_type` says whether `node` is a scope
///   (`''` or a category path) or a kind: a kind and a category may share a
///   name.
/// - `notification_user_settings` — one row per person: `paused_until`,
///   quiet hours (start and end in minutes after local midnight, the days a
///   window starts on as bits, the lowest level let through), the IANA
///   `timezone` they are read in, the settings document's `version`, and how
///   far the quiet-hours summary has reached.
/// - `device_tokens` gains `enabled` (default on), `min_level`,
///   `os_permission`, `os_disabled` (JSON array) and `app_version`.
///
/// # Backfill
///
/// [`BACKFILL_SQL`] copies every row of the three mute tables into
/// `notification_node_settings` as an explicit `false`, one row per node.
/// Delivery reads only the new table from this release on.
///
/// # Rollback
///
/// Additive. Two new tables the previous deploy never reads, and five
/// columns it never names: its entity selects its own columns and its
/// inserts leave these to their defaults (`enabled` is `NOT NULL DEFAULT
/// true`, a catalog-only change, the rest nullable). No constraint is added
/// to `device_tokens`; its values are checked in code.
///
/// The three mute tables stay. This release still writes every mute to them
/// as well, so a re-pinned binary keeps the mutes made since; it loses only
/// what they cannot hold (an explicit "on", a floor, pause, quiet hours,
/// device settings). What the re-pinned binary writes to them is brought back
/// on roll-forward by a reconcile at every boot
/// (`notifications::settings::reconcile`), since this backfill has run by
/// then. Contracting them is two later deploys: stop the mirror writes
/// (`settings::store::mirror_mute`) and the reconcile together, then drop
/// the tables.
///
/// # Locking
///
/// `ALTER TABLE device_tokens` takes an ACCESS EXCLUSIVE lock for a
/// catalog-only change (nullable columns, and one with a constant default).
#[derive(DeriveMigrationName)]
pub struct Migration;

/// Every mute in the three old tables, as an explicit `false` at its node.
/// Idempotent: a node that already has a row keeps it.
pub const BACKFILL_SQL: &str = "\
    INSERT INTO notification_node_settings \
      (id, user_id, app_id, node_type, node, push, email, slack) \
    SELECT gen_random_uuid(), user_id, app_id, node_type, node, \
           CASE WHEN bool_or(channel = 'push') THEN false END, \
           CASE WHEN bool_or(channel = 'email') THEN false END, \
           CASE WHEN bool_or(channel = 'slack') THEN false END \
    FROM ( \
      SELECT user_id, app_id, 'kind' AS node_type, kind AS node, channel \
        FROM notification_mutes \
      UNION ALL \
      SELECT user_id, NULL::uuid, 'kind', kind, channel \
        FROM notification_platform_mutes \
      UNION ALL \
      SELECT user_id, app_id, 'scope', scope, channel \
        FROM notification_scope_mutes \
    ) mutes \
    GROUP BY user_id, app_id, node_type, node \
    ON CONFLICT DO NOTHING";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS notification_node_settings (\
               id UUID PRIMARY KEY, \
               user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE, \
               app_id UUID REFERENCES apps(id) ON DELETE CASCADE, \
               node_type TEXT NOT NULL CHECK (node_type IN ('scope','kind')), \
               node TEXT NOT NULL, \
               push BOOLEAN, \
               email BOOLEAN, \
               slack BOOLEAN, \
               min_level TEXT CHECK (min_level IN ('passive','active','time_sensitive','critical')), \
               updated_at TIMESTAMPTZ NOT NULL DEFAULT now()\
             )",
        )
        .await?;
        db.execute_unprepared(
            "CREATE UNIQUE INDEX IF NOT EXISTS notification_node_settings_app_uidx \
               ON notification_node_settings (user_id, app_id, node_type, node) \
               WHERE app_id IS NOT NULL",
        )
        .await?;
        db.execute_unprepared(
            "CREATE UNIQUE INDEX IF NOT EXISTS notification_node_settings_oxygen_uidx \
               ON notification_node_settings (user_id, node_type, node) \
               WHERE app_id IS NULL",
        )
        .await?;
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS notification_user_settings (\
               user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE, \
               version BIGINT NOT NULL DEFAULT 0, \
               paused_until TIMESTAMPTZ, \
               timezone TEXT, \
               quiet_start SMALLINT CHECK (quiet_start BETWEEN 0 AND 1439), \
               quiet_end SMALLINT CHECK (quiet_end BETWEEN 0 AND 1439), \
               quiet_days SMALLINT CHECK (quiet_days BETWEEN 0 AND 127), \
               quiet_allow TEXT CHECK (quiet_allow IN ('critical','time_sensitive')), \
               quiet_summary_through TIMESTAMPTZ, \
               updated_at TIMESTAMPTZ NOT NULL DEFAULT now(), \
               CHECK ((quiet_start IS NULL) = (quiet_end IS NULL) \
                  AND (quiet_start IS NULL) = (quiet_days IS NULL) \
                  AND (quiet_start IS NULL) = (quiet_allow IS NULL))\
             )",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE device_tokens \
               ADD COLUMN IF NOT EXISTS enabled BOOLEAN NOT NULL DEFAULT true, \
               ADD COLUMN IF NOT EXISTS min_level TEXT, \
               ADD COLUMN IF NOT EXISTS os_permission TEXT, \
               ADD COLUMN IF NOT EXISTS os_disabled JSONB, \
               ADD COLUMN IF NOT EXISTS app_version TEXT",
        )
        .await?;
        db.execute_unprepared(BACKFILL_SQL).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE device_tokens \
               DROP COLUMN IF EXISTS enabled, \
               DROP COLUMN IF EXISTS min_level, \
               DROP COLUMN IF EXISTS os_permission, \
               DROP COLUMN IF EXISTS os_disabled, \
               DROP COLUMN IF EXISTS app_version",
        )
        .await?;
        db.execute_unprepared("DROP TABLE IF EXISTS notification_user_settings")
            .await?;
        db.execute_unprepared("DROP TABLE IF EXISTS notification_node_settings")
            .await?;
        Ok(())
    }
}
