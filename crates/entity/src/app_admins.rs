use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// A **platform grant** — one person's Oxy-staff standing, as `(role × scope)`.
///
/// The table name predates the capability split, when every row meant the same thing
/// ("is Oxy staff") and the model read it as a boolean. It now carries the role the
/// grant was issued as, so an App Operator and a Global Admin are different rows rather
/// than indistinguishable ones. The name is kept as a storage contract; see
/// `internal-docs/roles-and-authorization.md`.
///
/// Managed by `OXY_OWNER` users only — the grant table gates itself
/// (`Ring::GlobalOwnerOnly`), so no grant can ever widen its own holder.
///
/// Email is stored (not user_id) so grants can be created before the
/// user has signed in for the first time, matching the magic-link
/// onboarding model.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "app_admins")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub email: String,
    /// User who added this admin. NULL when the row was seeded from the
    /// legacy `OXY_APP_ADMINS` env var on startup.
    pub granted_by: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
    /// The preset this grant was issued as — `oxy_authz::PlatformRole::as_str`.
    /// Backfilled to `global_admin` for every row that predates the split, which is
    /// what makes the migration behaviour-neutral. An id this build cannot expand is
    /// **dropped** by the loader, denying rather than guessing.
    pub role: String,
    /// `true` = reaches every org, present and future. `false` = reaches exactly the
    /// orgs in `app_admin_scope_orgs`, and reaches nothing if that set is empty.
    /// An explicit column, not "the child table is empty", so deleting the last scope
    /// row narrows a grant instead of silently promoting it to global.
    pub scope_all: bool,
    /// When this grant last changed. A grant is upserted in place, so `created_at`
    /// answers "when did they first get access" and this answers "when did it become
    /// what it is now" — the question the console exists to answer, and the one
    /// `granted_by` alone leaves half-told.
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::GrantedBy",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "SetNull"
    )]
    Users,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
