use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "organizations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub name: String,
    #[sea_orm(unique)]
    pub slug: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::org_members::Entity")]
    OrgMembers,
    #[sea_orm(has_many = "super::org_invitations::Entity")]
    OrgInvitations,
    #[sea_orm(has_many = "super::workspaces::Entity")]
    Workspaces,
    #[sea_orm(has_many = "super::git_namespaces::Entity")]
    GitNamespaces,
}

impl Related<super::org_members::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::OrgMembers.def()
    }
}

impl Related<super::org_invitations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::OrgInvitations.def()
    }
}

impl Related<super::workspaces::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Workspaces.def()
    }
}

impl Related<super::git_namespaces::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::GitNamespaces.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
