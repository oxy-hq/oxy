use sea_orm::entity::prelude::*;

/// One row per compliance report the operator has arbitrated. Models
/// the operator's CURRENT belief about which pipeline was right, not
/// a history — re-arbitrating updates the row in place via the
/// `compliance_arbitrations_report_unique` index.
///
/// `report_id` is a loose VARCHAR ref into the Airhouse
/// `oxy_cam_compliance_reports.report_id` column. No FK — those rows
/// live in a different database. Workspace ownership is enforced at
/// the route layer before any insert lands here.
///
/// Why Postgres rather than Airhouse: arbitrations are low-volume
/// (~ tens per day per workspace at v0), mutation-friendly, and
/// joined on the operator UI with `users` + `workspaces`. The
/// analytics flow (per-class agreement metrics) reads from Airhouse
/// where the raw signal lives.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "compliance_arbitrations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// Matches `compliance_reports.report_id` over in Airhouse.
    pub report_id: String,
    /// NULL when the path didn't resolve a user (local-mode flows).
    pub arbiter_user_id: Option<Uuid>,
    /// One of `vlm_right` | `yolo_right` | `neither`. Free-text on
    /// the column for forward-compat (a `mixed` verdict makes sense
    /// once arbiters can edit per-class); the route layer is the
    /// canonical validator.
    pub verdict: String,
    pub notes: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
