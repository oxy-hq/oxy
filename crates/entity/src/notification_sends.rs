use sea_orm::entity::prelude::*;

/// One `ctx.notify` call (or one platform notice): who sent it, of what kind,
/// to how many people, and through which out-of-band channels.
///
/// The id is also the id of the content row in the org's OLTP store
/// (`oxy_inbox.messages`), so a send and what it said are one key apart and
/// never copied. The app's idempotency key is stored only as a SHA-256.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "notification_sends")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub org_id: Uuid,
    pub app_id: Option<Uuid>,
    /// `production`, `staging`, or a sandbox's name — the environment the
    /// sending function ran in. Part of the idempotency scope.
    pub environment: String,
    pub kind: String,
    /// Declared critical in the manifest: mutes do not apply.
    pub critical: bool,
    /// Whether email was asked for. Push is always attempted.
    pub email: bool,
    /// Whether a Slack DM was asked for.
    pub slack: bool,
    pub idempotency_key_sha256: Option<String>,
    pub recipients: i32,
    /// The delivery task's run, once queued.
    pub run_id: Option<String>,
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
