//! One message.
//!
//! Deleted by tombstone rather than by `DELETE`: a hard delete leaves a hole in
//! a conversation somebody is reading, while a tombstone renders as "message
//! deleted" and keeps the thread legible.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "chat_messages")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(indexed)]
    pub channel_id: Uuid,
    /// NULL when the author's user row is gone. The message stays: a thread
    /// that silently loses turns when somebody leaves the company is unreadable
    /// exactly when it is being audited.
    pub author_id: Option<Uuid>,
    pub body: String,
    pub created_at: DateTimeWithTimeZone,
    pub edited_at: Option<DateTimeWithTimeZone>,
    pub deleted_at: Option<DateTimeWithTimeZone>,
    #[sea_orm(
        belongs_to,
        from = "channel_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub chat_channels: BelongsTo<super::chat_channels::Entity>,
}

impl Model {
    /// What a client should render. A tombstone keeps its slot in the thread
    /// but must never leak the text it replaced.
    pub fn visible_body(&self) -> &str {
        if self.deleted_at.is_some() {
            "message deleted"
        } else {
            &self.body
        }
    }
}

impl ActiveModelBehavior for ActiveModel {}
