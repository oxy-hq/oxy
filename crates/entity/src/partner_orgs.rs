use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Which client orgs a partner manages.
///
/// `managed_org_id` is UNIQUE: a client is never managed by two partners. Detaching
/// is Oxy-only — a partner cannot unilaterally orphan a customer.
#[sea_orm::model]
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
    #[sea_orm(
        belongs_to,
        from = "partner_org_id",
        to = "org_id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub partner_grants: BelongsTo<super::partner_grants::Entity>,
    #[sea_orm(
        belongs_to,
        from = "managed_org_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub organizations: BelongsTo<super::organizations::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
