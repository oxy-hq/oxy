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
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::users::Entity")]
    AirhouseUsers,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::AirhouseUsers.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
