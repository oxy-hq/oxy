use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// An explicit, time-bounded staff impersonation ("assume role") session.
///
/// The platform has always let Oxy staff act as a tenant Owner — `org_context` /
/// `workspace_context` synthesize an Owner membership for a Global Owner /
/// Global Admin who is not a real member. This row is what makes that reach
/// **opt-in, scoped, bounded, and auditable**: no live row for `(actor, org)`,
/// no synthetic membership.
///
/// `actor_user_id` / `actor_email` are always the REAL staff user — never the
/// impersonated identity, so the audit trail names who actually acted.
///
/// A session is live when `ended_at IS NULL AND expires_at > now()`.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "admin_assume_sessions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub actor_user_id: Uuid,
    pub actor_email: String,
    /// Scope: exactly one org.
    pub org_id: Uuid,
    /// Why — required. An unexplained impersonation is a red flag.
    pub reason: String,
    pub started_at: DateTimeWithTimeZone,
    /// Hard bound; an expired row grants nothing.
    pub expires_at: DateTimeWithTimeZone,
    /// Set when explicitly ended; NULL while live.
    pub ended_at: Option<DateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::organizations::Entity",
        from = "Column::OrgId",
        to = "super::organizations::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Organizations,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::ActorUserId",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Users,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organizations.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
