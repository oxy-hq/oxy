use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum WorkspaceRole {
    #[sea_orm(string_value = "owner")]
    Owner,
    #[sea_orm(string_value = "admin")]
    Admin,
    #[sea_orm(string_value = "member")]
    Member,
    #[sea_orm(string_value = "viewer")]
    Viewer,
}

impl WorkspaceRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkspaceRole::Owner => "owner",
            WorkspaceRole::Admin => "admin",
            WorkspaceRole::Member => "member",
            WorkspaceRole::Viewer => "viewer",
        }
    }

    pub fn level(&self) -> u8 {
        match self {
            WorkspaceRole::Owner => 3,
            WorkspaceRole::Admin => 2,
            WorkspaceRole::Member => 1,
            WorkspaceRole::Viewer => 0,
        }
    }
}

impl PartialOrd for WorkspaceRole {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for WorkspaceRole {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.level().cmp(&other.level())
    }
}

impl FromStr for WorkspaceRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "owner" => Ok(Self::Owner),
            "admin" => Ok(Self::Admin),
            "member" => Ok(Self::Member),
            "viewer" => Ok(Self::Viewer),
            _ => Err(format!("Invalid workspace role: {s}")),
        }
    }
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "workspace_members")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub role: WorkspaceRole,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    #[sea_orm(
        belongs_to,
        from = "workspace_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub workspaces: BelongsTo<super::workspaces::Entity>,
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
#[path = "workspace_members_tests.rs"]
mod workspace_members_tests;
