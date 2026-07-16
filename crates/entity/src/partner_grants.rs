use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// A partner is not an entity — it is a **grant an organization holds**.
///
/// The org whose id is here IS the partner: its name, slug, people, workspaces and
/// bill are the org's. It simply also has permission to administer other orgs. That
/// is why a consultancy can use Oxy for its own business *and* manage clients
/// without maintaining two disconnected identities.
///
/// See `internal-docs/2026-07-16-partner-platform-design.md`.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "partner_grants")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub org_id: Uuid,
    /// "active" | "suspended".
    pub status: String,
    pub created_by: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
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
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organizations.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
