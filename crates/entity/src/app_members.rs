//! Per-app membership — who may open a `visibility = 'members'` app, and who
//! administers an app.
//!
//! This is deliberately NOT the same axis as `org_members`: an **app admin**
//! administers one app without holding org-Admin (which also carries billing and
//! member management). It is also not `workspace_members`, whose override is
//! elevate-only — app visibility has to be able to *subtract* access.
//!
//! These rows are FACTS. The rules that read them live in `oxy-authz`
//! (`Ring::AppAccess` for visibility, `Ring::AppAdmin` for the privileged
//! surface), loaded by `server::authz::loader`.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "app_members")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(indexed)]
    pub app_id: Uuid,
    #[sea_orm(indexed)]
    pub user_id: Uuid,
    /// `admin` or `member`. `admin` additionally grants the app's privileged
    /// surface (`Action::AppAdmin`); both grant access to a restricted app.
    #[sea_orm(default_value = "member")]
    pub role: String,
    pub created_at: DateTimeWithTimeZone,
    /// Who granted the membership. NULL when the granter's user row is gone
    /// (`ON DELETE SET NULL`) or for a seeded/system grant.
    pub created_by: Option<Uuid>,
    #[sea_orm(
        belongs_to,
        from = "app_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub apps: BelongsTo<super::apps::Entity>,
    #[sea_orm(
        belongs_to,
        from = "user_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub users: BelongsTo<super::users::Entity>,
}

/// The role an `app_members` row carries. Kept as a plain string in the DB (with
/// a CHECK) so a new tier doesn't need a PG type migration.
pub const ROLE_ADMIN: &str = "admin";
pub const ROLE_MEMBER: &str = "member";

impl Model {
    /// True when this membership administers the app.
    pub fn is_admin(&self) -> bool {
        self.role == ROLE_ADMIN
    }
}

impl ActiveModelBehavior for ActiveModel {}
