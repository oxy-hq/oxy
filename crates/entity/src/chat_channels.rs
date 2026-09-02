//! A chat channel — the tenancy unit for conversation.
//!
//! Scoped to exactly one org, and there is deliberately no cross-org channel:
//! the org is the tenancy boundary, and a conversation spanning two of them has
//! no owner who could answer a deletion request.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "chat_channels")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(indexed)]
    pub org_id: Uuid,
    /// `channel` (named, joinable) or `dm` (a fixed set of members).
    pub kind: String,
    /// NULL for a DM — its title is derived from its members, and a stored one
    /// goes stale the moment somebody leaves. The schema enforces the pairing.
    pub name: Option<String>,
    pub topic: Option<String>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
    /// Archived channels stay readable and stop accepting messages. Deleting
    /// one would take its history with it, which is rarely what "archive" means
    /// to the person clicking it.
    pub archived_at: Option<DateTimeWithTimeZone>,
    #[sea_orm(has_many)]
    pub chat_channel_members: HasMany<super::chat_channel_members::Entity>,
    #[sea_orm(has_many)]
    pub chat_messages: HasMany<super::chat_messages::Entity>,
}

impl Model {
    /// A channel accepts messages unless it has been archived.
    pub fn is_writable(&self) -> bool {
        self.archived_at.is_none()
    }
}

impl ActiveModelBehavior for ActiveModel {}
