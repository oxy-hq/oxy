use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
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
        from = "workspace_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub workspaces: BelongsTo<super::workspaces::Entity>,
    #[sea_orm(
        belongs_to,
        from = "oxy_thread_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub threads: BelongsTo<super::threads::Entity>,
    #[sea_orm(
        belongs_to,
        from = "initiated_by_user_link_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "SetNull"
    )]
    #[serde(skip)]
    pub slack_user_links: BelongsTo<Option<super::slack_user_links::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
