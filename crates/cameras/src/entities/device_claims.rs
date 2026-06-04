use sea_orm::entity::prelude::*;

/// Device-workspace binding. One row per claim attempt; a partial
/// unique index keeps at most one `pending_claim`/`claimed` row per
/// device. Revoked rows are retained for audit.
///
/// Lifecycle:
///   `pending_claim` — operator scanned the QR + picked a workspace,
///   but the device hasn't called `/fleet/bootstrap` yet to
///   acknowledge.
///   `claimed` — device successfully bootstrapped. `edge_box_id` is
///   populated; the device now participates in the normal
///   `/control/*` flow under the bound workspace.
///   `revoked` — operator explicitly unclaimed (UI surface to come)
///   or device was returned + re-shipped. A new `pending_claim` row
///   can then be created.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "device_claims")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub device_id: Uuid,
    /// Loose cross-aggregate ref to `workspaces.id` — no FK constraint
    /// (per `domain-boundaries.md` P3). Application-level cleanup.
    pub workspace_id: Uuid,
    /// Site the operator picked at claim time; bootstrap reads it
    /// when materializing the `edge_boxes` row. Nullable for
    /// forward-compat (a future "auto-pick site" flow could leave
    /// it NULL), but the phase 1B claim route requires it.
    pub site_id: Option<Uuid>,
    /// Set on transition to `claimed`. Holds the `edge_boxes.id`
    /// the device materialized as; NULL during `pending_claim`.
    pub edge_box_id: Option<Uuid>,
    /// One of `pending_claim` | `claimed` | `revoked`. Stored as
    /// TEXT (not an enum) so a future state addition doesn't
    /// require a migration.
    pub status: String,
    /// Plain UUID — no FK to the auth aggregate. The user who
    /// landed on the claim page and picked the workspace.
    pub claimed_by: Option<Uuid>,
    pub claimed_at: Option<DateTimeWithTimeZone>,
    pub revoked_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::device_registry::Entity",
        from = "Column::DeviceId",
        to = "super::device_registry::Column::DeviceId",
        on_delete = "Cascade"
    )]
    Device,
}

impl Related<super::device_registry::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Device.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
