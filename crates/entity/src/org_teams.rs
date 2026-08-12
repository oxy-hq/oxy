//! A named audience inside one org — "Finance", "Store Managers".
//!
//! Teams exist so an org admin grants an app to a *group the org already
//! recognises* rather than re-picking the same eight people for every app, and so
//! a new hire joins one team instead of being added to six apps.
//!
//! **Org-scoped, not workspace-scoped.** An org with several workspaces should name
//! "Finance" once; the officer doing the granting thinks in org terms, not project
//! terms. Membership is restricted to org members (enforced at write time, and
//! independently by `Ring::AppAccess`'s org-membership term) — a team is never a
//! back door for an outsider.
//!
//! Today the only consumer is custom-app access ([`super::app_team_grants`]).
//! Nothing here is app-specific, so a later consumer (workspaces, monitors) can
//! read the same rows — but until one exists, this is deliberately not a general
//! authorization principal in `oxy-authz`.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "org_teams")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(indexed)]
    pub org_id: Uuid,
    /// Display name. Unique per org **case-insensitively** — "Finance" and
    /// "finance" are the same team to a human, so the DB has a `lower(name)`
    /// unique index rather than a plain one.
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    /// Who created the team. NULL when their user row is gone
    /// (`ON DELETE SET NULL`) or for a seeded/system team.
    pub created_by: Option<Uuid>,
    #[sea_orm(
        belongs_to,
        from = "org_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub organizations: BelongsTo<super::organizations::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
