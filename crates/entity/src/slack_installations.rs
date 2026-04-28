use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "slack_installations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub org_id: Uuid,
    pub slack_team_id: String,
    pub slack_team_name: String,
    pub slack_enterprise_id: Option<String>,
    pub bot_user_id: String,
    pub bot_token_secret_id: Uuid,
    #[sea_orm(column_type = "Text")]
    pub bot_scopes: String,
    pub installed_by_user_id: Uuid,
    pub installed_by_slack_user_id: String,
    pub installed_at: DateTimeWithTimeZone,
    pub revoked_at: Option<DateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::organizations::Entity",
        from = "Column::OrgId",
        to = "super::organizations::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Organizations,
    #[sea_orm(
        belongs_to = "super::org_secrets::Entity",
        from = "Column::BotTokenSecretId",
        to = "super::org_secrets::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    OrgSecrets,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::InstalledByUserId",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    Users,
    #[sea_orm(has_many = "super::slack_user_links::Entity")]
    SlackUserLinks,
    #[sea_orm(has_many = "super::slack_threads::Entity")]
    SlackThreads,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organizations.def()
    }
}

impl Related<super::org_secrets::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::OrgSecrets.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

impl Related<super::slack_user_links::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SlackUserLinks.def()
    }
}

impl Related<super::slack_threads::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SlackThreads.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
