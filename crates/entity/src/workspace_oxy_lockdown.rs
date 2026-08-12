use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// One row per workspace that has **locked Oxy staff out**.
///
/// This is the inverse of the old `workspace_oxy_access` consent table: the
/// default (NO row) is that Oxy staff (`app_admins`) may access the workspace's
/// custom apps, so support works without the customer having to opt in. A row
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
#[sea_orm::model]
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
        from = "locked_by",
        to = "id",
        on_update = "NoAction",
        on_delete = "SetNull"
    )]
    #[serde(skip)]
    pub users: BelongsTo<Option<super::users::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
