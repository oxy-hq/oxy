use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "stripe_webhook_events")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub stripe_event_id: String,
    pub event_type: String,
    pub payload: Json,
    pub processed_at: DateTimeWithTimeZone,
    pub status: String,
    pub attempts: i32,
    pub last_error: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

pub mod status {
    pub const PROCESSING: &str = "processing";
    pub const SUCCESS: &str = "success";
    pub const FAILED: &str = "failed";
}
