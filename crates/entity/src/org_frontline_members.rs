//! A frontline worker's standing in an org — deliberately **not** an
//! `org_members` row.
//!
//! An org Member today reaches Airhouse settings, and through
//! `EffectiveWorkspaceRole` reaches workspace surfaces including Databases and
//! Secrets. Enrolling a restaurant's hourly staff there would hand 127 people
//! the tenant's credential surface. That is privilege escalation by
//! construction, not a policy that could be tightened later — so frontline
//! standing gets its own, narrower binding.
//!
//! What it grants: in `oxy-authz` this feeds `PrincipalFacts::frontline_orgs`,
//! read by exactly one ring (`AppAccess`) and only when ANDed with an
//! `app_members` grant. A frontline worker reaches the apps they were explicitly
//! given and nothing else — no org read, no workspace, no settings.
//!
//! **No `role` column, on purpose.** Role vocabulary is the one part of frontline
//! identity that genuinely needs the customer (Poke House runs eight, split
//! across "works at a location" and "works at head office"). An empty enum now
//! beats a wrong one shipped.
//!
//! Design record: `internal-docs/frontline-identity.md`.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// The two values `status` takes. One spelling, read by the loader, the
/// gates, the directory, the roster and the suspension route — a literal that
/// drifted in any of them would silently open or close a door.
pub const STATUS_ACTIVE: &str = "active";
pub const STATUS_SUSPENDED: &str = "suspended";

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "org_frontline_members")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub org_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false, indexed)]
    pub user_id: Uuid,
    /// `active` | `suspended`. Suspension is how a tenant switches off a
    /// departing worker's logins without deleting the rows their submissions
    /// are attributed to.
    pub status: String,
    pub created_at: DateTimeWithTimeZone,
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
