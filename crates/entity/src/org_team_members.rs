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

#[sea_orm::model]
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
    #[sea_orm(
        belongs_to,
        from = "team_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub org_teams: BelongsTo<super::org_teams::Entity>,
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

impl ActiveModelBehavior for ActiveModel {}
