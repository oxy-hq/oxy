//! Who is in a channel — and the read gate for every chat query.
//!
//! Channel access is enforced as a JOIN rather than as a fact in
//! `PrincipalFacts`: a user's channel set is unbounded, so loading it on every
//! request would put an unbounded read on the hot path to answer a question a
//! `WHERE` clause answers for free. `oxy-authz` still decides whether the
//! caller may reach the ORG's chat at all; this decides which channels within
//! it they see.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "chat_channel_members")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub channel_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false, indexed)]
    pub user_id: Uuid,
    pub joined_at: DateTimeWithTimeZone,
    /// Where this member has read up to. Unread is DERIVED from this rather
    /// than counted into a column — a stored counter has to be updated by every
    /// writer and drifts the first time one of them fails.
    pub last_read_at: Option<DateTimeWithTimeZone>,
    pub muted: bool,
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

impl ActiveModelBehavior for ActiveModel {}
