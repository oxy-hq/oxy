//! The notification inbox — the durable half of "we told them".
//!
//! Deliberately separate from push delivery. A row here is the record a person
//! opens and reads; it works with no vendor, no credentials and no network. A
//! push is a best-effort attempt to draw attention to one.
//!
//! Conflating them is the classic mistake: a notification that exists only as a
//! push is gone the moment the send fails, the device is offline, or the user
//! reinstalls — and "we told them" becomes unfalsifiable at exactly the moment
//! somebody is asking whether the store was warned.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "notifications")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub org_id: Uuid,
    #[sea_orm(indexed)]
    pub user_id: Uuid,
    /// `work_assigned` | `work_overdue` | `announcement` | `chat_mention`.
    ///
    /// A string rather than an enum so adding a kind is a deploy rather than a
    /// migration plus a coordinated rollout across every reader.
    pub kind: String,
    pub title: String,
    pub body: Option<String>,
    /// Where tapping it should land — a RELATIVE path, so it works on an org
    /// subdomain, a custom-app subdomain and the admin host without the sender
    /// having to know which one the reader is on.
    pub link: Option<String>,
    pub subject_kind: Option<String>,
    pub subject_id: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub read_at: Option<DateTimeWithTimeZone>,
}

impl Model {
    pub fn is_unread(&self) -> bool {
        self.read_at.is_none()
    }
}

impl ActiveModelBehavior for ActiveModel {}
