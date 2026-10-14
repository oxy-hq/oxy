use sea_orm::entity::prelude::*;

/// A channel a person turned off for one kind of notification from one app.
///
/// No row means on. The inbox cannot be muted — it is the record that they
/// were told — and a kind the app declared `critical` ignores mutes.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "notification_mutes")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub app_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub kind: String,
    /// `push` | `email` | `slack`.
    #[sea_orm(primary_key, auto_increment = false)]
    pub channel: String,
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
