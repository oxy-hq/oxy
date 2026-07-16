use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Which client orgs a partner manages.
///
/// `managed_org_id` is UNIQUE: a client is never managed by two partners. Detaching
/// is Oxy-only — a partner cannot unilaterally orphan a customer.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "partner_orgs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// The partner (an org holding a `partner_grants` row).
    pub partner_org_id: Uuid,
    /// The client. UNIQUE across the table.
    #[sea_orm(unique)]
    pub managed_org_id: Uuid,
    pub created_by: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::partner_grants::Entity",
        from = "Column::PartnerOrgId",
        to = "super::partner_grants::Column::OrgId",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    PartnerGrants,
    #[sea_orm(
        belongs_to = "super::organizations::Entity",
        from = "Column::ManagedOrgId",
        to = "super::organizations::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Organizations,
}

impl Related<super::partner_grants::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::PartnerGrants.def()
    }
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organizations.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
