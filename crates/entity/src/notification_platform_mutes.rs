use sea_orm::entity::prelude::*;

/// A channel a person turned off for one kind of platform notice (`oxy.*`).
///
/// The platform's counterpart of `notification_mutes`, which is keyed by app
/// and so cannot name a notice no app sent. No row means on. A critical kind
/// (`oxy.billing_past_due`) cannot be muted at all.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "notification_platform_mutes")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub kind: String,
    /// `push` | `email` | `slack`.
    #[sea_orm(primary_key, auto_increment = false)]
    pub channel: String,
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
