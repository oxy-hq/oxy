use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "slack_user_links")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub installation_id: Uuid,
    pub slack_user_id: String,
    pub oxy_user_id: Uuid,
    pub link_method: String,
    pub linked_at: DateTimeWithTimeZone,
    pub last_seen_at: DateTimeWithTimeZone,
    #[sea_orm(
        belongs_to,
        from = "installation_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub slack_installations: BelongsTo<super::slack_installations::Entity>,
    #[sea_orm(
        belongs_to,
        from = "oxy_user_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub users: BelongsTo<super::users::Entity>,
    #[sea_orm(has_many)]
    #[serde(skip)]
    pub slack_user_preferences: HasMany<super::slack_user_preferences::Entity>,
    #[sea_orm(has_many)]
    #[serde(skip)]
    pub slack_threads: HasMany<super::slack_threads::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
