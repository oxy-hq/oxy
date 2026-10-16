use sea_orm::entity::prelude::*;

/// A channel a person turned off for everything above one kind: a whole
/// source, or a category subtree of it.
///
/// `app_id` is the custom app, or `NULL` for Oxygen's own notices. `scope` is
/// `''` for the whole source or a category path (`operations/food-safety`),
/// which covers its subtree. A notification is muted on a channel when its
/// kind or any ancestor is; a critical kind ignores every mute. Kind-level
/// mutes stay in `notification_mutes` and `notification_platform_mutes`.
///
/// Unique per (user, app, scope, channel) by two partial indexes, one for
/// `app_id IS NULL` (a primary key cannot hold a `NULL`), so `id` is a
/// surrogate.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "notification_scope_mutes")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub user_id: Uuid,
    pub app_id: Option<Uuid>,
    pub scope: String,
    /// `push` | `email` | `slack`.
    pub channel: String,
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
