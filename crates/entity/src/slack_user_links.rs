use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

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
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::slack_installations::Entity",
        from = "Column::InstallationId",
        to = "super::slack_installations::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    SlackInstallations,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::OxyUserId",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Users,
    #[sea_orm(has_many = "super::slack_user_preferences::Entity")]
    SlackUserPreferences,
    #[sea_orm(has_many = "super::slack_threads::Entity")]
    SlackThreads,
}

impl Related<super::slack_installations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SlackInstallations.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

impl Related<super::slack_user_preferences::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SlackUserPreferences.def()
    }
}

impl Related<super::slack_threads::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SlackThreads.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
