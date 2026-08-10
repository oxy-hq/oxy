//! Anomaly Inbox API — list / scan / acknowledge / dismiss.
//!
//! The detection algorithm + `.monitor.yml` parsing live in
//! `oxy-metric-monitoring`. This module is the HTTP surface plus the
//! `MonitorOutcome` → `metric_anomalies` row upsert. Scans run inline on
//! the request thread for now; the eventual scheduler tick will reuse the
//! same `run_scan` helper from a background task.

use agentic_http::AgenticState;
use agentic_runtime::lifecycle::crud::runs::{insert_run, update_run_done, update_run_failed};
use axum::{
    extract::{Extension, Json, Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{TimeZone, Utc};
use entity::metric_anomalies::{self, Entity as AnomaliesEntity};
use entity::metric_monitor_coverage::{self, Entity as CoverageEntity};
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_metric_monitoring as monitoring;
use sea_orm::ActiveValue::Set;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, QueryOrder,
    QuerySelect,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use crate::agentic_wiring::OxyMetricTreeRunner;
use crate::server::api::middlewares::workspace_context::{
    EffectiveWorkspaceRole, WorkspaceManagerExtractor,
};

// ── Errors ──────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum AnomalyError {
    Db(sea_orm::DbErr),
    Scan(monitoring::ScanError),
    NotFound,
    BadStatus(String),
    Internal(String),
}

impl IntoResponse for AnomalyError {
    fn into_response(self) -> Response {
        match self {
            AnomalyError::Db(e) => {
                tracing::error!("metric_anomalies db error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response()
            }
            AnomalyError::Scan(e) => {
                tracing::error!("metric_anomalies scan error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("scan failed: {e}"),
                )
                    .into_response()
            }
            AnomalyError::NotFound => (StatusCode::NOT_FOUND, "anomaly not found").into_response(),
            AnomalyError::BadStatus(s) => (
                StatusCode::BAD_REQUEST,
                format!("invalid status '{s}' (expected: new | acknowledged | dismissed)"),
            )
                .into_response(),
            AnomalyError::Internal(msg) => {
                tracing::error!("metric_anomalies internal error: {msg}");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
            }
        }
    }
}

impl From<sea_orm::DbErr> for AnomalyError {
    fn from(e: sea_orm::DbErr) -> Self {
        AnomalyError::Db(e)
    }
}

// ── List ────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
pub struct ListQuery {
    /// Filter by status. `None` returns every status.
    pub status: Option<String>,
    /// Max **events** (not rows) for the default severity ranking; max **rows**
    /// for `order=recent`. Defaults to 100. Under the default ranking every
    /// bucket of a returned event comes back, so the row count is
    /// `limit × buckets-per-event`, bounded defensively in [`load_ranked_events`].
    pub limit: Option<u64>,
    /// `recent` orders by `detected_at DESC` (row-limited; index-served when a
    /// status is set — see [`list_recent`]) for consumers that want the latest
    /// firings — e.g. the Monitors tab's "last anomaly" column. Any other value
    /// (default) ranks worst-first by event
    /// severity for the Insights Inbox.
    pub order: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListResponse {
    pub anomalies: Vec<metric_anomalies::Model>,
}

#[derive(Debug, Serialize)]
pub struct ListMonitorsResponse {
    pub monitors: Vec<monitoring::config::MonitorEntry>,
    /// One row per **scanned segment** (a `group_by` monitor fans out to many),
    /// recording how much history it has against how much it needs.
    ///
    /// Without this the tab cannot distinguish a healthy monitor that found
    /// nothing from one that is not being scored at all — both show an empty
    /// inbox. Empty until the first scan after this shipped.
    #[serde(default)]
    pub coverage: Vec<entity::metric_monitor_coverage::Model>,
}

/// Per-segment scan coverage for a workspace.
///
/// A persisted-data read, so it stays `FleetOk` — unlike the `.monitor.yml`
/// fallback in the caller, which is why the compiled fast path exists at all.
///
/// Never fails the request: coverage is advisory, and losing the monitor list
/// over a status column would be a worse outcome than the ambiguity this column
/// exists to remove.
async fn load_coverage(
    db: &sea_orm::DatabaseConnection,
    workspace_id: Uuid,
) -> Vec<entity::metric_monitor_coverage::Model> {
    CoverageEntity::find()
        .filter(metric_monitor_coverage::Column::WorkspaceId.eq(workspace_id))
        .order_by_asc(metric_monitor_coverage::Column::Measure)
        .all(db)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                workspace_id = %workspace_id,
                error = %e,
                "list_monitors: coverage lookup failed; returning monitors without it"
            );
            Vec::new()
        })
}

/// `GET /workspaces/{workspace_id}/semantic/monitors` — list every entry in
/// `.monitor.yml`, plus per-segment scan coverage. Returns an empty list when
/// the file is missing or empty. Returns 400 when it exists but fails to parse.
pub async fn list_monitors(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Extension(state): Extension<Arc<AgenticState>>,
) -> Result<Json<ListMonitorsResponse>, (StatusCode, String)> {
    let workspace_id = workspace_manager.workspace_id;
    let coverage = load_coverage(&state.db, workspace_id).await;

    // Compile-boundary fast path. When the workspace is promoted, hydrate the
    // MonitorConfig from `monitor_configs` and skip the .monitor.yml disk read.
    if let Ok(Some(definition)) =
        crate::server::api::compiled_reader::resolve_monitor_config(workspace_id, None).await
    {
        match serde_json::from_value::<monitoring::config::MonitorConfig>(definition) {
            Ok(cfg) => {
                return Ok(Json(ListMonitorsResponse {
                    monitors: cfg.monitors,
                    coverage,
                }));
            }
            Err(e) => tracing::warn!(
                workspace_id = %workspace_id,
                error = ?e,
                "list_monitors: compiled monitor config deserialise failed; falling through to FS"
            ),
        }
    }

    let workspace_root = workspace_manager.config_manager.workspace_path();
    let config_path = monitoring::config::default_config_path(workspace_root);
    let config = monitoring::config::load_from_file(&config_path)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(ListMonitorsResponse {
        monitors: config.monitors,
        coverage,
    }))
}

/// `GET /workspaces/{workspace_id}/semantic/anomalies?status=new&limit=100`
/// (`limit` counts **events**, not rows).
///
/// Two-phase so the inbox ranks on severity without truncating an event
/// mid-way:
///  1. [`rank_event_keys`] ranks *events* worst-first and takes the top `limit`.
///  2. [`load_ranked_events`] fetches every row of exactly those events (modulo
///     the status filter — a filtered tab still returns only its status).
///
/// Ranking per event (not per row) is what keeps the per-event Ack loop and the
/// `worst of N` bucket count honest *within the selected tab*: a waived-band
/// continuation bucket is filed `low`, so a row-limited severity sort would
/// strand those buckets off the page.
pub async fn list_anomalies(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Extension(state): Extension<Arc<AgenticState>>,
    Query(q): Query<ListQuery>,
) -> Result<Json<ListResponse>, AnomalyError> {
    let workspace_id = workspace_manager.workspace_id;
    let limit = q.limit.unwrap_or(100).min(500);
    let status = q.status.as_deref();

    // Recency path: a plain `detected_at DESC` row-limited scan for consumers
    // that want the latest firings rather than the worst. Kept separate so the
    // Insights Inbox's severity ranking can't regress this (a measure whose
    // recent anomalies are all `low` would fall off a severity-ranked page).
    if q.order.as_deref() == Some("recent") {
        let anomalies = list_recent(&state.db, workspace_id, status, limit).await?;
        return Ok(Json(ListResponse { anomalies }));
    }

    let keys = rank_event_keys(&state.db, workspace_id, status, limit).await?;
    if keys.is_empty() {
        return Ok(Json(ListResponse { anomalies: vec![] }));
    }
    let anomalies = load_ranked_events(&state.db, workspace_id, status, &keys).await?;
    Ok(Json(ListResponse { anomalies }))
}

/// Latest firings first — a `detected_at DESC` scan. When a status is set the
/// `idx_metric_anomalies_workspace_status_detected` index serves the sort
/// directly; the Monitors-tab caller passes no status, so there Postgres scans
/// the workspace's anomaly rows and sorts before the `LIMIT` (the same shape the
/// endpoint had pre-PR — a `(workspace_id, detected_at)` index would bound it if
/// that path ever gets hot).
async fn list_recent(
    db: &sea_orm::DatabaseConnection,
    workspace_id: Uuid,
    status: Option<&str>,
    limit: u64,
) -> Result<Vec<metric_anomalies::Model>, AnomalyError> {
    let mut query = AnomaliesEntity::find()
        .filter(metric_anomalies::Column::WorkspaceId.eq(workspace_id))
        .order_by_desc(metric_anomalies::Column::DetectedAt)
        .limit(limit);
    if let Some(status) = status {
        query = query.filter(metric_anomalies::Column::Status.eq(status));
    }
    Ok(query.all(db).await?)
}

/// The event key for a row — its `event_id`, or its own id for pre-event rows.
/// Mirrors the frontend's `event_id ?? "ungrouped:" + id` grouping key.
fn event_key_of(m: &metric_anomalies::Model) -> String {
    m.event_id
        .map(|e| e.to_string())
        .unwrap_or_else(|| m.id.to_string())
}

/// Rank the workspace's anomaly **events** worst-first and return the top
/// `limit` event keys.
///
/// Grouped, not windowed. `GROUP BY` + `ORDER BY MAX(...)` can't push the
/// `LIMIT` into the scan, so this still reads every row matching the `WHERE`
/// — but at a much smaller constant than the window version it replaced (narrow
/// projection, one sort of *groups* instead of a `WindowAgg` plus a full-row
/// sort). The status predicate is appended only when set, so the default
/// **New** tab keeps the `status` column of
/// `idx_metric_anomalies_workspace_status_detected` sargable rather than folding
/// it into a non-sargable `OR`. The `LIMIT` bounds *events*, and phase 2 fetches
/// each returned event whole — the property the row-limited window ordering
/// could not give. Active events sort ahead of fully-dismissed ones so the
/// `all` tab doesn't spend its page budget on dismissed rows.
async fn rank_event_keys(
    db: &sea_orm::DatabaseConnection,
    workspace_id: Uuid,
    status: Option<&str>,
    limit: u64,
) -> Result<Vec<String>, AnomalyError> {
    use sea_orm::{ConnectionTrait, DbBackend, Statement, Value};
    let mut params: Vec<Value> = vec![workspace_id.into()];
    let status_clause = match status {
        Some(s) => {
            params.push(s.to_string().into());
            "AND status = $2 "
        }
        None => "",
    };
    params.push((limit as i64).into());
    let limit_param = params.len(); // $2 when unfiltered, $3 when filtered
    let sql = format!(
        "SELECT COALESCE(event_id::text, id::text) AS event_key \
         FROM metric_anomalies \
         WHERE workspace_id = $1 {status_clause}\
         GROUP BY 1 \
         ORDER BY \
           MAX(CASE WHEN status <> 'dismissed' THEN 1 ELSE 0 END) DESC, \
           MAX({rank}) DESC, \
           MAX(detected_at) DESC, \
           event_key \
         LIMIT ${limit_param}",
        rank = monitoring::detect::severity_rank_case_sql(),
    );
    let rows = db
        .query_all(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            params,
        ))
        .await?;
    rows.into_iter()
        .map(|r| r.try_get::<String>("", "event_key"))
        .collect::<Result<Vec<_>, _>>()
        .map_err(AnomalyError::Db)
}

/// Fetch every row of the given events, ordered to match the phase-1 event rank
/// (then `period_start` within an event). Keys are UUID strings: a key is an
/// `event_id` for grouped rows or a row `id` for pre-event ones, so match on
/// either — `event_id IN keys` never matches a `NULL`, and `id IN keys` picks up
/// the ungrouped rows.
async fn load_ranked_events(
    db: &sea_orm::DatabaseConnection,
    workspace_id: Uuid,
    status: Option<&str>,
    keys: &[String],
) -> Result<Vec<metric_anomalies::Model>, AnomalyError> {
    let uuids: Vec<Uuid> = keys
        .iter()
        .filter_map(|k| Uuid::parse_str(k).ok())
        .collect();
    let mut query = AnomaliesEntity::find()
        .filter(metric_anomalies::Column::WorkspaceId.eq(workspace_id))
        .filter(
            sea_orm::Condition::any()
                .add(metric_anomalies::Column::EventId.is_in(uuids.clone()))
                .add(metric_anomalies::Column::Id.is_in(uuids)),
        );
    if let Some(status) = status {
        query = query.filter(metric_anomalies::Column::Status.eq(status));
    }
    let mut rows = query.all(db).await?;

    // Order by phase-1 event rank, then oldest-first within an event. The rank
    // is authoritative; anything not in it (a scan landing between phase 1 and
    // phase 2 can add a row to a ranked event, or make one return no rows —
    // self-correcting on the next load) sorts last rather than panicking.
    // `sort_by_cached_key` computes the key once per row, not once per compare,
    // so `event_key_of`'s allocation happens n times, not n·log n.
    let rank: std::collections::HashMap<&str, usize> = keys
        .iter()
        .enumerate()
        .map(|(i, k)| (k.as_str(), i))
        .collect();
    rows.sort_by_cached_key(|m| {
        let r = rank
            .get(event_key_of(m).as_str())
            .copied()
            .unwrap_or(usize::MAX);
        (r, m.period_start)
    });

    // Defensive payload cap: the page bounds events, and `metric_anomalies` has
    // no retention, so a pathological long-running regime shift is one event
    // with an unbounded bucket count. Cut at an *event boundary* — never
    // mid-event, or we'd hand the frontend a partial event and reopen the very
    // Ack/undercount bug the two-phase query closed — keeping at least the first
    // event so a single pathological chain still renders.
    let cap = keys.len().saturating_mul(MAX_BUCKETS_PER_EVENT);
    if rows.len() > cap {
        let row_keys: Vec<String> = rows.iter().map(event_key_of).collect();
        let refs: Vec<&str> = row_keys.iter().map(String::as_str).collect();
        rows.truncate(event_boundary(&refs, cap));
    }
    Ok(rows)
}

/// The largest prefix length `<= cap` that ends on an event boundary, where
/// `keys[i]` is row `i`'s event key and rows are grouped by event (contiguous,
/// modulo unranked rows a concurrent scan may interleave — a vanishingly rare,
/// self-correcting case). Always at least the whole first event, so a single
/// event larger than `cap` still renders rather than returning nothing.
fn event_boundary(keys: &[&str], cap: usize) -> usize {
    if keys.len() <= cap {
        return keys.len();
    }
    // Walk back from the cut index to where the straddled event began.
    let straddled = keys[cap];
    let mut end = cap;
    while end > 0 && keys[end - 1] == straddled {
        end -= 1;
    }
    if end > 0 {
        return end;
    }
    // The cut fell inside the first event: keep that whole event.
    let first = keys[0];
    keys.iter().take_while(|k| **k == first).count()
}

/// Safety-valve ceiling on buckets returned per requested event (see
/// [`load_ranked_events`]). Generous — real events span a handful of buckets;
/// this only guards against an unbounded chain, not normal sizing.
const MAX_BUCKETS_PER_EVENT: usize = 50;

// ── Status updates: acknowledge / dismiss ────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct UpdateStatusRequest {
    pub status: String,
}

/// `POST /workspaces/{workspace_id}/semantic/anomalies/{id}/status`
/// with body `{"status": "acknowledged" | "dismissed" | "new"}`.
pub async fn update_status(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    _role: EffectiveWorkspaceRole,
    Extension(state): Extension<Arc<AgenticState>>,
    Path((_workspace_id, anomaly_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateStatusRequest>,
) -> Result<Json<metric_anomalies::Model>, AnomalyError> {
    if !matches!(req.status.as_str(), "new" | "acknowledged" | "dismissed") {
        return Err(AnomalyError::BadStatus(req.status));
    }
    let workspace_id = workspace_manager.workspace_id;
    let existing = AnomaliesEntity::find_by_id(anomaly_id)
        .filter(metric_anomalies::Column::WorkspaceId.eq(workspace_id))
        .one(&state.db)
        .await?
        .ok_or(AnomalyError::NotFound)?;
    let mut active = existing.into_active_model();
    active.status = Set(req.status);
    active.updated_at = Set(Utc::now().into());
    let updated = active.update(&state.db).await?;
    Ok(Json(updated))
}

// ── Scan trigger ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ScanResponse {
    pub monitors_scanned: usize,
    pub monitors_failed: usize,
    pub anomalies_persisted: usize,
    /// `true` when the scan was still running at response time. The scan
    /// continues in the background and results will appear in subsequent
    /// list calls. Clients should refetch anomalies after a short delay.
    #[serde(default)]
    pub pending: bool,
    /// One entry per monitor that errored, so the UI can tell the user
    /// *which* monitor failed and *why* rather than just a count. Empty on a
    /// clean scan and (necessarily) on the `pending` path, where the scan is
    /// still running and failures aren't known yet.
    #[serde(default)]
    pub failures: Vec<ScanFailureDetail>,
}

/// A single failed monitor, flattened for the wire. Mirrors the fields the
/// inbox needs to name the monitor + segment and show the error message.
#[derive(Debug, Serialize)]
pub struct ScanFailureDetail {
    pub measure: String,
    pub time_dimension: String,
    pub granularity: String,
    pub label: Option<String>,
    /// Segment key when the failed monitor was a `group_by`/filtered segment
    /// (e.g. `"labor_daily.restaurant_id=loc-abc"`); empty for chain-wide.
    pub dimension_key: String,
    /// Raw filters identifying the segment, mirroring the persisted anomaly's
    /// `filters` so the inbox can render the failure with the same friendly
    /// segment label it uses for anomaly rows. `None` for chain-wide monitors.
    pub filters: Option<serde_json::Value>,
    /// Human-readable error (the `ScanError` `Display` chain).
    pub error: String,
}

fn to_failure_detail(f: &monitoring::service::MonitorFailure) -> ScanFailureDetail {
    let filters = if f.entry.filters.is_empty() {
        None
    } else {
        serde_json::to_value(&f.entry.filters).ok()
    };
    ScanFailureDetail {
        measure: f.entry.measure.clone(),
        time_dimension: f.entry.time_dimension.clone(),
        granularity: f.entry.granularity.airlayer_str().to_string(),
        label: f.entry.label.clone(),
        dimension_key: monitoring::config::MonitorFilter::key_for(&f.entry.filters),
        filters,
        error: f.error.to_string(),
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct ScanQuery {
    /// Override the reference date the scan treats as "now" (YYYY-MM-DD).
    /// Lets demos / tests target a period when warehouse data actually
    /// exists. Omitted = real `Utc::now()`.
    pub as_of: Option<String>,
}

/// `POST /workspaces/{workspace_id}/semantic/anomalies/scan` — run every
/// `.monitor.yml` entry once and upsert new anomalies into the inbox.
///
/// The scan runs inside a detached `tokio::spawn` task so it isn't cancelled
/// when the global 60-second request timeout fires. If the scan completes
/// within 55 seconds the full result is returned synchronously. If it takes
/// longer, the endpoint returns immediately with `pending: true` and the
/// task continues to completion in the background — the anomalies list will
/// reflect the new results on the next fetch.
pub async fn run_scan(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    EffectiveWorkspaceRole(role): EffectiveWorkspaceRole,
    Extension(state): Extension<Arc<AgenticState>>,
    Query(q): Query<ScanQuery>,
) -> Result<Json<ScanResponse>, AnomalyError> {
    let workspace_id = workspace_manager.workspace_id;

    // Debounce: if a scan completed or started within the last 60 s, return
    // immediately with `pending: true` rather than spawning a second parallel
    // scan over the same `.monitor.yml`. Reuses the same global registry that
    // the periodic cron tick uses so the debounce window is shared.
    if !monitoring::global_registry()
        .take_if_due(workspace_id, Duration::from_secs(60))
        .await
    {
        return Ok(Json(ScanResponse {
            monitors_scanned: 0,
            monitors_failed: 0,
            anomalies_persisted: 0,
            pending: true,
            failures: Vec::new(),
        }));
    }

    let runner = Arc::new(OxyMetricTreeRunner::new(
        workspace_manager.clone(),
        user.id,
        role,
    ));
    // Compile-boundary fast path. When the workspace is promoted +
    // flag is on, we materialise `.monitor.yml` into a tempdir from
    // Postgres and pass THAT path to scan_workspace. The tempdir is
    // owned by the spawned task below so it lives for the whole scan.
    let materialised_monitor = match crate::server::api::semantic_scan::materialise_monitor_config(
        workspace_id,
    )
    .await
    {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(error = ?e, "run_scan: monitor materialise failed; falling through to FS");
            None
        }
    };
    let workspace_root = workspace_manager.config_manager.workspace_path();
    let fs_config_path = monitoring::default_config_path(workspace_root);
    let config_path = materialised_monitor
        .as_ref()
        .map(|m| m.config_path.clone())
        .unwrap_or(fs_config_path);
    let now = parse_as_of(q.as_of.as_deref())?;
    let db = state.db.clone();

    // Create a run row so the scan appears in the coordinator.
    let run_id = Uuid::new_v4().to_string();
    let meta = serde_json::json!({ "trigger": "manual" });
    if let Err(e) = insert_run(
        &db,
        &run_id,
        "Anomaly scan",
        None,
        "monitor_scan",
        Some(meta),
        workspace_id,
    )
    .await
    {
        tracing::warn!(error = %e, "run_scan: failed to create run row; scan will proceed without coordinator tracking");
    }

    // Spawn the scan. The spawned task always finalises the run status so the
    // coordinator shows done/failed whether the 55-second wait fires or not.
    let (tx, rx) =
        tokio::sync::oneshot::channel::<Result<(monitoring::ScanResult, usize), AnomalyError>>();
    let db_bg = db.clone();
    let run_id_bg = run_id.clone();
    // Move the tempdir handle into the spawned task so it lives as
    // long as the scan does. Drop at task end cleans up the file.
    let _materialised_monitor_guard = materialised_monitor;
    tokio::spawn(async move {
        let _hold = _materialised_monitor_guard;
        let outcome = async {
            // Events already on record, so a bucket continuing a reported
            // slide is not made to re-clear its seasonal envelope alone.
            let open_events = monitoring::load_open_events(&db_bg, workspace_id)
                .await
                .map_err(AnomalyError::Db)?;
            let result = monitoring::scan_workspace(runner, &config_path, now, None, &open_events)
                .await
                .map_err(AnomalyError::Scan)?;
            let persisted = monitoring::persist_scan(&db_bg, workspace_id, &result)
                .await
                .map_err(AnomalyError::Db)?;
            Ok::<(monitoring::ScanResult, usize), AnomalyError>((result, persisted))
        }
        .await;

        let summary = match &outcome {
            Ok((result, persisted)) => format!(
                "scanned={} failed={} persisted={}",
                result.outcomes.len(),
                result.failures.len(),
                persisted
            ),
            Err(e) => format!("{e:?}"),
        };
        let has_failures = matches!(&outcome, Ok((r, _)) if !r.failures.is_empty());
        if outcome.is_err() || has_failures {
            let _ = update_run_failed(&db_bg, &run_id_bg, &summary).await;
        } else {
            let _ = update_run_done(&db_bg, &run_id_bg, &summary, None).await;
        }
        let _ = tx.send(outcome);
    });

    // Wait up to 55 s — safely inside the 60 s global timeout. If the scan
    // finishes in time, return the full tally. If it's still running, return
    // `pending: true`; the spawned task continues and updates the run row.
    match tokio::time::timeout(Duration::from_secs(55), rx).await {
        Ok(Ok(Ok((result, persisted)))) => Ok(Json(ScanResponse {
            monitors_scanned: result.outcomes.len(),
            monitors_failed: result.failures.len(),
            anomalies_persisted: persisted,
            pending: false,
            failures: result.failures.iter().map(to_failure_detail).collect(),
        })),
        Ok(Ok(Err(e))) => Err(e),
        Ok(Err(_)) => Err(AnomalyError::Internal("scan task dropped".into())),
        Err(_elapsed) => {
            tracing::info!(
                target: "metric_anomalies",
                workspace_id = %workspace_id,
                run_id = %run_id,
                "scan still running after 55 s — returning pending response"
            );
            // Run row stays "running"; the background task updates it on finish.
            Ok(Json(ScanResponse {
                monitors_scanned: 0,
                monitors_failed: 0,
                anomalies_persisted: 0,
                pending: true,
                failures: Vec::new(),
            }))
        }
    }
}

/// Parse the optional `as_of` query string. Accepts `YYYY-MM-DD`. Returns
/// real `Utc::now()` when absent so the cron tick path stays unchanged.
fn parse_as_of(input: Option<&str>) -> Result<chrono::DateTime<Utc>, AnomalyError> {
    let Some(raw) = input else {
        return Ok(Utc::now());
    };
    chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .map(|d| Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap()))
        .map_err(|e| AnomalyError::BadStatus(format!("invalid as_of '{raw}': {e}")))
}

// ── Per-anomaly explain (cached) ────────────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
pub struct ExplainAnomalyQuery {
    /// Force a fresh computation even if the row already has a cached result.
    #[serde(default)]
    pub refresh: bool,
}

/// `POST /workspaces/{workspace_id}/semantic/anomalies/{id}/explain` — run
/// (or return cached) airlayer explain for this anomaly bucket.
///
/// First call computes and writes the JSON result onto the row. Subsequent
/// calls return the cached payload instantly so the Insights drawer
/// survives page refreshes without re-running the 20-30s recursive search.
/// `?refresh=true` busts the cache for the rescan affordance in the UI.
pub async fn explain_anomaly(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    EffectiveWorkspaceRole(role): EffectiveWorkspaceRole,
    Extension(state): Extension<Arc<AgenticState>>,
    Path((_workspace_id, anomaly_id)): Path<(Uuid, Uuid)>,
    Query(q): Query<ExplainAnomalyQuery>,
) -> Result<Json<serde_json::Value>, AnomalyError> {
    let row = AnomaliesEntity::find_by_id(anomaly_id)
        .filter(metric_anomalies::Column::WorkspaceId.eq(workspace_manager.workspace_id))
        .one(&state.db)
        .await?
        .ok_or(AnomalyError::NotFound)?;

    // Cheap path: hand back the cache.
    if !q.refresh
        && let Some(cached) = &row.explain_cache
    {
        return Ok(Json(cached.clone()));
    }

    // Build the explain request from the anomaly's fields. Current = the
    // anomaly bucket; previous = the same phase one seasonal cycle earlier,
    // derived from the monitor's persisted seasonality (falling back to the
    // granularity default for pre-column rows). Aligning to the seasonal
    // period — not a hardcoded offset — is what keeps a daily/weekly-seasonal
    // anomaly compared against the same weekday rather than the weekend.
    let current = row.period_start.date_naive();
    let periods = agentic_analytics::anomaly_period::resolve_seasonal_period(
        row.seasonal_period,
        &row.granularity,
    );
    let previous =
        agentic_analytics::anomaly_period::shift_date_back(current, &row.granularity, periods);
    let current_str = current.format("%Y-%m-%d").to_string();
    let previous_str = previous.format("%Y-%m-%d").to_string();

    // Scope the explain to the anomaly's segment (e.g. the specific restaurant
    // a `group_by` monitor flagged) so the totals and decomposition reflect
    // that segment, not the chain-wide aggregate.
    let filters = agentic_analytics::anomaly_store::segment_query_filters(&row.filters);

    use agentic_analytics::MetricTreeRunner as _;
    let runner = OxyMetricTreeRunner::new(workspace_manager, user.id, role);
    let mut config = airlayer::engine::metric_tree_ops::ExplainConfig::default();
    config.deep = false;

    let result = runner
        .run_explain(
            row.measure.clone(),
            row.time_dimension.clone(),
            (current_str.clone(), current_str),
            (previous_str.clone(), previous_str),
            filters,
            config,
        )
        .await
        .map_err(|e| AnomalyError::Scan(monitoring::ScanError::FetchSeries(e)))?;

    let value = serde_json::to_value(&result)
        .map_err(|e| AnomalyError::BadStatus(format!("serialize explain: {e}")))?;

    // Persist cache. Failures here are non-fatal — the user gets the
    // result; next click will just recompute.
    let mut active = row.into_active_model();
    active.explain_cache = Set(Some(value.clone()));
    active.explain_cached_at = Set(Some(Utc::now().into()));
    if let Err(e) = active.update(&state.db).await {
        tracing::warn!(
            target: "metric_anomalies",
            anomaly_id = %anomaly_id,
            error = %e,
            "failed to persist explain cache"
        );
    }

    Ok(Json(value))
}

#[cfg(test)]
mod tests {
    use super::event_boundary;

    #[test]
    fn boundary_returns_all_when_under_cap() {
        let keys = ["a", "a", "b"];
        assert_eq!(event_boundary(&keys, 10), 3);
    }

    #[test]
    fn boundary_cuts_between_events_never_mid_event() {
        // cap 3 lands inside event "b" — drop "b" entirely and keep "a".
        let keys = ["a", "a", "b", "b", "b", "c"];
        // cap 3 → keys[3] == "b", walk back to index 2 → keep ["a","a"].
        assert_eq!(event_boundary(&keys, 3), 2);
        // cap 5 → keys[5] == "c", walk back to 5 → keep a,a,b,b,b.
        assert_eq!(event_boundary(&keys, 5), 5);
    }

    #[test]
    fn boundary_keeps_the_whole_first_event_when_it_alone_exceeds_cap() {
        let keys = ["a", "a", "a", "a", "b"];
        // cap 2 falls inside the first event; keep the whole first event.
        assert_eq!(event_boundary(&keys, 2), 4);
    }
}
