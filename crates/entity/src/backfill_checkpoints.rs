//! `backfill_checkpoints` — one row per `(pipeline, period chunk)` of a chunked
//! backfill (see `agentic_pipeline::backfill`). The row is the unit of work,
//! the resume key (skip `status = 'done'`), and the "what period is missing?"
//! report (any expected chunk whose row is absent or not `done`).

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "backfill_checkpoints")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// Owning workspace. Scopes every checkpoint so two workspaces with the same
    /// relative `pipeline_ref` path never collide in shared multi-tenant
    /// Postgres — otherwise coverage would leak the other tenant's chunks and
    /// resume could skip a period another tenant marked `done`. `Uuid::nil()`
    /// (LOCAL_WORKSPACE_ID) for the single-tenant local/CLI path.
    pub workspace_id: Uuid,
    /// The `backfill_ranges` row that owns this chunk. Chunks belong to exactly
    /// one range (per-run): the same period backfilled by two ranges is two
    /// rows. Unique per `(backfill_range_id, period_start, period_end)`.
    pub backfill_range_id: Uuid,
    /// The `*.airway.yml` pipeline this chunk belongs to (denormalized from the
    /// owning range for cheap pipeline-wide coverage scans).
    pub pipeline_ref: String,
    /// Half-open chunk window `[period_start, period_end)` (the airway
    /// `backfill_from`/`backfill_to`). Unique per `(pipeline_ref, …)`.
    pub period_start: DateTimeWithTimeZone,
    pub period_end: DateTimeWithTimeZone,
    /// `pending` | `running` | `done` | `failed`.
    pub status: String,
    /// The `agentic_runs` id of the latest attempt (for drill-down).
    pub run_id: Option<String>,
    /// Rows loaded by the successful run (best-effort, for coverage stats).
    pub row_count: Option<i64>,
    /// How many times this chunk has been attempted.
    pub attempts: i32,
    /// Last failure message, when `status = 'failed'`.
    pub error: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
