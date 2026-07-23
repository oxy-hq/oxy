use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// One row per HTML serve of a custom-app bundle — the moment a user
/// "opened the app" in their browser. Recorded server-side from
/// `custom_apps_serve::serve_dispatch` via `tokio::spawn` so the
/// write is fire-and-forget on the response path.
///
/// The Activity tab in AppDetail queries this table for "who opened
/// this app and when?" — see also `super::custom_app_event` for the
/// engineer-tagged custom events fired from inside the bundle.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "custom_app_view_event")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub app_id: Uuid,
    pub user_id: Uuid,
    /// Denormalized for the "recent visitors" table so we don't have
    /// to join `users` on every row. Kept in sync at insert time only;
    /// stale on email change (acceptable — operator can re-derive
    /// from `user_id`).
    pub user_email: String,
    /// Sticky 30-min cookie value. Multiple views with the same
    /// `session_id` collapse into one session for the active-users
    /// count.
    pub session_id: Uuid,
    pub viewed_at: DateTimeWithTimeZone,
    /// HTTP `Referer` header when present and same-host (third-party
    /// referrers are dropped at insert time so we don't leak external
    /// URLs into the DB). The HTTP header itself is canonically spelled
    /// `Referer` (one r — a famous early-spec typo); the plural in
    /// prose uses the dictionary spelling so the typos linter stays
    /// happy.
    pub referrer: Option<String>,
    /// `"browser"` (default — has User-Agent + Sec-Fetch-Site headers
    /// consistent with a real navigation) / `"sdk"` (the bundle SDK
    /// or curl-style probe) / `"unknown"`.
    pub user_agent_class: String,
    /// `"subpath"` (served from `/customer-apps/<org>/<slug>/`) or
    /// `"subdomain"` (served from
    /// `<org>--<slug>.customer-apps[-env].oxygen-hq.com`). Lets the
    /// admin see which surface their users are reaching.
    pub source: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::apps::Entity",
        from = "Column::AppId",
        to = "super::apps::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Apps,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Users,
}

impl Related<super::apps::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Apps.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
