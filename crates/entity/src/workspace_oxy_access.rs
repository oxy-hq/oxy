use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// One row per workspace that has opted into "Oxy can build tailored
/// apps on your data". Combined with the `app_admins` table on the
/// access path: any user in `app_admins` can access customer apps in
/// workspaces that have a row here.
///
/// Unique on `workspace_id` — presence of the row IS the toggle. The
/// audit trail (`granted_by`, `created_at`) is incidental but useful
/// in support tickets.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "workspace_oxy_access")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub workspace_id: Uuid,
    /// User who enabled access. NULL only if that user was deleted.
    pub granted_by: Option<Uuid>,
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
        from = "Column::GrantedBy",
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
