use sea_orm::entity::prelude::*;
use serde::Serialize;

/// Operator-defined canary rollout for an `target_image_tag`.
///
/// Lifecycle: `pending` → `canary` → `promoting` → `complete`,
/// with `aborted` reachable from any non-terminal state. See
/// `service::rollouts` for the state machine.
///
/// Cohort filter: when `canary_cohort` is set, we apply the
/// target to every edge_box with `edge_boxes.cohort = canary_cohort`
/// during the canary phase. The `promoting` phase applies to
/// every remaining box in the workspace regardless of cohort.
///
/// `failure_threshold_pct` triggers the automatic abort: if the
/// fraction of boxes reporting `revert_failed` /
/// `stuck_at_broken_target` / `reverted` in the canary cohort
/// exceeds this percent, the supervisor flips to `aborted` and
/// stops promoting. Default 5% — one bad box in twenty trips it.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize)]
#[sea_orm(table_name = "camera_rollout_plans")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub target_image_tag: String,
    pub canary_cohort: String,
    pub canary_min_minutes: i32,
    pub failure_threshold_pct: f32,
    /// One of `pending` | `canary` | `promoting` | `complete` | `aborted`.
    /// Stored as text to keep the schema migration-light; service
    /// layer guards transitions.
    pub status: String,
    pub canary_started_at: Option<DateTimeWithTimeZone>,
    pub promotion_started_at: Option<DateTimeWithTimeZone>,
    pub completed_at: Option<DateTimeWithTimeZone>,
    pub aborted_at: Option<DateTimeWithTimeZone>,
    pub abort_reason: Option<String>,
    /// Operator who created the rollout. NULL in local mode.
    pub actor_user_id: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
