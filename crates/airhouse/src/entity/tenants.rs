use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "snake_case")]
pub enum TenantStatus {
    #[sea_orm(string_value = "active")]
    Active,
    #[sea_orm(string_value = "failed")]
    Failed,
    #[sea_orm(string_value = "pending_delete")]
    PendingDelete,
}

impl TenantStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TenantStatus::Active => "active",
            TenantStatus::Failed => "failed",
            TenantStatus::PendingDelete => "pending_delete",
        }
    }
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "airhouse_tenants")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub workspace_id: Uuid,
    #[sea_orm(unique)]
    pub airhouse_tenant_id: String,
    pub bucket: String,
    pub prefix: Option<String>,
    pub status: TenantStatus,
    pub created_at: DateTimeWithTimeZone,

    /// Airhouse-side id of the service account bound to this tenant.
    /// `Some` once the tenant has been provisioned in SA mode; `None` for
    /// rows that predate the SA migration (provisioner backfills lazily).
    pub service_account_id: Option<String>,
    /// AES-GCM-sealed SA bearer (envelope-encrypted with the platform
    /// master key). The bearer is shown exactly once by the Admin API at
    /// SA-create time; losing this ciphertext means the SA must be
    /// revoked and re-minted, since Airhouse only persists the hash.
    pub bearer_ciphertext: Option<Vec<u8>>,
    /// Max role any token minted by this SA may carry. Always `"admin"`
    /// today; persisted for forward-compat if we ever scope per-tenant.
    pub bearer_max_role: Option<String>,
    /// Max TTL in seconds the SA may issue. Defaults to the airhouse
    /// system cap (86400 = 24h).
    pub bearer_max_ttl_secs: Option<i32>,
    pub sa_created_at: Option<DateTimeWithTimeZone>,
    /// Last time the SA was rotated. Populated by the bearer-leak
    /// response flow; `None` means the SA has never been rotated.
    pub sa_rotated_at: Option<DateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
