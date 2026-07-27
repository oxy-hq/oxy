use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Last-known health status per workspace. One row per workspace; updated by
/// the periodic health-eval job. Drives transition-only Slack alerting.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "workspace_health_state")]
pub struct Model {
    /// `workspace_id` is the natural key — exactly one state row per workspace.
    #[sea_orm(primary_key, auto_increment = false)]
    pub workspace_id: Uuid,
    /// `healthy` | `degraded` | `unhealthy`
    pub status: String,
    /// JSON array of human-readable reason strings for the current status.
    pub reasons: Json,
    /// When `status` last changed value (not when the row was last touched).
    pub changed_at: DateTimeWithTimeZone,
    /// When this row was last written by an eval pass.
    pub updated_at: DateTimeWithTimeZone,
    /// Full serialized health rollup (dimensions + signals + reconciliation),
    /// written by the sweep and returned verbatim by the read endpoint. `None`
    /// until the first sweep records this workspace.
    pub payload: Option<Json>,
    /// When the workspace smoke test last actually ran. The smoke probes are on
    /// their own slower cadence than the eval pass, so most passes read this,
    /// decide the interval hasn't elapsed, and reuse the cached verdicts from
    /// `payload`. `None` until the first smoke run (or when smoke is disabled).
    pub last_smoke_at: Option<DateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
