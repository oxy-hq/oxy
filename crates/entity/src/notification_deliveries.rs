use sea_orm::entity::prelude::*;

/// One delivery attempt of one notification over one out-of-band channel.
///
/// Inserted as `claimed` before anything is sent, so a re-run of the delivery
/// task sends nothing twice: at most once per (notification, channel). The
/// inbox row itself is not a delivery — it is the record.
///
/// `reason` is a fixed code (`muted`, `no_devices`, `no_address`,
/// `not_linked`, `not_configured`, `provider_error`), never a provider's error
/// text, which can carry an address.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "notification_deliveries")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub notification_id: Uuid,
    /// `push` | `email` | `slack`.
    #[sea_orm(primary_key, auto_increment = false)]
    pub channel: String,
    /// `claimed` | `sent` | `skipped` | `failed`.
    pub status: String,
    pub reason: Option<String>,
    pub attempted_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
