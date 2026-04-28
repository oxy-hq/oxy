use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "slack_oauth_states")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub kind: String,
    #[sea_orm(unique)]
    pub nonce: String,
    pub org_id: Option<Uuid>,
    pub slack_team_id: Option<String>,
    pub slack_user_id: Option<String>,
    /// Channel where the user originally sent the unlinked message.
    /// Used by the confirm handler to post a "✅ You're connected!" ephemeral.
    pub slack_channel_id: Option<String>,
    /// Thread timestamp to target for the post-connection confirmation.
    pub slack_thread_ts: Option<String>,
    pub oxy_user_id: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
    pub expires_at: DateTimeWithTimeZone,
    pub consumed_at: Option<DateTimeWithTimeZone>,
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
        belongs_to = "super::users::Entity",
        from = "Column::OxyUserId",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Users,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organizations.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
