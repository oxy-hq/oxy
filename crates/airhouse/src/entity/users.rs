use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum AirhouseUserRole {
    #[sea_orm(string_value = "reader")]
    Reader,
    #[sea_orm(string_value = "writer")]
    Writer,
    #[sea_orm(string_value = "admin")]
    Admin,
}

impl AirhouseUserRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            AirhouseUserRole::Reader => "reader",
            AirhouseUserRole::Writer => "writer",
            AirhouseUserRole::Admin => "admin",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "snake_case")]
pub enum AirhouseUserStatus {
    #[sea_orm(string_value = "active")]
    Active,
    #[sea_orm(string_value = "failed")]
    Failed,
    #[sea_orm(string_value = "pending_delete")]
    PendingDelete,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "airhouse_users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub tenant_row_id: Uuid,
    pub workspace_id: Uuid,
    pub oxy_user_id: Uuid,
    pub username: String,
    pub role: AirhouseUserRole,
    /// id of the row in `org_secrets` storing the plaintext password. NULL once
    /// the password has been revealed (single-show flow) or if provisioning
    /// failed before persisting.
    pub password_secret_id: Option<Uuid>,
    pub password_revealed_at: Option<DateTimeWithTimeZone>,
    pub status: AirhouseUserStatus,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::tenants::Entity",
        from = "Column::TenantRowId",
        to = "super::tenants::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    AirhouseTenants,
}

impl Related<super::tenants::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::AirhouseTenants.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
