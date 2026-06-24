//! `/api/admin/internal-jobs/*` — operator dashboard for the agentic task
//! queue and worker fleet. Distinct from the customer-facing
//! Coordinator/Orchestrator UI which surfaces individual analytics runs;
//! this module shows the meta view (workers, queue depth, dead-letter,
//! periodic system jobs).
//!
//! `OXY_OWNER` enforcement happens at the router layer via
//! `oxy_owner_guard_middleware`; handlers here assume the caller has
//! already been allow-listed.
//!
//! Database access is on-demand via `oxy::database::client::establish_connection()`
//! — same pattern `billing_service()` uses. We deliberately do not put a
//! pool on `AppState` for this admin-only surface.

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use chrono::{DateTime, FixedOffset};
use sea_orm::{
    ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, FromQueryResult, Statement,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::server::router::AppState;

/// Mount internal-jobs routes under `/admin`. Caller (see `admin::router`)
/// Routes are flat (no `/internal-jobs/` prefix) — `router::global` nests
/// the whole thing under `/admin/internal-jobs` so that the more permissive
/// `oxy_owner_or_app_admin_guard` can be applied without bringing the rest
/// of `/admin/*` along.
pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/queue-stats", get(queue_stats))
        .route("/recent-failures", get(recent_failures))
        .route("/dead-letter", get(list_dead_letter))
        .route("/dead-letter/{task_id}/reenqueue", post(reenqueue_dead))
        .route("/dead-letter/{task_id}", delete(delete_dead))
        .route("/workers", get(list_workers))
        .route("/scheduled", get(list_scheduled))
        .route("/run-reaper", post(run_reaper))
        .route("/run-retention", post(run_retention))
}

// ---------------------------------------------------------------------------
// Connect-on-demand helper
// ---------------------------------------------------------------------------

pub(crate) async fn connect() -> Result<DatabaseConnection, Response> {
    oxy::database::client::establish_connection()
        .await
        .map_err(|e| {
            tracing::error!(?e, "internal-jobs: DB connect failed");
            error_body(
                StatusCode::SERVICE_UNAVAILABLE,
                "db_unavailable",
                Some("Database connection failed".into()),
            )
        })
}

// ---------------------------------------------------------------------------
// Queue stats
// ---------------------------------------------------------------------------

#[derive(Serialize, Default, Debug, PartialEq)]
pub struct QueueStatusCounts {
    pub queued: i64,
    pub claimed: i64,
    pub completed: i64,
    pub failed: i64,
    pub cancelled: i64,
    pub dead: i64,
}

#[derive(Serialize, Debug)]
pub struct QueueStatsResponse {
    pub last_1h: QueueStatusCounts,
    pub last_24h: QueueStatusCounts,
    pub total: QueueStatusCounts,
}

#[derive(Debug, FromQueryResult)]
struct StatusBucketRow {
    bucket: String,
    queue_status: String,
    cnt: i64,
}

async fn queue_stats() -> Result<Json<QueueStatsResponse>, Response> {
    let db = connect().await?;
    let stats = fetch_queue_stats(&db).await.map_err(db_err)?;
    Ok(Json(stats))
}

pub(crate) async fn fetch_queue_stats(
    db: &DatabaseConnection,
) -> Result<QueueStatsResponse, DbErr> {
    // Single grouped query — emits one row per (bucket, status) pair. The
    // CASE expression bucketizes by updated_at age; the WHERE clause keeps
    // total a strict superset of the others.
    let sql = "\
        SELECT bucket, queue_status, COUNT(*) AS cnt FROM ( \
            SELECT \
                CASE \
                    WHEN updated_at > now() - interval '1 hour' THEN '1h' \
                    WHEN updated_at > now() - interval '24 hours' THEN '24h' \
                    ELSE 'older' \
                END AS bucket, \
                queue_status \
            FROM agentic_task_queue \
        ) t \
        GROUP BY bucket, queue_status";

    let rows = StatusBucketRow::find_by_statement(Statement::from_string(
        DatabaseBackend::Postgres,
        sql.to_string(),
    ))
    .all(db)
    .await?;

    let mut out = QueueStatsResponse {
        last_1h: QueueStatusCounts::default(),
        last_24h: QueueStatusCounts::default(),
        total: QueueStatusCounts::default(),
    };
    for row in rows {
        // total always increments
        accumulate(&mut out.total, &row.queue_status, row.cnt);
        if row.bucket == "1h" {
            accumulate(&mut out.last_1h, &row.queue_status, row.cnt);
            accumulate(&mut out.last_24h, &row.queue_status, row.cnt);
        } else if row.bucket == "24h" {
            accumulate(&mut out.last_24h, &row.queue_status, row.cnt);
        }
    }
    Ok(out)
}

fn accumulate(counts: &mut QueueStatusCounts, status: &str, n: i64) {
    match status {
        "queued" => counts.queued += n,
        "claimed" => counts.claimed += n,
        "completed" => counts.completed += n,
        "failed" => counts.failed += n,
        "cancelled" => counts.cancelled += n,
        "dead" => counts.dead += n,
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Recent failures
// ---------------------------------------------------------------------------

/// A failed/dead job, enriched with the tenant + run context an operator needs
/// to actually debug it. The enriched fields are LEFT-joined from
/// `agentic_runs → workspaces → organizations` (+ `threads → users`), so they
/// are `Option` — system jobs or orphaned runs render with nulls rather than
/// being dropped.
#[derive(Serialize, Debug)]
pub struct QueueRowDto {
    pub task_id: String,
    pub run_id: String,
    pub queue_status: String,
    pub worker_id: Option<String>,
    pub claim_count: i32,
    pub max_claims: i32,
    pub last_heartbeat: Option<DateTime<FixedOffset>>,
    pub claimed_at: Option<DateTime<FixedOffset>>,
    pub created_at: DateTime<FixedOffset>,
    pub updated_at: DateTime<FixedOffset>,
    pub task_type: Option<String>,
    /// Decoded `TaskSpec` JSON so the UI can show agent_id / workflow_ref /
    /// question / variables without a second round-trip.
    pub spec: serde_json::Value,
    // --- enriched tenant + run context (LEFT-joined) ---
    pub workspace_id: Option<Uuid>,
    pub workspace_name: Option<String>,
    pub org_id: Option<Uuid>,
    pub org_name: Option<String>,
    pub run_status: Option<String>,
    pub run_error_message: Option<String>,
    pub originating_user_email: Option<String>,
}

/// Basic queue row (no joins) — used by the single-row re-fetch after
/// re-enqueue, where tenant context is not needed.
#[derive(Debug, FromQueryResult)]
struct QueueRowRaw {
    task_id: String,
    run_id: String,
    queue_status: String,
    worker_id: Option<String>,
    claim_count: i32,
    max_claims: i32,
    last_heartbeat: Option<DateTime<FixedOffset>>,
    claimed_at: Option<DateTime<FixedOffset>>,
    created_at: DateTime<FixedOffset>,
    updated_at: DateTime<FixedOffset>,
    spec: serde_json::Value,
}

impl From<QueueRowRaw> for QueueRowDto {
    fn from(r: QueueRowRaw) -> Self {
        let task_type = extract_task_type(&r.spec);
        Self {
            task_id: r.task_id,
            run_id: r.run_id,
            queue_status: r.queue_status,
            worker_id: r.worker_id,
            claim_count: r.claim_count,
            max_claims: r.max_claims,
            last_heartbeat: r.last_heartbeat,
            claimed_at: r.claimed_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
            task_type,
            spec: redact_secrets(r.spec),
            workspace_id: None,
            workspace_name: None,
            org_id: None,
            org_name: None,
            run_status: None,
            run_error_message: None,
            originating_user_email: None,
        }
    }
}

/// Enriched queue row — basic columns plus the joined tenant/run context.
#[derive(Debug, FromQueryResult)]
struct EnrichedQueueRowRaw {
    task_id: String,
    run_id: String,
    queue_status: String,
    worker_id: Option<String>,
    claim_count: i32,
    max_claims: i32,
    last_heartbeat: Option<DateTime<FixedOffset>>,
    claimed_at: Option<DateTime<FixedOffset>>,
    created_at: DateTime<FixedOffset>,
    updated_at: DateTime<FixedOffset>,
    spec: serde_json::Value,
    workspace_id: Option<Uuid>,
    workspace_name: Option<String>,
    org_id: Option<Uuid>,
    org_name: Option<String>,
    run_status: Option<String>,
    run_error_message: Option<String>,
    originating_user_email: Option<String>,
}

impl From<EnrichedQueueRowRaw> for QueueRowDto {
    fn from(r: EnrichedQueueRowRaw) -> Self {
        let task_type = extract_task_type(&r.spec);
        Self {
            task_id: r.task_id,
            run_id: r.run_id,
            queue_status: r.queue_status,
            worker_id: r.worker_id,
            claim_count: r.claim_count,
            max_claims: r.max_claims,
            last_heartbeat: r.last_heartbeat,
            claimed_at: r.claimed_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
            task_type,
            spec: redact_secrets(r.spec),
            workspace_id: r.workspace_id,
            workspace_name: r.workspace_name,
            org_id: r.org_id,
            org_name: r.org_name,
            run_status: r.run_status,
            run_error_message: r.run_error_message,
            originating_user_email: r.originating_user_email,
        }
    }
}

/// Shared SELECT + JOIN prefix for the enriched failure/dead-letter feeds.
/// Callers append a `WHERE` clause, `ORDER BY`, and `LIMIT`/`OFFSET`.
/// All joins are LEFT joins so a job whose run/workspace/org is missing still
/// appears (with null context) instead of silently vanishing.
const ENRICHED_SELECT: &str = "\
    SELECT q.task_id, q.run_id, q.queue_status, q.worker_id, q.claim_count, \
           q.max_claims, q.last_heartbeat, q.claimed_at, q.created_at, \
           q.updated_at, q.spec, \
           r.task_status AS run_status, r.error_message AS run_error_message, \
           r.workspace_id AS workspace_id, \
           w.name AS workspace_name, w.org_id AS org_id, \
           o.name AS org_name, u.email AS originating_user_email \
    FROM agentic_task_queue q \
    LEFT JOIN agentic_runs r ON q.run_id = r.id \
    LEFT JOIN workspaces w ON r.workspace_id = w.id \
    LEFT JOIN organizations o ON w.org_id = o.id \
    LEFT JOIN threads t ON r.thread_id = t.id \
    LEFT JOIN users u ON t.user_id = u.id ";

/// Best-effort extraction of a "task type" tag from the serialized
/// `TaskSpec` JSON for display in the UI. The spec is a tagged enum so we
/// peek at the top-level keys; if the shape changes, fall back to None
/// (the column is purely informational).
/// Defense-in-depth: blank out obviously secret-shaped values before the
/// decoded `TaskSpec` is sent to the admin debug panel. Credentials are
/// supposed to live in the secret manager, but a misconfigured airway/automation
/// pipeline could inline a token into `variables`/`payload`; this keeps it from
/// surfacing verbatim in the UI. Operator-only guard already bounds exposure —
/// this is the belt to that guard's braces.
fn redact_secrets(mut spec: serde_json::Value) -> serde_json::Value {
    redact_in_place(&mut spec);
    spec
}

fn redact_in_place(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                if is_secret_key(k) {
                    *val = serde_json::Value::String("***redacted***".to_string());
                } else {
                    redact_in_place(val);
                }
            }
        }
        serde_json::Value::Array(arr) => arr.iter_mut().for_each(redact_in_place),
        _ => {}
    }
}

fn is_secret_key(key: &str) -> bool {
    const NEEDLES: &[&str] = &[
        "password",
        "passwd",
        "secret",
        "token",
        "credential",
        "api_key",
        "apikey",
        "access_key",
        "private_key",
        "authorization",
    ];
    let k = key.to_lowercase();
    NEEDLES.iter().any(|n| k.contains(n))
}

fn extract_task_type(spec: &serde_json::Value) -> Option<String> {
    if let Some(obj) = spec.as_object() {
        // `TaskSpec` is internally tagged (`#[serde(tag = "type")]`), so the
        // variant name lives under the `type` key (e.g. `"agent"`,
        // `"workflow"`, `"airway"`). Fall back to the first key for any
        // legacy/externally-tagged shape.
        if let Some(t) = obj.get("type").and_then(|v| v.as_str()) {
            return Some(t.to_string());
        }
        if let Some((k, _)) = obj.iter().next() {
            return Some(k.clone());
        }
    }
    None
}

#[derive(Deserialize)]
struct LimitQuery {
    limit: Option<u64>,
}

async fn recent_failures(Query(q): Query<LimitQuery>) -> Result<Json<Vec<QueueRowDto>>, Response> {
    let db = connect().await?;
    let limit = q.limit.unwrap_or(50).clamp(1, 500) as i64;
    let sql = format!(
        "{ENRICHED_SELECT} \
         WHERE q.queue_status IN ('failed', 'dead') \
         ORDER BY q.updated_at DESC \
         LIMIT $1"
    );
    let rows = EnrichedQueueRowRaw::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [limit.into()],
    ))
    .all(&db)
    .await
    .map_err(db_err)?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

// ---------------------------------------------------------------------------
// Dead-letter list + actions
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct DeadLetterQuery {
    limit: Option<u64>,
    offset: Option<u64>,
}

#[derive(Serialize, Debug)]
pub struct DeadLetterResponse {
    pub rows: Vec<QueueRowDto>,
    pub total: i64,
}

#[derive(Debug, FromQueryResult)]
struct CountRow {
    cnt: i64,
}

async fn list_dead_letter(
    Query(q): Query<DeadLetterQuery>,
) -> Result<Json<DeadLetterResponse>, Response> {
    let db = connect().await?;
    let limit = q.limit.unwrap_or(50).clamp(1, 500) as i64;
    let offset = q.offset.unwrap_or(0) as i64;

    let sql = format!(
        "{ENRICHED_SELECT} \
         WHERE q.queue_status = 'dead' \
         ORDER BY q.updated_at DESC \
         LIMIT $1 OFFSET $2"
    );
    let rows = EnrichedQueueRowRaw::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [limit.into(), offset.into()],
    ))
    .all(&db)
    .await
    .map_err(db_err)?;

    let total = CountRow::find_by_statement(Statement::from_string(
        DatabaseBackend::Postgres,
        "SELECT COUNT(*) AS cnt FROM agentic_task_queue WHERE queue_status = 'dead'".to_string(),
    ))
    .one(&db)
    .await
    .map_err(db_err)?
    .map(|r| r.cnt)
    .unwrap_or(0);

    Ok(Json(DeadLetterResponse {
        rows: rows.into_iter().map(Into::into).collect(),
        total,
    }))
}

#[derive(Debug, FromQueryResult)]
struct StatusOnlyRow {
    queue_status: String,
}

/// Look up the current queue_status for a given task_id. Returns:
/// - `Ok(Some(status))` — row exists
/// - `Ok(None)` — no row
/// - `Err(_)` — DB error
async fn current_status(db: &DatabaseConnection, task_id: &str) -> Result<Option<String>, DbErr> {
    StatusOnlyRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "SELECT queue_status FROM agentic_task_queue WHERE task_id = $1",
        [task_id.into()],
    ))
    .one(db)
    .await
    .map(|opt| opt.map(|r| r.queue_status))
}

async fn reenqueue_dead(Path(task_id): Path<String>) -> Result<Json<QueueRowDto>, Response> {
    let db = connect().await?;
    let status = current_status(&db, &task_id).await.map_err(db_err)?;
    match status.as_deref() {
        None => Err(not_found_row()),
        Some("dead") => {
            // TOCTOU guard: the SELECT above is advisory — the reaper or another
            // admin could mutate the row before our UPDATE lands. The
            // `WHERE queue_status = 'dead'` clause prevents the wrong write,
            // but we must inspect rows_affected to detect the race and
            // surface a 409 rather than a misleading 200.
            let result = db
                .execute(Statement::from_sql_and_values(
                    DatabaseBackend::Postgres,
                    "UPDATE agentic_task_queue \
                     SET queue_status = 'queued', \
                         claim_count = 0, \
                         worker_id = NULL, \
                         claimed_at = NULL, \
                         last_heartbeat = NULL, \
                         updated_at = now() \
                     WHERE task_id = $1 AND queue_status = 'dead'",
                    [task_id.clone().into()],
                ))
                .await
                .map_err(db_err)?;

            if result.rows_affected() == 0 {
                return Err(error_body(
                    StatusCode::CONFLICT,
                    "not_dead",
                    Some(format!(
                        "Task '{task_id}' was not in queue_status='dead' at write time; \
                         another process likely moved it. Re-fetch and try again."
                    )),
                ));
            }

            let row = QueueRowRaw::find_by_statement(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "SELECT task_id, run_id, queue_status, worker_id, claim_count, max_claims, \
                        last_heartbeat, claimed_at, created_at, updated_at, spec \
                 FROM agentic_task_queue WHERE task_id = $1",
                [task_id.into()],
            ))
            .one(&db)
            .await
            .map_err(db_err)?;
            row.map(|r| Json(r.into())).ok_or_else(not_found_row)
        }
        Some(other) => Err(error_body(
            StatusCode::CONFLICT,
            "not_dead",
            Some(format!(
                "Task is in queue_status='{other}', only 'dead' rows can be re-enqueued."
            )),
        )),
    }
}

async fn delete_dead(Path(task_id): Path<String>) -> Result<StatusCode, Response> {
    let db = connect().await?;
    let status = current_status(&db, &task_id).await.map_err(db_err)?;
    match status.as_deref() {
        None => Err(not_found_row()),
        Some("dead") => {
            // Same TOCTOU guard as reenqueue_dead.
            let result = db
                .execute(Statement::from_sql_and_values(
                    DatabaseBackend::Postgres,
                    "DELETE FROM agentic_task_queue WHERE task_id = $1 AND queue_status = 'dead'",
                    [task_id.clone().into()],
                ))
                .await
                .map_err(db_err)?;

            if result.rows_affected() == 0 {
                return Err(error_body(
                    StatusCode::CONFLICT,
                    "not_dead",
                    Some(format!(
                        "Task '{task_id}' was not in queue_status='dead' at write time; \
                         another process likely moved it. Re-fetch and try again."
                    )),
                ));
            }

            Ok(StatusCode::NO_CONTENT)
        }
        Some(other) => Err(error_body(
            StatusCode::CONFLICT,
            "not_dead",
            Some(format!(
                "Task is in queue_status='{other}', only 'dead' rows can be deleted."
            )),
        )),
    }
}

// ---------------------------------------------------------------------------
// Worker fleet
// ---------------------------------------------------------------------------

#[derive(Serialize, Debug)]
pub struct WorkerDto {
    pub worker_id: String,
    pub last_claim_at: Option<DateTime<FixedOffset>>,
    pub inflight_count: i64,
}

#[derive(Serialize, Debug)]
pub struct WorkersResponse {
    pub supported: bool,
    pub workers: Vec<WorkerDto>,
}

#[derive(Debug, FromQueryResult)]
struct WorkerRow {
    worker_id: String,
    last_claim_at: Option<DateTime<FixedOffset>>,
    inflight_count: i64,
}

async fn list_workers() -> Result<Json<WorkersResponse>, Response> {
    let db = connect().await?;
    // Aggregate by worker_id over rows the worker has touched in the last
    // 24h or that are still claimed. Inflight = currently `claimed` by them.
    let rows = WorkerRow::find_by_statement(Statement::from_string(
        DatabaseBackend::Postgres,
        "SELECT \
             worker_id, \
             MAX(claimed_at) AS last_claim_at, \
             COUNT(*) FILTER (WHERE queue_status = 'claimed') AS inflight_count \
         FROM agentic_task_queue \
         WHERE worker_id IS NOT NULL \
           AND (queue_status = 'claimed' OR claimed_at > now() - interval '24 hours') \
         GROUP BY worker_id \
         ORDER BY MAX(claimed_at) DESC NULLS LAST"
            .to_string(),
    ))
    .all(&db)
    .await
    .map_err(db_err)?;

    Ok(Json(WorkersResponse {
        supported: true,
        workers: rows
            .into_iter()
            .map(|r| WorkerDto {
                worker_id: r.worker_id,
                last_claim_at: r.last_claim_at,
                inflight_count: r.inflight_count,
            })
            .collect(),
    }))
}

// ---------------------------------------------------------------------------
// Scheduled jobs registry (static for now)
// ---------------------------------------------------------------------------

#[derive(Serialize, Debug)]
pub struct ScheduledJobDto {
    pub name: &'static str,
    pub interval_secs: u64,
    pub description: &'static str,
    pub last_known_run_at: Option<DateTime<FixedOffset>>,
    pub trigger_path: Option<&'static str>,
}

async fn list_scheduled() -> Json<Vec<ScheduledJobDto>> {
    Json(scheduled_jobs())
}

/// Static registry — wiring real `last_known_run_at` is future work; for
/// now this lets the UI surface the periodic loops that exist in the
/// runtime so on-call knows what's supposed to be running.
pub(crate) fn scheduled_jobs() -> Vec<ScheduledJobDto> {
    vec![
        ScheduledJobDto {
            name: "reaper",
            interval_secs: 30,
            description: "Resets stale claims so dead workers' work can be re-claimed (or dead-lettered when claim_count >= max_claims).",
            last_known_run_at: None,
            trigger_path: Some("/api/admin/internal-jobs/run-reaper"),
        },
        ScheduledJobDto {
            name: "matcher_health_probe",
            interval_secs: 60,
            description: "Self-NOTIFYs on the task router's health channel so listeners can observe LISTEN/NOTIFY stalls.",
            last_known_run_at: None,
            trigger_path: None,
        },
        ScheduledJobDto {
            name: "worker_recovery_loop",
            interval_secs: 30,
            description: "Pre-pass reaper tick from the standalone `oxy worker` process (in addition to the in-server reaper).",
            last_known_run_at: None,
            trigger_path: None,
        },
        ScheduledJobDto {
            name: "task_queue_retention",
            // Reflect the live (env-tunable) sweep cadence so the operator sees
            // the real interval, not a hardcoded guess.
            interval_secs: agentic_runtime::background::RetentionConfig::from_env()
                .interval
                .as_secs(),
            description: "Prunes old terminal task-queue rows (completed/cancelled after OXY_TASK_QUEUE_RETENTION_DAYS, failed/dead after OXY_TASK_QUEUE_DEAD_RETENTION_DAYS) so internal-job history doesn't clutter the DB.",
            last_known_run_at: None,
            trigger_path: Some("/api/admin/internal-jobs/run-retention"),
        },
    ]
}

// ---------------------------------------------------------------------------
// Run reaper now
// ---------------------------------------------------------------------------

#[derive(Serialize, Debug)]
pub struct RunReaperResponse {
    pub rows_affected: u64,
}

async fn run_reaper() -> Result<Json<RunReaperResponse>, Response> {
    let db = connect().await?;
    let transport = agentic_runtime::transport::DurableTransport::new(db);
    let rows_affected = transport.run_reaper().await;
    Ok(Json(RunReaperResponse { rows_affected }))
}

// ---------------------------------------------------------------------------
// Run retention prune now
// ---------------------------------------------------------------------------

#[derive(Serialize, Debug)]
pub struct RunRetentionResponse {
    pub rows_deleted: u64,
}

/// Manually sweep old terminal task-queue rows using the same retention
/// windows the periodic loop uses (read from the environment). Lets an
/// operator reclaim space immediately instead of waiting for the next cycle.
async fn run_retention() -> Result<Json<RunRetentionResponse>, Response> {
    let db = connect().await?;
    let cfg = agentic_runtime::background::RetentionConfig::from_env();
    let transport = agentic_runtime::transport::DurableTransport::new(db);
    let rows_deleted = transport
        .run_retention(cfg.completed_ttl, cfg.dead_ttl)
        .await;
    Ok(Json(RunRetentionResponse { rows_deleted }))
}

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

fn error_body(status: StatusCode, code: &'static str, message: Option<String>) -> Response {
    (status, Json(ErrorBody { code, message })).into_response()
}

pub(crate) fn db_err(e: DbErr) -> Response {
    tracing::error!(?e, "internal-jobs: DB error");
    error_body(
        StatusCode::INTERNAL_SERVER_ERROR,
        "db_error",
        Some(e.to_string()),
    )
}

fn not_found_row() -> Response {
    error_body(StatusCode::NOT_FOUND, "task_not_found", None)
}

#[cfg(test)]
#[path = "internal_jobs_tests.rs"]
mod tests;
