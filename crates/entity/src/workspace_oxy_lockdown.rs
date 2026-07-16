use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// One row per workspace that has **locked Oxy staff out**.
///
/// This is the inverse of the old `workspace_oxy_access` consent table: the
/// default (NO row) is that Oxy staff (`app_admins`) may access the workspace's
/// customer apps, so support works without the customer having to opt in. A row
/// here revokes that.
///
/// Tenant-sovereign: only a **real** org owner/admin may create or remove it —
/// the synthesized global-operator Owner membership is rejected, so Oxy staff
/// cannot unlock themselves. (The old toggle was guarded on the workspace Owner
/// role, which operators get synthetically — meaning staff could grant themselves
/// the very access the toggle existed to gate.)
///
/// Unique on `workspace_id` — presence of the row IS the lockdown. `locked_by` /
/// `created_at` are the audit trail.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "workspace_oxy_lockdown")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub workspace_id: Uuid,
    /// The org officer who locked Oxy out. NULL only if that user was deleted.
    pub locked_by: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
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
        from = "Column::LockedBy",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "SetNull"
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
