//! Who is in an [`super::org_teams`] team.
//!
//! A plain join row — the team carries no role of its own. What a team *buys* is
//! decided per-app by [`super::app_team_grants`], so the same "Finance" team can be
//! plain members of one app and admins of another.
//!
//! Rows are FACTS. `oxy_server_authz::loader` walks
//! `user → org_team_members → app_team_grants` and unions the result into the SAME
//! `PrincipalFacts` vectors that direct `app_members` rows populate — which is why
//! no `oxy-authz` ring mentions teams.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "org_team_members")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(indexed)]
    pub team_id: Uuid,
    #[sea_orm(indexed)]
    pub user_id: Uuid,
    pub created_at: DateTimeWithTimeZone,
    /// Who added them. NULL when the granter's user row is gone
    /// (`ON DELETE SET NULL`).
    pub created_by: Option<Uuid>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::org_teams::Entity",
        from = "Column::TeamId",
        to = "super::org_teams::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    OrgTeams,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Users,
}

impl Related<super::org_teams::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::OrgTeams.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
