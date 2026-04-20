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
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::workspaces::Entity",
        from = "Column::WorkspaceId",
        to = "super::workspaces::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Workspaces,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Users,
}

impl Related<super::workspaces::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Workspaces.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

#[cfg(test)]
#[path = "workspace_members_tests.rs"]
mod workspace_members_tests;
