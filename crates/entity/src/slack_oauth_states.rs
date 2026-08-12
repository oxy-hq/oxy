use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
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
    #[sea_orm(
        belongs_to,
        from = "org_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub organizations: BelongsTo<Option<super::organizations::Entity>>,
    #[sea_orm(
        belongs_to,
        from = "oxy_user_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub users: BelongsTo<Option<super::users::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
