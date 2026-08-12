use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum OrgRole {
    #[sea_orm(string_value = "owner")]
    Owner,
    #[sea_orm(string_value = "admin")]
    Admin,
    #[sea_orm(string_value = "member")]
    Member,
}

impl OrgRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrgRole::Owner => "owner",
            OrgRole::Admin => "admin",
            OrgRole::Member => "member",
        }
    }

    pub fn level(&self) -> u8 {
        match self {
            OrgRole::Owner => 2,
            OrgRole::Admin => 1,
            OrgRole::Member => 0,
        }
    }
}

impl PartialOrd for OrgRole {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrgRole {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.level().cmp(&other.level())
    }
}

impl FromStr for OrgRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "owner" => Ok(Self::Owner),
            "admin" => Ok(Self::Admin),
            "member" => Ok(Self::Member),
            _ => Err(format!("Invalid org role: {s}")),
        }
    }
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "org_members")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub org_id: Uuid,
    pub user_id: Uuid,
    pub role: OrgRole,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    #[sea_orm(
        belongs_to,
        from = "org_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub organizations: BelongsTo<super::organizations::Entity>,
    #[sea_orm(
        belongs_to,
        from = "user_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub users: BelongsTo<super::users::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}

#[cfg(test)]
#[path = "org_members_tests.rs"]
mod org_members_tests;
