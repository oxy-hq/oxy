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
