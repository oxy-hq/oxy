//! Generic, resumable chunked backfill for airway pipelines.
//!
//! A backfill of a long `[start, end)` range is split into period-aligned
//! chunks (month / week / day). Each chunk is run as a bounded airway backfill
//! (`backfill_from`/`backfill_to`) and its outcome is recorded in a
//! `backfill_checkpoint` row, so a crashed or cancelled backfill **resumes** by
//! skipping chunks already `done`, and the same rows answer "what period is
//! missing?" (any expected chunk that isn't `done`).
//!
//! This module is pipeline-agnostic: the chunk enumeration here is the reusable
//! primitive; the orchestrator drives any `*.airway.yml` whose source honours a
//! `[backfill_from, backfill_to)` window (Toast, QuickBooks, …).

use std::sync::Arc;

use agentic_runtime::crud;
use agentic_runtime::event_registry::EventRegistry;
use agentic_runtime::router::NoopTaskRouter;
use agentic_runtime::state::RuntimeState;
use chrono::{DateTime, Datelike, Duration, FixedOffset, Months, TimeZone, Utc};
use entity::backfill_checkpoints::{
    ActiveModel as CpActive, Column as CpCol, Entity as Checkpoint, Model as CpModel,
};
use entity::backfill_ranges::{
    ActiveModel as RangeActive, Column as RangeCol, Entity as BackfillRange,
};
use futures::stream::StreamExt;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, QueryOrder,
};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use crate::airway_run::{
    AirwayRunError, StartAirwayRequest, spawn_airway_run_drive, start_airway_run,
};
use crate::platform::PlatformContext;
use crate::{AIRWAY_SOURCE_TYPE, TaskScope, WorkflowWorkspaceContext, airway_event_handler};

/// Granularity of a backfill chunk. Pick the coarsest that keeps a single chunk
/// a tractable unit of work + a useful resume/coverage granularity — month is a
/// good default (it also aligns with `year(business_date)`-style partitioning).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkGranularity {
    Day,
    Week,
    Month,
}

impl ChunkGranularity {
    /// Parse from a CLI/config string. `None` for unknown.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "day" | "daily" => Some(Self::Day),
            "week" | "weekly" => Some(Self::Week),
            "month" | "monthly" => Some(Self::Month),
            _ => None,
        }
    }

    /// Canonical lowercase name, for persisting on a `backfill_ranges` row.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
        }
    }

    /// Start of the period containing `t`, in UTC: month → 1st 00:00, week →
    /// Monday 00:00, day → 00:00.
    fn period_start(self, t: DateTime<Utc>) -> DateTime<Utc> {
        let d = t.date_naive();
        let aligned = match self {
            Self::Day => d,
            Self::Week => d - Duration::days(i64::from(d.weekday().num_days_from_monday())),
            Self::Month => d.with_day(1).expect("day 1 is always valid"),
        };
        Utc.from_utc_datetime(&aligned.and_hms_opt(0, 0, 0).expect("midnight is valid"))
    }

    /// The next period boundary at/after the period start of `t`.
    fn next_boundary(self, t: DateTime<Utc>) -> DateTime<Utc> {
        let ps = self.period_start(t);
        match self {
            Self::Day => ps + Duration::days(1),
            Self::Week => ps + Duration::weeks(1),
            Self::Month => ps + Months::new(1),
        }
    }
}

/// One half-open `[start, end)` backfill window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chunk {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Split `[start, end)` into period-aligned half-open chunks. The first chunk
/// starts at `start` (not its period boundary) and the last ends at `end`, so
/// the chunks' union is exactly `[start, end)` with no gaps or overlaps. Empty
/// when `start >= end`. The interior boundaries are aligned (e.g. the 1st of each
/// month), which makes resume/coverage line up with calendar periods.
pub fn enumerate_chunks(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    granularity: ChunkGranularity,
) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut cursor = start;
    while cursor < end {
        let chunk_end = granularity.next_boundary(cursor).min(end);
        chunks.push(Chunk {
            start: cursor,
            end: chunk_end,
        });
        cursor = chunk_end;
    }
    chunks
}

// ── Chunked-backfill orchestration ─────────────────────────────────────────
//
// A backfill of a long `[from, to)` range is split into chunks (above); each
// chunk runs as a bounded airway backfill and its outcome is recorded in a
// `backfill_checkpoints` row, so a crashed/cancelled backfill resumes by
// skipping `done` chunks and the same rows answer "what period is missing?".
// Shared by the `oxy airway backfill` CLI and the HTTP
// `POST /agentic-airway/chunked-backfill` handler.

/// Hard ceiling on how long one chunk may run before we give up waiting and
/// mark it `failed`, so a wedged worker / lost driver lease can't hang the
/// driver forever — the chunk stays resumable instead.
const MAX_CHUNK_WAIT: std::time::Duration = std::time::Duration::from_secs(2 * 60 * 60);

fn build_event_registry() -> EventRegistry {
    let mut registry = EventRegistry::new();
    registry.register(AIRWAY_SOURCE_TYPE, airway_event_handler());
    registry
}

fn is_terminal(status: Option<&str>) -> bool {
    matches!(
        status,
        Some("done") | Some("failed") | Some("cancelled") | Some("timed_out")
    )
}

/// Outcome of one chunk run, derived from the run's **events** (not just its
/// runtime `task_status`): airway finishes a run with skipped resources as
/// `task_status = "done"` + a `LoadCompleted` event, so keying success on
/// `task_status` alone would silently checkpoint a partial load as fully done.
struct ChunkOutcome {
    run_id: String,
    /// `done` (clean) | `completed_with_errors` (loaded, ≥1 resource failed)
    /// | `failed`/`cancelled`/`timed_out` (not complete). Only `done` is treated
    /// as complete on resume; the rest re-run and show in coverage as missing.
    status: String,
    /// Total rows written across all tables (`LoadCompleted.rows_loaded`), or
    /// `None` if the run didn't load.
    row_count: Option<i64>,
    /// Failed-resource list or error message, surfaced into the checkpoint.
    detail: Option<String>,
}

/// Classify a terminated run by reading its event stream: a `pipeline_error`
/// (or a non-`done` runtime status) is a hard failure; any `resource_failed` /
/// `table_load_failed` makes it `completed_with_errors`; otherwise `done`. Also
/// sums `LoadCompleted.rows_loaded` for the checkpoint's `row_count`.
async fn classify_run_outcome(
    db: &DatabaseConnection,
    run_id: &str,
    task_status: &str,
) -> (String, Option<i64>, Option<String>) {
    let mut processor = build_event_registry().stream_processor(AIRWAY_SOURCE_TYPE);
    let mut failed_resources: Vec<String> = Vec::new();
    let mut hard_error: Option<String> = None;
    let mut rows_loaded: i64 = 0;
    let mut saw_load_completed = false;
    for row in crud::get_events_after(db, run_id, -1)
        .await
        .unwrap_or_default()
    {
        for (event_type, payload) in processor.process(&row.event_type, &row.payload) {
            match event_type.as_str() {
                // Both "skipped a table, run still finishes done" events: a
                // ResourceFailed (extract path) AND a TableLoadFailed (streaming
                // load path). Either makes the chunk completed_with_errors — both
                // carry a `table` field, so the same extraction works.
                "resource_failed" | "table_load_failed" => failed_resources.push(
                    payload
                        .get("table")
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                        .to_string(),
                ),
                "pipeline_error" => {
                    hard_error = Some(
                        payload
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown")
                            .to_string(),
                    )
                }
                "load_completed" => {
                    saw_load_completed = true;
                    if let Some(rl) = payload.get("rows_loaded").and_then(Value::as_object) {
                        rows_loaded += rl.values().filter_map(Value::as_u64).sum::<u64>() as i64;
                    }
                }
                _ => {}
            }
        }
    }
    let row_count = saw_load_completed.then_some(rows_loaded);
    if task_status != "done" {
        // cancelled / timed_out / failed — surface the pipeline_error if we have it.
        return (task_status.to_string(), row_count, hard_error);
    }
    if let Some(err) = hard_error {
        return ("failed".to_string(), row_count, Some(err));
    }
    if !failed_resources.is_empty() {
        return (
            "completed_with_errors".to_string(),
            row_count,
            Some(format!("resources failed: {}", failed_resources.join(", "))),
        );
    }
    ("done".to_string(), row_count, None)
}

/// Run one bounded `[from, to)` backfill of `pipeline_ref`, block until it
/// terminates (or `MAX_CHUNK_WAIT` elapses), and classify the outcome. Uses a
/// fresh `RuntimeState` + `NoopTaskRouter` per chunk so it's self-contained
/// (no shared orchestrator state required).
async fn run_airway_window(
    db: &DatabaseConnection,
    platform: &Arc<dyn PlatformContext>,
    pipeline_ref: &str,
    variables: Option<Value>,
    backfill_from: String,
    backfill_to: String,
) -> Result<ChunkOutcome, AirwayRunError> {
    let request = StartAirwayRequest {
        pipeline_ref: pipeline_ref.to_string(),
        variables,
        thread_id: None,
        resources: Vec::new(),
        schedule_id: None,
        trigger: Some("backfill".to_string()),
        logical_date: None,
        retry_of: None,
        backfill_from: Some(backfill_from),
        backfill_to: Some(backfill_to),
    };
    let workspace: Arc<dyn WorkflowWorkspaceContext> = platform.clone();
    let workspace_id = platform.workspace_id();
    let run_id = start_airway_run(
        db,
        workspace.as_ref(),
        request,
        TaskScope::Scoped,
        workspace_id,
    )
    .await?;

    let state = Arc::new(RuntimeState::new());
    let (cancel_tx, cancel_rx) = watch::channel(false);
    state.register(&run_id, mpsc::channel::<String>(1).0, cancel_tx);
    spawn_airway_run_drive(
        db.clone(),
        Arc::clone(&state),
        run_id.clone(),
        Arc::clone(platform),
        cancel_rx,
        Arc::new(NoopTaskRouter),
    );

    let deadline = tokio::time::Instant::now() + MAX_CHUNK_WAIT;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        match crud::get_run(db, &run_id).await? {
            Some(run) if is_terminal(run.task_status.as_deref()) => {
                let task_status = run.task_status.unwrap_or_else(|| "done".into());
                let (status, row_count, detail) =
                    classify_run_outcome(db, &run_id, &task_status).await;
                return Ok(ChunkOutcome {
                    run_id,
                    status,
                    row_count,
                    detail,
                });
            }
            // A vanished run row is an anomaly, not a success — fail it so it
            // retries on the next pass instead of being checkpointed `done`.
            None => {
                return Ok(ChunkOutcome {
                    run_id,
                    status: "failed".into(),
                    row_count: None,
                    detail: Some("run row vanished before reaching a terminal status".into()),
                });
            }
            _ if tokio::time::Instant::now() >= deadline => {
                return Ok(ChunkOutcome {
                    run_id,
                    status: "failed".into(),
                    row_count: None,
                    detail: Some(format!(
                        "timed out after {}s waiting for a terminal status",
                        MAX_CHUNK_WAIT.as_secs()
                    )),
                });
            }
            _ => {}
        }
    }
}

/// Find-or-create the checkpoint row for a chunk within a range (the
/// resume/coverage key).
///
/// Non-atomic find-then-insert: fine for a single sequential driver, but two
/// concurrent drivers of the same range would race the unique
/// `(backfill_range_id, period_start, period_end)` index into a violation
/// rather than no-op'ing. Make it an `ON CONFLICT DO NOTHING` insert if this
/// ever runs from multiple drivers at once.
async fn checkpoint_upsert_pending(
    db: &DatabaseConnection,
    backfill_range_id: Uuid,
    workspace_id: Uuid,
    pipeline_ref: &str,
    start: DateTime<FixedOffset>,
    end: DateTime<FixedOffset>,
) -> Result<CpModel, DbErr> {
    if let Some(existing) = Checkpoint::find()
        .filter(CpCol::BackfillRangeId.eq(backfill_range_id))
        .filter(CpCol::PeriodStart.eq(start))
        .filter(CpCol::PeriodEnd.eq(end))
        .one(db)
        .await?
    {
        return Ok(existing);
    }
    let now = Utc::now().fixed_offset();
    CpActive {
        id: Set(Uuid::new_v4()),
        workspace_id: Set(workspace_id),
        backfill_range_id: Set(backfill_range_id),
        pipeline_ref: Set(pipeline_ref.to_string()),
        period_start: Set(start),
        period_end: Set(end),
        status: Set("pending".to_string()),
        run_id: Set(None),
        row_count: Set(None),
        attempts: Set(0),
        error: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await
}

/// Update a checkpoint's status (and only the touched columns).
async fn checkpoint_set(
    db: &DatabaseConnection,
    cp: &CpModel,
    status: &str,
    run_id: Option<String>,
    error: Option<String>,
    row_count: Option<i64>,
    bump_attempts: bool,
) -> Result<(), DbErr> {
    let mut active: CpActive = cp.clone().into();
    active.status = Set(status.to_string());
    active.run_id = Set(run_id);
    active.error = Set(error);
    active.row_count = Set(row_count);
    active.updated_at = Set(Utc::now().fixed_offset());
    if bump_attempts {
        active.attempts = Set(cp.attempts + 1);
    }
    active.update(db).await?;
    Ok(())
}

/// The disposition of one chunk, for progress reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkDisposition {
    /// Already `done` on a prior pass — skipped.
    Resumed,
    /// Ran clean.
    Done,
    /// Loaded but ≥1 resource failed (`completed_with_errors`).
    Degraded,
    /// Hard failure / timeout / cancelled.
    Failed,
}

/// One progress tick, emitted per chunk as a chunked backfill proceeds so a
/// caller (the CLI) can surface it. `note` carries the failure detail.
pub struct ChunkProgress {
    pub label: String,
    pub disposition: ChunkDisposition,
    pub note: Option<String>,
}

/// Tally of a chunked-backfill pass.
#[derive(Debug, Clone, Default, Serialize)]
pub struct BackfillSummary {
    pub total: usize,
    pub done: usize,
    pub resumed: usize,
    pub degraded: usize,
    pub failed: usize,
}

/// Run one already-checkpointed chunk to completion: skip if `done`, else mark
/// `running`, drive the bounded window, and record the outcome. Returns the
/// progress tick for the driver to fold into the summary. Each chunk touches
/// only its own `backfill_checkpoints` row (distinct `(range, period)`), so this
/// is safe to run concurrently with other chunks of the same range.
async fn run_one_chunk(
    db: DatabaseConnection,
    platform: Arc<dyn PlatformContext>,
    backfill_range_id: Uuid,
    pipeline_ref: String,
    variables: Option<Value>,
    chunk: Chunk,
) -> Result<ChunkProgress, AirwayRunError> {
    let label = format!("{} → {}", chunk.start.date_naive(), chunk.end.date_naive());
    let (ps, pe) = (chunk.start.fixed_offset(), chunk.end.fixed_offset());
    let workspace_id = platform.workspace_id();
    // The row was pre-created by the driver; find-or-create is idempotent.
    let cp = checkpoint_upsert_pending(&db, backfill_range_id, workspace_id, &pipeline_ref, ps, pe)
        .await?;
    if cp.status == "done" {
        return Ok(ChunkProgress {
            label,
            disposition: ChunkDisposition::Resumed,
            note: None,
        });
    }
    checkpoint_set(&db, &cp, "running", None, None, None, true).await?;
    let (disposition, note) = match run_airway_window(
        &db,
        &platform,
        &pipeline_ref,
        variables,
        chunk.start.to_rfc3339(),
        chunk.end.to_rfc3339(),
    )
    .await
    {
        Ok(o) if o.status == "done" => {
            checkpoint_set(&db, &cp, "done", Some(o.run_id), None, o.row_count, false).await?;
            (ChunkDisposition::Done, None)
        }
        Ok(o) => {
            // completed_with_errors / failed / cancelled / timed_out — NOT
            // `done`, so it re-runs next pass and shows in coverage.
            let note = o.detail.clone().unwrap_or_else(|| o.status.clone());
            let disposition = if o.status == "completed_with_errors" {
                ChunkDisposition::Degraded
            } else {
                ChunkDisposition::Failed
            };
            checkpoint_set(
                &db,
                &cp,
                &o.status,
                Some(o.run_id),
                o.detail,
                o.row_count,
                false,
            )
            .await?;
            (disposition, Some(note))
        }
        Err(e) => {
            let note = e.to_string();
            checkpoint_set(&db, &cp, "failed", None, Some(note.clone()), None, false).await?;
            (ChunkDisposition::Failed, Some(note))
        }
    };
    Ok(ChunkProgress {
        label,
        disposition,
        note,
    })
}

/// Record a user-initiated backfill of `[from, to)` as a `backfill_ranges` row
/// and return its id. Chunks created under this id are owned by exactly this
/// range (per-run) — overlapping backfills are distinct ranges, never merged.
#[allow(clippy::too_many_arguments)]
pub async fn create_backfill_range(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    pipeline_ref: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    granularity: ChunkGranularity,
    concurrency: i32,
    created_by: Option<Uuid>,
) -> Result<Uuid, DbErr> {
    let id = Uuid::new_v4();
    let now = Utc::now().fixed_offset();
    RangeActive {
        id: Set(id),
        workspace_id: Set(workspace_id),
        pipeline_ref: Set(pipeline_ref.to_string()),
        requested_from: Set(from.fixed_offset()),
        requested_to: Set(to.fixed_offset()),
        granularity: Set(granularity.as_str().to_string()),
        concurrency: Set(concurrency),
        created_by: Set(created_by),
        status: Set("running".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await?;
    Ok(id)
}

/// Find an existing range that exactly matches this window (same workspace,
/// pipeline, `[from, to)`, granularity) or create one. Used by the CLI so that
/// re-running `oxy airway backfill` with the same window RESUMES that range
/// (drives only its not-`done` chunks) instead of spawning a duplicate range
/// that re-runs everything. The HTTP path deliberately creates a fresh range per
/// request — each user-initiated backfill is its own entry in the ranges gantt,
/// and a range's own "Resume" is the explicit resume path there.
#[allow(clippy::too_many_arguments)]
pub async fn find_or_create_backfill_range(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    pipeline_ref: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    granularity: ChunkGranularity,
    concurrency: i32,
    created_by: Option<Uuid>,
) -> Result<Uuid, DbErr> {
    if let Some(existing) = BackfillRange::find()
        .filter(RangeCol::WorkspaceId.eq(workspace_id))
        .filter(RangeCol::PipelineRef.eq(pipeline_ref))
        .filter(RangeCol::RequestedFrom.eq(from.fixed_offset()))
        .filter(RangeCol::RequestedTo.eq(to.fixed_offset()))
        .filter(RangeCol::Granularity.eq(granularity.as_str()))
        .order_by_desc(RangeCol::CreatedAt)
        .one(db)
        .await?
    {
        return Ok(existing.id);
    }
    create_backfill_range(
        db,
        workspace_id,
        pipeline_ref,
        from,
        to,
        granularity,
        concurrency,
        created_by,
    )
    .await
}

/// Drive a backfill range to completion: split its `[requested_from,
/// requested_to)` window into `granularity` chunks and run each as a bounded
/// airway backfill, checkpointing every outcome under `range_id`. Resumable —
/// chunks already `done` are skipped, so re-driving retries only failed/partial
/// chunks. No cross-range merge: this range owns exactly the chunks its window
/// enumerates, and re-loading a period another range already covered is safe
/// (airhouse is merge-on-read). The range `status` is rolled up from its chunks
/// when the pass finishes.
///
/// Up to the range's `concurrency` chunks run at once (`buffer_unordered`);
/// `on_progress` fires once per chunk in *completion* order (the CLI prints; the
/// HTTP driver passes a no-op). Each chunk is a full airway run writing the same
/// table, and DuckLake serializes catalog commits per table — so a high
/// concurrency mostly trades parallel extract for commit-time conflict-retries.
/// A modest value (≈4) is the sweet spot; checkpoints make any conflict-failed
/// chunk safely resumable.
pub async fn drive_backfill_range(
    db: &DatabaseConnection,
    platform: Arc<dyn PlatformContext>,
    range_id: Uuid,
    variables: Option<Value>,
    on_progress: impl FnMut(ChunkProgress),
) -> Result<BackfillSummary, AirwayRunError> {
    let range = BackfillRange::find_by_id(range_id)
        .one(db)
        .await?
        .ok_or_else(|| {
            AirwayRunError::InvalidInput(format!("backfill range {range_id} not found"))
        })?;
    let from = range.requested_from.with_timezone(&Utc);
    let to = range.requested_to.with_timezone(&Utc);
    let granularity =
        ChunkGranularity::parse(&range.granularity).unwrap_or(ChunkGranularity::Month);
    let concurrency = range.concurrency.max(1) as usize;
    let chunks = enumerate_chunks(from, to, granularity);
    // Pre-create every chunk as `pending` so coverage shows the plan (0/N) at once.
    for chunk in &chunks {
        checkpoint_upsert_pending(
            db,
            range_id,
            range.workspace_id,
            &range.pipeline_ref,
            chunk.start.fixed_offset(),
            chunk.end.fixed_offset(),
        )
        .await?;
    }
    let summary = run_chunks(
        db,
        platform,
        range_id,
        &range.pipeline_ref,
        variables,
        chunks,
        concurrency,
        on_progress,
    )
    .await?;
    rollup_range_status(db, range_id).await?;
    Ok(summary)
}

/// Drive a fixed set of one range's chunks concurrently (≤ `concurrency` at
/// once), folding each outcome into a `BackfillSummary`. Shared by
/// `drive_backfill_range` (the window) and `resume_backfill_range` (the missing
/// chunks).
#[allow(clippy::too_many_arguments)]
async fn run_chunks(
    db: &DatabaseConnection,
    platform: Arc<dyn PlatformContext>,
    backfill_range_id: Uuid,
    pipeline_ref: &str,
    variables: Option<Value>,
    chunks: Vec<Chunk>,
    concurrency: usize,
    mut on_progress: impl FnMut(ChunkProgress),
) -> Result<BackfillSummary, AirwayRunError> {
    let mut summary = BackfillSummary {
        total: chunks.len(),
        ..Default::default()
    };
    // Each chunk future owns its inputs (cheap clones — `DatabaseConnection`
    // and `Arc` are ref-counted) so the buffered set is `'static + Send` and can
    // be driven from a detached `tokio::spawn`. Iterate `Chunk` by value
    // (it's `Copy`): a `&Chunk`-taking closure returning a future trips a
    // higher-ranked-lifetime `FnOnce is not general enough` error.
    let mut running = futures::stream::iter(chunks.into_iter().map(|chunk| {
        run_one_chunk(
            db.clone(),
            platform.clone(),
            backfill_range_id,
            pipeline_ref.to_string(),
            variables.clone(),
            chunk,
        )
    }))
    .buffer_unordered(concurrency.max(1));
    while let Some(progress) = running.next().await.transpose()? {
        match progress.disposition {
            ChunkDisposition::Resumed => summary.resumed += 1,
            ChunkDisposition::Done => summary.done += 1,
            ChunkDisposition::Degraded => summary.degraded += 1,
            ChunkDisposition::Failed => summary.failed += 1,
        }
        on_progress(progress);
    }
    Ok(summary)
}

/// Recompute a range's rollup `status` from its chunks and persist it if
/// changed: `running` while any chunk is pending/running, else `done` (all
/// done), `failed` (any hard failure), or `degraded` (some
/// `completed_with_errors`, the rest done).
async fn rollup_range_status(db: &DatabaseConnection, range_id: Uuid) -> Result<(), DbErr> {
    let statuses: Vec<String> = Checkpoint::find()
        .filter(CpCol::BackfillRangeId.eq(range_id))
        .all(db)
        .await?
        .into_iter()
        .map(|r| r.status)
        .collect();
    let rolled = if statuses.is_empty() {
        "done"
    } else if statuses.iter().any(|s| s == "pending" || s == "running") {
        "running"
    } else if statuses.iter().all(|s| s == "done") {
        "done"
    } else if statuses
        .iter()
        .any(|s| s == "failed" || s == "timed_out" || s == "cancelled")
    {
        "failed"
    } else {
        "degraded"
    };
    if let Some(range) = BackfillRange::find_by_id(range_id).one(db).await?
        && range.status != rolled
    {
        let mut active: RangeActive = range.into();
        active.status = Set(rolled.to_string());
        active.updated_at = Set(Utc::now().fixed_offset());
        active.update(db).await?;
    }
    Ok(())
}

/// Resume a backfill range by re-running exactly its not-`done` chunks, read
/// straight from the checkpoints owned by `range_id` — so only the actual
/// missing periods run (never a re-derived window), independent of the original
/// granularity. `run_one_chunk` skips any that flipped to `done` since the read,
/// so it's race-safe. Rolls the range status up afterward.
pub async fn resume_backfill_range(
    db: &DatabaseConnection,
    platform: Arc<dyn PlatformContext>,
    range_id: Uuid,
    variables: Option<Value>,
    on_progress: impl FnMut(ChunkProgress),
) -> Result<BackfillSummary, AirwayRunError> {
    // Gate on the caller's workspace so a range_id from another tenant can't be
    // driven cross-tenant.
    let range = BackfillRange::find_by_id(range_id)
        .filter(RangeCol::WorkspaceId.eq(platform.workspace_id()))
        .one(db)
        .await?
        .ok_or_else(|| {
            AirwayRunError::InvalidInput(format!("backfill range {range_id} not found"))
        })?;
    let concurrency = range.concurrency.max(1) as usize;
    let chunks: Vec<Chunk> = Checkpoint::find()
        .filter(CpCol::BackfillRangeId.eq(range_id))
        .filter(CpCol::Status.ne("done"))
        .order_by_asc(CpCol::PeriodStart)
        .all(db)
        .await?
        .into_iter()
        .map(|r| Chunk {
            start: r.period_start.with_timezone(&Utc),
            end: r.period_end.with_timezone(&Utc),
        })
        .collect();
    let summary = run_chunks(
        db,
        platform,
        range_id,
        &range.pipeline_ref,
        variables,
        chunks,
        concurrency,
        on_progress,
    )
    .await?;
    rollup_range_status(db, range_id).await?;
    Ok(summary)
}

/// One chunk's coverage row (serialized for the HTTP coverage endpoint and the
/// CLI `--json` output).
#[derive(Debug, Clone, Serialize)]
pub struct CoverageChunk {
    pub period_start: DateTime<FixedOffset>,
    pub period_end: DateTime<FixedOffset>,
    pub status: String,
    pub run_id: Option<String>,
    pub row_count: Option<i64>,
    pub attempts: i32,
    pub error: Option<String>,
}

/// Rollup of coverage: how many chunks are `done`, the loaded *envelope*
/// (min/max over `done` chunks — NOT necessarily gap-free), and how many chunks
/// are still missing.
#[derive(Debug, Clone, Serialize)]
pub struct CoverageSummary {
    pub total: usize,
    pub done: usize,
    /// Earliest `period_start` / latest `period_end` across `done` chunks, or
    /// `null` when nothing is done yet. This is an envelope: an interior
    /// not-`done` chunk does not narrow it — check `missing` / per-chunk status.
    pub loaded_from: Option<DateTime<FixedOffset>>,
    pub loaded_to: Option<DateTime<FixedOffset>>,
    pub missing: usize,
}

/// Full coverage report — every checkpoint chunk plus the rollup. Scoped either
/// to a single range (`range_id = Some`, the drill-in view) or to a whole
/// pipeline across all its ranges (`range_id = None`, the CLI/overall view).
#[derive(Debug, Clone, Serialize)]
pub struct CoverageReport {
    pub pipeline_ref: String,
    /// The range this report covers, or `None` for a pipeline-wide aggregate.
    pub range_id: Option<Uuid>,
    pub chunks: Vec<CoverageChunk>,
    pub summary: CoverageSummary,
}

/// Roll a set of checkpoint rows up into a `CoverageReport`.
fn build_coverage(
    pipeline_ref: String,
    range_id: Option<Uuid>,
    rows: Vec<CpModel>,
) -> CoverageReport {
    let total = rows.len();
    let done = rows.iter().filter(|r| r.status == "done").count();
    let loaded_from = rows
        .iter()
        .filter(|r| r.status == "done")
        .map(|r| r.period_start)
        .min();
    let loaded_to = rows
        .iter()
        .filter(|r| r.status == "done")
        .map(|r| r.period_end)
        .max();
    let chunks = rows
        .into_iter()
        .map(|r| CoverageChunk {
            period_start: r.period_start,
            period_end: r.period_end,
            status: r.status,
            run_id: r.run_id,
            row_count: r.row_count,
            attempts: r.attempts,
            error: r.error,
        })
        .collect();
    CoverageReport {
        pipeline_ref,
        range_id,
        chunks,
        summary: CoverageSummary {
            total,
            done,
            loaded_from,
            loaded_to,
            missing: total - done,
        },
    }
}

/// Pipeline-wide coverage across *all* of `pipeline_ref`'s ranges (ordered by
/// period). Answers "what period is missing?" — any chunk that isn't `done`.
/// Note: with per-run ranges a period backfilled by two ranges appears as two
/// chunk rows, so this aggregate counts chunk *rows*, not distinct periods.
pub async fn load_coverage(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    pipeline_ref: &str,
) -> Result<CoverageReport, DbErr> {
    let rows = Checkpoint::find()
        .filter(CpCol::WorkspaceId.eq(workspace_id))
        .filter(CpCol::PipelineRef.eq(pipeline_ref))
        .order_by_asc(CpCol::PeriodStart)
        .all(db)
        .await?;
    Ok(build_coverage(pipeline_ref.to_string(), None, rows))
}

/// Coverage for a single backfill range (the UI drill-in): the range's own
/// chunk rows, ordered by period. Gated on `workspace_id` so a `range_id` from
/// another tenant returns an empty report rather than leaking its chunks.
pub async fn load_range_coverage(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    range_id: Uuid,
) -> Result<CoverageReport, DbErr> {
    let Some(range) = BackfillRange::find_by_id(range_id)
        .filter(RangeCol::WorkspaceId.eq(workspace_id))
        .one(db)
        .await?
    else {
        return Ok(build_coverage(String::new(), Some(range_id), Vec::new()));
    };
    let rows = Checkpoint::find()
        .filter(CpCol::BackfillRangeId.eq(range_id))
        .order_by_asc(CpCol::PeriodStart)
        .all(db)
        .await?;
    Ok(build_coverage(range.pipeline_ref, Some(range_id), rows))
}

/// One backfill range plus its chunk tally, for the ranges list / gantt.
#[derive(Debug, Clone, Serialize)]
pub struct BackfillRangeInfo {
    pub id: Uuid,
    pub pipeline_ref: String,
    pub requested_from: DateTime<FixedOffset>,
    pub requested_to: DateTime<FixedOffset>,
    pub granularity: String,
    pub concurrency: i32,
    pub created_by: Option<Uuid>,
    pub status: String,
    pub created_at: DateTime<FixedOffset>,
    pub updated_at: DateTime<FixedOffset>,
    /// Chunk tally for this range.
    pub total: usize,
    pub done: usize,
    pub missing: usize,
}

/// List a pipeline's backfill ranges (newest first) with each range's chunk
/// tally — the source for the ranges gantt. One checkpoint scan, grouped by
/// range in memory.
pub async fn list_backfill_ranges(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    pipeline_ref: &str,
) -> Result<Vec<BackfillRangeInfo>, DbErr> {
    let ranges = BackfillRange::find()
        .filter(RangeCol::WorkspaceId.eq(workspace_id))
        .filter(RangeCol::PipelineRef.eq(pipeline_ref))
        .order_by_desc(RangeCol::CreatedAt)
        .all(db)
        .await?;
    let checkpoints = Checkpoint::find()
        .filter(CpCol::WorkspaceId.eq(workspace_id))
        .filter(CpCol::PipelineRef.eq(pipeline_ref))
        .all(db)
        .await?;
    let mut tally: std::collections::HashMap<Uuid, (usize, usize)> =
        std::collections::HashMap::new();
    for c in &checkpoints {
        let e = tally.entry(c.backfill_range_id).or_insert((0, 0));
        e.0 += 1;
        if c.status == "done" {
            e.1 += 1;
        }
    }
    Ok(ranges
        .into_iter()
        .map(|r| {
            let (total, done) = tally.get(&r.id).copied().unwrap_or((0, 0));
            BackfillRangeInfo {
                id: r.id,
                pipeline_ref: r.pipeline_ref,
                requested_from: r.requested_from,
                requested_to: r.requested_to,
                granularity: r.granularity,
                concurrency: r.concurrency,
                created_by: r.created_by,
                status: r.status,
                created_at: r.created_at,
                updated_at: r.updated_at,
                total,
                done,
                missing: total - done,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn month_chunks_are_calendar_aligned_and_clamped() {
        let chunks = enumerate_chunks(
            ts("2024-01-15T00:00:00Z"),
            ts("2024-03-10T00:00:00Z"),
            ChunkGranularity::Month,
        );
        assert_eq!(
            chunks,
            vec![
                Chunk {
                    start: ts("2024-01-15T00:00:00Z"),
                    end: ts("2024-02-01T00:00:00Z")
                },
                Chunk {
                    start: ts("2024-02-01T00:00:00Z"),
                    end: ts("2024-03-01T00:00:00Z")
                },
                Chunk {
                    start: ts("2024-03-01T00:00:00Z"),
                    end: ts("2024-03-10T00:00:00Z")
                },
            ]
        );
    }

    #[test]
    fn chunks_tile_the_range_with_no_gaps_or_overlaps() {
        let start = ts("2023-11-20T00:00:00Z");
        let end = ts("2024-02-03T00:00:00Z");
        for g in [
            ChunkGranularity::Day,
            ChunkGranularity::Week,
            ChunkGranularity::Month,
        ] {
            let chunks = enumerate_chunks(start, end, g);
            assert_eq!(
                chunks.first().unwrap().start,
                start,
                "{g:?} starts at start"
            );
            assert_eq!(chunks.last().unwrap().end, end, "{g:?} ends at end");
            for w in chunks.windows(2) {
                assert_eq!(w[0].end, w[1].start, "{g:?} contiguous, no gap/overlap");
            }
        }
    }

    #[test]
    fn empty_or_inverted_range_yields_no_chunks() {
        let t = ts("2024-01-01T00:00:00Z");
        assert!(enumerate_chunks(t, t, ChunkGranularity::Month).is_empty());
        assert!(enumerate_chunks(t, t - Duration::days(1), ChunkGranularity::Day).is_empty());
    }

    #[test]
    fn week_aligns_to_monday() {
        // 2024-01-03 is a Wednesday → first interior boundary is Mon 2024-01-08.
        let chunks = enumerate_chunks(
            ts("2024-01-03T00:00:00Z"),
            ts("2024-01-20T00:00:00Z"),
            ChunkGranularity::Week,
        );
        assert_eq!(chunks[0].end, ts("2024-01-08T00:00:00Z"));
        assert_eq!(chunks[1].end, ts("2024-01-15T00:00:00Z"));
    }

    #[test]
    fn granularity_parse() {
        assert_eq!(
            ChunkGranularity::parse("Monthly"),
            Some(ChunkGranularity::Month)
        );
        assert_eq!(ChunkGranularity::parse("day"), Some(ChunkGranularity::Day));
        assert_eq!(ChunkGranularity::parse("fortnight"), None);
    }
}
