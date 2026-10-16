//! The notification inbox — the durable half of "we told them".
//!
//! This row is the **signal**: who was told, by which org (and app), and
//! whether they have read it. What they were told lives in the org's own OLTP
//! store, keyed by `message_id` — see `internal-docs/custom-app-notifications.md`.
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
    /// Legacy content columns, written empty (`title`, still `NOT NULL` so the
    /// previous deploy can decode the row) and `NULL` since content moved to
    /// the org's OLTP store (`oxy_inbox.messages`, read by
    /// [`Self::message_id`]). They drop in the contract step that follows
    /// `m20261014_000001_app_notifications`; nothing reads them.
    pub title: String,
    pub body: Option<String>,
    pub link: Option<String>,
    /// What a platform notice (`oxy.*` kind) is about: `workspace`,
    /// `automation_run`, `airway_run`, `anomaly` or `org`, and that record's
    /// id. A platform notice stores no text — its title, body and link are
    /// rendered from the record when it is read or delivered — and it is sent
    /// at most once per (recipient, kind, subject). `NULL` on an app's
    /// notification. Live columns, unlike the three above.
    pub subject_kind: Option<String>,
    pub subject_id: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub read_at: Option<DateTimeWithTimeZone>,
    /// The content row in the org's OLTP store — what the notification says.
    /// Ids only here: the title and body are the org's data, not Oxy's.
    pub message_id: Option<Uuid>,
    /// The custom app that sent it, when one did. Loose (no foreign key), like
    /// `org_id` on most signal tables.
    pub app_id: Option<Uuid>,
    /// The thread the app put it in (`ctx.notify({ thread })`), as a hash —
    /// SHA-256 hex, first 32 characters — never the app's text. Clients
    /// collapse a thread; a phone groups by it. `NULL` when none was given.
    pub thread_key: Option<String>,
}

impl Model {
    pub fn is_unread(&self) -> bool {
        self.read_at.is_none()
    }
}

impl ActiveModelBehavior for ActiveModel {}
