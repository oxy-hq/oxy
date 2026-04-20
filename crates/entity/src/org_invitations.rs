use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use super::org_members::OrgRole;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum InviteStatus {
    #[sea_orm(string_value = "pending")]
    Pending,
    #[sea_orm(string_value = "accepted")]
    Accepted,
    #[sea_orm(string_value = "expired")]
    Expired,
}

impl InviteStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            InviteStatus::Pending => "pending",
            InviteStatus::Accepted => "accepted",
            InviteStatus::Expired => "expired",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "pending" => Ok(InviteStatus::Pending),
            "accepted" => Ok(InviteStatus::Accepted),
            "expired" => Ok(InviteStatus::Expired),
            _ => Err(format!("Invalid invite status: {s}")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "org_invitations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub org_id: Uuid,
    pub email: String,
    pub role: OrgRole,
    pub invited_by: Uuid,
    #[sea_orm(unique)]
    pub token: String,
    pub status: InviteStatus,
    pub expires_at: DateTimeWithTimeZone,
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
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::InvitedBy",
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

#[cfg(test)]
#[path = "org_invitations_tests.rs"]
mod org_invitations_tests;
