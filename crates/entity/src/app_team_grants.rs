//! App access granted to an [`super::org_teams`] team — the twin of
//! [`super::app_members`], keyed by team instead of user.
//!
//! The two tables are deliberately the same shape (`role` included) because they
//! are one concept with two grantee kinds: the API renders them as a single tagged
//! union (`{kind: "user" | "team", role}`) and the fact loader unions them into the
//! same `PrincipalFacts` vectors. A `role = 'admin'` grant hands the app's
//! privileged surface (`ctx.user.appRole`) to *everyone* in the team — which is
//! useful for a "Finance Leads" team and a foot-gun for a 40-person one, so the UI
//! defaults new grants to `member`.
//!
//! Only meaningful when `apps.visibility = 'members'` (for *access*); a `role =
//! 'admin'` grant still reports through `ctx.user.appRole` on an unrestricted app,
//! matching how a direct `app_members` admin row behaves.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "app_team_grants")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(indexed)]
    pub app_id: Uuid,
    #[sea_orm(indexed)]
    pub team_id: Uuid,
    /// `admin` or `member` — the same vocabulary as `app_members::role`, reused
    /// rather than re-spelled so the two grant kinds can't drift.
    #[sea_orm(default_value = "member")]
    pub role: String,
    pub created_at: DateTimeWithTimeZone,
    /// Who granted it. NULL when the granter's user row is gone
    /// (`ON DELETE SET NULL`).
    pub created_by: Option<Uuid>,
}

impl Model {
    /// True when this grant confers the app's privileged surface on the team.
    pub fn is_admin(&self) -> bool {
        self.role == super::app_members::ROLE_ADMIN
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
        belongs_to = "super::org_teams::Entity",
        from = "Column::TeamId",
        to = "super::org_teams::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    OrgTeams,
}

impl Related<super::apps::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Apps.def()
    }
}

impl Related<super::org_teams::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::OrgTeams.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
