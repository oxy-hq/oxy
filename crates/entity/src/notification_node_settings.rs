use sea_orm::entity::prelude::*;

/// What a person set at one node of a notification source's tree: the whole
/// source (`node_type = 'scope'`, `node = ''`), a category subtree
/// (`'scope'`, a path) or one kind (`'kind'`, its name). `app_id` is `NULL`
/// for Oxygen.
///
/// Every value is explicit or `NULL`, and `NULL` means "inherit": the nearest
/// node with a value wins, so a kind can be turned back on under a category
/// that is off. A row whose values are all `NULL` is deleted.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "notification_node_settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub user_id: Uuid,
    pub app_id: Option<Uuid>,
    /// `scope` | `kind`.
    pub node_type: String,
    pub node: String,
    /// `true` on, `false` off, `NULL` inherit.
    pub push: Option<bool>,
    pub email: Option<bool>,
    pub slack: Option<bool>,
    /// The floor: kinds below it go to the inbox only. `passive` | `active` |
    /// `time_sensitive` | `critical`, or `NULL` to inherit.
    pub min_level: Option<String>,
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
