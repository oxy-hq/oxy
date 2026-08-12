use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "slack_seen_events")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub event_id: String,
    pub received_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
