use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "slack_user_preferences")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub user_link_id: Uuid,
    pub default_workspace_id: Option<Uuid>,
    pub default_agent_path: Option<String>,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::slack_user_links::Entity",
        from = "Column::UserLinkId",
        to = "super::slack_user_links::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    SlackUserLinks,
    #[sea_orm(
        belongs_to = "super::workspaces::Entity",
        from = "Column::DefaultWorkspaceId",
        to = "super::workspaces::Column::Id",
        on_update = "NoAction",
        on_delete = "SetNull"
    )]
    Workspaces,
}

impl Related<super::slack_user_links::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SlackUserLinks.def()
    }
}

impl Related<super::workspaces::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Workspaces.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
