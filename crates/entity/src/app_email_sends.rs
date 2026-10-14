use sea_orm::entity::prelude::*;

/// The claim behind `ctx.email.send`'s `idempotencyKey`.
///
/// Inserted before the message is handed to SES; `message_id` is stamped once
/// SES accepts it. A second send with the same key in the same app and
/// environment finds the row and sends nothing. A send that failed deletes its
/// claim, so the retry that follows can send.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "app_email_sends")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub app_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub environment: String,
    /// SHA-256 of the app's key, hex. The key is the app's text and may embed
    /// customer data; the hash is all a dedupe needs.
    #[sea_orm(primary_key, auto_increment = false)]
    pub idempotency_key_sha256: String,
    pub message_id: Option<String>,
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
