use crate::errors::OxyError;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::Serialize;
use uuid::Uuid;

#[derive(Serialize)]
pub struct MessageItem {
    pub content: String,
    pub id: Uuid,
    pub is_human: bool,
    pub created_at: DateTimeWithTimeZone,
}
