use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "slack_threads")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub installation_id: Uuid,
    pub slack_channel_id: String,
    pub slack_thread_ts: String,
    pub workspace_id: Uuid,
    pub agent_path: String,
    pub oxy_thread_id: Uuid,
    pub initiated_by_user_link_id: Option<Uuid>,
    pub last_slack_message_ts: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
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
        belongs_to = "super::workspaces::Entity",
        from = "Column::WorkspaceId",
        to = "super::workspaces::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Workspaces,
    #[sea_orm(
        belongs_to = "super::threads::Entity",
        from = "Column::OxyThreadId",
        to = "super::threads::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Threads,
    #[sea_orm(
        belongs_to = "super::slack_user_links::Entity",
        from = "Column::InitiatedByUserLinkId",
        to = "super::slack_user_links::Column::Id",
        on_update = "NoAction",
        on_delete = "SetNull"
    )]
    SlackUserLinks,
}

impl Related<super::slack_installations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SlackInstallations.def()
    }
}

impl Related<super::workspaces::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Workspaces.def()
    }
}

impl Related<super::threads::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Threads.def()
    }
}

impl Related<super::slack_user_links::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SlackUserLinks.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
