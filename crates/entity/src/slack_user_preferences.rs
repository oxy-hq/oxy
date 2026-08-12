use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
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
    #[sea_orm(
        belongs_to,
        from = "user_link_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub slack_user_links: BelongsTo<super::slack_user_links::Entity>,
    #[sea_orm(
        belongs_to,
        from = "default_workspace_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "SetNull"
    )]
    #[serde(skip)]
    pub workspaces: BelongsTo<Option<super::workspaces::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
