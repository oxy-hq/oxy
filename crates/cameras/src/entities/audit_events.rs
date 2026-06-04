use sea_orm::entity::prelude::*;

/// One row per operator-action audit event (Tier B #7). Written
/// by the service layer at every mutating endpoint; surfaced by
/// the operator UI's Audit tab as "who did what, when."
///
/// Cross-aggregate refs are loose UUIDs (no FK):
///   - `workspace_id` → workspaces, owned by the auth aggregate
///   - `actor_user_id` → users, owned by the auth aggregate; NULL
///     when the action was system-initiated (cron, bg job)
///   - `target_id` → varies by action (edge_box_id, device_id,
///     claim_id, …); the action key disambiguates
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "camera_audit_events")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub actor_user_id: Option<Uuid>,
    /// Canonical action key (snake_case). Examples:
    /// `edge_box.register`, `edge_box.set_target`,
    /// `edge_box.bulk_set_target`, `device.claim`,
    /// `device.bind_existing`, `pack.update`.
    pub action: String,
    /// The primary target — edge_box_id, device_id, or claim_id
    /// depending on action. NULL for workspace-wide actions.
    pub target_id: Option<Uuid>,
    /// Action-specific JSON context. Rendered by the UI as
    /// key/value pairs; keep it small (operator-readable).
    pub details_json: Json,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
