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
    /// When Slack was last paged about this workspace being unhealthy. Drives the
    /// re-alert reminder, so it is distinct from `updated_at` (every pass) and
    /// `changed_at` (every transition). Cleared when the workspace leaves
    /// unhealthy, so the next outage always pages immediately.
    pub last_alerted_at: Option<DateTimeWithTimeZone>,
    /// The failing dimensions carried by that last alert, as a JSON array of
    /// `{dimension, status}`. Compared against the current failure set so a
    /// workspace that picks up a *new* (or worse) failure while already unhealthy
    /// re-pages immediately instead of waiting out the reminder interval. `None`
    /// when nothing has been alerted.
    ///
    /// Deliberately dimensions and not the reason strings from `reasons`: reason
    /// text embeds live counts and percentages that move on nearly every pass, so
    /// diffing it would re-page continuously and defeat the reminder interval.
    pub alerted_failures: Option<Json>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
