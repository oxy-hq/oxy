use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// A partner is not an entity — it is a **grant an organization holds**.
///
/// The org whose id is here IS the partner: its name, slug, people, workspaces and
/// bill are the org's. It simply also has permission to administer other orgs. That
/// is why a consultancy can use Oxy for its own business *and* manage clients
/// without maintaining two disconnected identities.
///
/// See `internal-docs/partner-platform.md`.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "partner_grants")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub org_id: Uuid,
    /// "active" | "suspended".
    pub status: String,
    pub created_by: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
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
