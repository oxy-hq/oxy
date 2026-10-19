use sea_orm_migration::prelude::*;

/// `device_tokens.web_push_p256dh` and `.web_push_auth`: a browser
/// subscription's encryption keys, so a Web Push can carry a payload.
///
/// RFC 8291 encrypts a push message to the subscription: the browser's P-256
/// public key (`p256dh`) and a 16-byte shared secret (`auth`), both handed out
/// by `PushSubscription.toJSON()` beside the endpoint. Until now only the
/// endpoint was stored, which is all a bodiless wake needs. With the keys, the
/// push names the app and the kind's label (`web_push_payload.rs`) — still ids
/// and config only, now end-to-end encrypted to the device.
///
/// Both nullable: an `apns`/`fcm` row has neither, and a `web` row registered
/// before this keeps getting the bodiless push until its browser registers
/// again. Not a secret in the credential sense — the browser generated them
/// for exactly this server to encrypt to — but they are per-device, so they
/// live on the device's row, not in any log.
///
/// **Rollback-safe**: two nullable columns, no constraint. The previous deploy
/// never reads them, and a row it writes has both `NULL`.
///
/// # Locking
///
/// `ALTER TABLE device_tokens` takes an ACCESS EXCLUSIVE lock for a
/// catalog-only change (two nullable columns, no default, no rewrite).
/// Registered after `m20261017_000001_notification_settings`, which alters
/// the same table.
#[derive(DeriveMigrationName)]
pub struct Migration;

const UP_SQL: &str = "ALTER TABLE device_tokens \
       ADD COLUMN IF NOT EXISTS web_push_p256dh TEXT, \
       ADD COLUMN IF NOT EXISTS web_push_auth TEXT";

const DOWN_SQL: &str = "ALTER TABLE device_tokens \
       DROP COLUMN IF EXISTS web_push_p256dh, \
       DROP COLUMN IF EXISTS web_push_auth";

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
