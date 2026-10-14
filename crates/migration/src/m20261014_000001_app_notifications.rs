use sea_orm_migration::prelude::*;

/// Custom apps can notify people (`ctx.notify`), and the inbox's content leaves
/// Oxy's database.
///
/// # Why
///
/// `ctx.notify` addresses a person, not an address: the platform resolves who,
/// writes the inbox row, and queues push, email and Slack per recipient. Every
/// table below holds **ids, kinds, hashes and reason codes** — the part Oxy
/// routes on. What the notification *says* (title, body, link, email body) is the
/// org's content and goes to the org's OLTP store (`oxy_inbox.messages`, platform
/// step 4 in `oxy_oltp::platform`), per `internal-docs/data-placement.md`.
///
/// - `notifications` becomes the per-recipient signal. `message_id` names the
///   content row in the org's store; `app_id` the custom app that sent it, when
///   one did — loose, with no foreign key, because adding a constraint to a
///   table that already exists is a contraction the previous deploy would have
///   to live with. `title` keeps its `NOT NULL` and is written `''`, `body` and
///   `link` are written `NULL`: no content, and still a row the previous deploy's
///   entity (`title: String`) can decode, so its inbox does not 500 mid-rollout
///   or after a re-pin. The three drop together in a later deploy — the
///   *contract* half (`migration_rollback_safety.rs`).
/// - `notification_sends` — one row per `ctx.notify` call. Its id IS the content
///   row's id. The app's idempotency key is kept only as a SHA-256, so a key that
///   embeds a customer's text does not land here, and the partial unique index is
///   what makes a retried call a no-op.
/// - `notification_deliveries` — one claim per (recipient signal, channel). A
///   delivery task inserts the claim before it sends, so a re-run sends nothing
///   twice; `reason` is a fixed code (`muted`, `no_devices`, …), never a
///   provider's error text, which can carry an address.
/// - `notification_mutes` — what a person turned off, per app, kind and channel.
///   The inbox itself cannot be muted: it is the record that they were told.
/// - `app_email_sends` — the claim that makes `ctx.email.send`'s `idempotencyKey`
///   do what its docs always said it would.
///
/// # Locking
///
/// `ALTER TABLE notifications` takes an ACCESS EXCLUSIVE lock for a catalog-only
/// change (adding nullable columns); the table had no writer before this
/// release. Everything else is new.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE notifications \
               ADD COLUMN IF NOT EXISTS message_id UUID, \
               ADD COLUMN IF NOT EXISTS app_id UUID",
        )
        .await?;
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS notification_sends (\
               id UUID PRIMARY KEY, \
               org_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE, \
               app_id UUID REFERENCES apps(id) ON DELETE CASCADE, \
               environment TEXT NOT NULL, \
               kind TEXT NOT NULL, \
               critical BOOLEAN NOT NULL DEFAULT false, \
               email BOOLEAN NOT NULL DEFAULT false, \
               slack BOOLEAN NOT NULL DEFAULT false, \
               idempotency_key_sha256 TEXT, \
               recipients INTEGER NOT NULL DEFAULT 0, \
               run_id TEXT, \
               created_at TIMESTAMPTZ NOT NULL DEFAULT now()\
             )",
        )
        .await?;
        // The dedupe: a second call with the same key in the same app and
        // environment conflicts here and answers with the first call's id.
        db.execute_unprepared(
            "CREATE UNIQUE INDEX IF NOT EXISTS notification_sends_idempotency \
             ON notification_sends (app_id, environment, idempotency_key_sha256) \
             WHERE idempotency_key_sha256 IS NOT NULL",
        )
        .await?;
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS notification_deliveries (\
               notification_id UUID NOT NULL REFERENCES notifications(id) ON DELETE CASCADE, \
               channel TEXT NOT NULL CHECK (channel IN ('push','email','slack')), \
               status TEXT NOT NULL CHECK (status IN ('claimed','sent','skipped','failed')), \
               reason TEXT, \
               attempted_at TIMESTAMPTZ NOT NULL DEFAULT now(), \
               PRIMARY KEY (notification_id, channel)\
             )",
        )
        .await?;
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS notification_mutes (\
               user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE, \
               app_id UUID NOT NULL REFERENCES apps(id) ON DELETE CASCADE, \
               kind TEXT NOT NULL, \
               channel TEXT NOT NULL CHECK (channel IN ('push','email','slack')), \
               created_at TIMESTAMPTZ NOT NULL DEFAULT now(), \
               PRIMARY KEY (user_id, app_id, kind, channel)\
             )",
        )
        .await?;
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS app_email_sends (\
               app_id UUID NOT NULL REFERENCES apps(id) ON DELETE CASCADE, \
               environment TEXT NOT NULL, \
               idempotency_key_sha256 TEXT NOT NULL, \
               message_id TEXT, \
               created_at TIMESTAMPTZ NOT NULL DEFAULT now(), \
               PRIMARY KEY (app_id, environment, idempotency_key_sha256)\
             )",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "DROP TABLE IF EXISTS app_email_sends; \
             DROP TABLE IF EXISTS notification_mutes; \
             DROP TABLE IF EXISTS notification_deliveries; \
             DROP TABLE IF EXISTS notification_sends; \
             ALTER TABLE notifications DROP COLUMN IF EXISTS app_id, \
               DROP COLUMN IF EXISTS message_id",
        )
        .await?;
        Ok(())
    }
}
