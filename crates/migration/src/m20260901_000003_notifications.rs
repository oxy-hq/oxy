use sea_orm_migration::prelude::*;

/// The notification inbox, and the device tokens delivery will need.
///
/// Two tables that are deliberately not one. The INBOX is the durable record a
/// person opens and reads — it is the product surface, and it works with no
/// vendor, no credentials and no network. PUSH is a delivery attempt against
/// that record: best-effort, external, and failable.
///
/// Conflating them is the classic mistake. A notification that exists only as a
/// push is gone the moment the send fails, the device is offline, or the user
/// reinstalls — and "we told them" becomes unfalsifiable at exactly the moment
/// somebody is asking whether the store was warned.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE notifications (
                id           UUID PRIMARY KEY,
                org_id       UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                -- Who it is for. Indexed with `read_at` because the only query
                -- that matters is "my unread", and it runs on every page load.
                user_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                -- What kind of thing happened: 'work_assigned', 'work_overdue',
                -- 'announcement', 'chat_mention'. A string rather than an enum
                -- so a new kind is a deploy rather than a migration plus a
                -- coordinated rollout across every reader.
                kind         TEXT NOT NULL,
                title        TEXT NOT NULL,
                body         TEXT,
                -- Where tapping it should land. A relative path, so it works on
                -- an org subdomain, a custom-app subdomain and the admin host
                -- without the sender having to know which one the reader is on.
                link         TEXT,
                -- What it is about, same polymorphic shape as `work_items`
                -- provenance and for the same reason: notifications are
                -- generated from five kinds of thing and will be from more.
                subject_kind TEXT,
                subject_id   TEXT,
                created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
                read_at      TIMESTAMPTZ
            );
            -- Partial on unread: the read tail is unbounded and nothing reads it
            -- in bulk, while "my unread" runs constantly.
            CREATE INDEX notifications_unread
                ON notifications (user_id, created_at DESC) WHERE read_at IS NULL;
             CREATE INDEX notifications_unread_by_org
                ON notifications (user_id, org_id, created_at DESC) WHERE read_at IS NULL;
            CREATE INDEX notifications_inbox
                ON notifications (user_id, created_at DESC);

            CREATE TABLE device_tokens (
                id          UUID PRIMARY KEY,
                user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                -- 'apns' | 'fcm' | 'web'. Web push is a real third platform
                -- rather than a variant of the others: different key format,
                -- different endpoint, and it is what an installed PWA uses.
                platform    TEXT NOT NULL CHECK (platform IN ('apns','fcm','web')),
                token       TEXT NOT NULL,
                -- So a person's other devices survive one being wiped.
                device_name TEXT,
                created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
                last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                -- A token is unique to a device install, not to a user: the
                -- same token can be handed to a second account after a device
                -- is passed on, and the OLD row must lose it rather than both
                -- keeping it and one person getting the other's notifications.
                UNIQUE (platform, token)
            );
            CREATE INDEX device_tokens_by_user ON device_tokens (user_id);
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            DROP TABLE IF EXISTS device_tokens CASCADE;
            DROP TABLE IF EXISTS notifications CASCADE;
        "#,
            )
            .await?;
        Ok(())
    }
}
