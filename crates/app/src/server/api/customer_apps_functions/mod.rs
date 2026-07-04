//! `POST /customer-apps/<org>/<slug>/fn/<name>` — Oxy Functions route
//! invocation.
//!
//! See `internal-docs/2026-06-12-customer-apps-functions-design.md`. This
//! handler covers the **route** invocation mode (§2); schedule and Airway
//! triggers reuse [`crate::server::api::customer_apps_functions::runtime::run`]
//! from their own call sites and are not wired here.

#[cfg(feature = "customer-app-functions")]
pub mod host;
mod result_cache;
#[cfg(feature = "customer-app-functions")]
pub mod runtime;

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
#[cfg(feature = "customer-app-functions")]
use entity::prelude::AppFunctionInvocations;
use entity::prelude::{AppBuilds, AppFunctions};
use entity::{app_function_invocations, app_functions};
use oxy::database::client::establish_connection;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::Deserialize;
use tracing::error;
use uuid::Uuid;

use super::customer_apps_auth::authenticate_and_authorize;
use super::customer_apps_build_store;

// ── Rate limit: 60/min/(user, app, function) by default ────────────────────
// Mirrors the (user, app) `RateBucket` in `customer_apps_activity.rs`
// (design doc §11.6), extended with the function name.
//
// TODO(scaling): this table is per-process, so in the multi-instance worker
// fleet (see oxy-scaling-design) the effective limit is `limit * N` instances,
// not `limit`. Acceptable for the MVP; back this with a shared counter (e.g.
// a Postgres-backed sliding window) before relying on it as a hard cap.

const DEFAULT_RATE_PER_MIN: u64 = 60;
const RATE_BUCKET_TTL: Duration = Duration::from_secs(120);

#[derive(Debug)]
struct RateBucket {
    window_start: Instant,
    count: u64,
}

fn rate_table() -> &'static Mutex<HashMap<(Uuid, Uuid, String), RateBucket>> {
    static TABLE: std::sync::OnceLock<Mutex<HashMap<(Uuid, Uuid, String), RateBucket>>> =
        std::sync::OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn would_exceed_rate(user_id: Uuid, app_id: Uuid, function: &str, limit_per_min: u64) -> bool {
    let now = Instant::now();
    let mut table = rate_table().lock().unwrap();
    table.retain(|_, b| now.duration_since(b.window_start) < RATE_BUCKET_TTL);

    let key = (user_id, app_id, function.to_string());
    let bucket = table.entry(key).or_insert_with(|| RateBucket {
        window_start: now,
        count: 0,
    });
    if now.duration_since(bucket.window_start).as_secs() >= 60 {
        bucket.window_start = now;
        bucket.count = 0;
    }
    bucket.count += 1;
    bucket.count > limit_per_min
}

// ── Manifest entry shape (subset relevant to invocation) ────────────────────

#[derive(Debug, Deserialize, Default)]
struct FunctionManifestEntry {
    #[serde(default)]
    route: Option<bool>,
    #[serde(default, rename = "timeoutSeconds")]
    timeout_seconds: Option<u32>,
    #[serde(default, rename = "rateLimitPerMinute")]
    rate_limit_per_minute: Option<u64>,
    /// Opt-in result caching. Absent → never cached (the safe default for a
    /// side-effectful function). Present with a positive `ttlSeconds` → a route
    /// invocation's result is cached per (build, function, user, body) for that
    /// window. Only declare this for read-only / idempotent functions.
    #[serde(default)]
    cache: Option<CacheSpec>,
    /// Databases this function's `ctx.warehouse.*` writes may target. Absent or
    /// empty → the function may NOT write to any database (fail-closed): a write
    /// is rejected before any connector — and therefore any ephemeral credential
    /// — is built. Read-only functions omit it. This is the §11.3 app-scoped
    /// destination allowlist; without it a function could `ctx.warehouse.exec`
    /// arbitrary DDL/DML against the project's source warehouse.
    #[serde(default)]
    destinations: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Default, Clone)]
struct CacheSpec {
    #[serde(rename = "ttlSeconds")]
    ttl_seconds: Option<u64>,
}

impl FunctionManifestEntry {
    /// The cache TTL if opted in with a positive value, else `None`.
    fn cache_ttl(&self) -> Option<Duration> {
        self.cache
            .as_ref()
            .and_then(|c| c.ttl_seconds)
            .filter(|&s| s > 0)
            // Cap the TTL so a typo can't cache for years; a day is plenty for
            // the "don't recompute per page-load" use case.
            .map(|s| Duration::from_secs(s.min(86_400)))
    }

    /// Database names this function may write to via `ctx.warehouse` (empty →
    /// none; writes fail-closed). The §11.3 destination allowlist.
    fn write_destinations(&self) -> Vec<String> {
        self.destinations.clone().unwrap_or_default()
    }
}

/// Per-mode default `timeoutSeconds` (design doc §11.11). Route invocations
/// default to 10s; the manifest's `timeoutSeconds`, if set, is a ceiling
/// capped at 300 regardless of mode.
const ROUTE_DEFAULT_TIMEOUT_SECS: u64 = 10;
const TIMEOUT_CEILING_SECS: u64 = 300;

fn resolve_timeout(entry: &FunctionManifestEntry) -> Duration {
    let secs = entry
        .timeout_seconds
        .map(u64::from)
        .unwrap_or(ROUTE_DEFAULT_TIMEOUT_SECS)
        .min(TIMEOUT_CEILING_SECS);
    Duration::from_secs(secs)
}

// ── SSE helpers ──────────────────────────────────────────────────────────

fn sse_event(event: &str, data: &serde_json::Value) -> String {
    format!("event: {event}\ndata: {data}\n\n")
}

fn sse_response(body: String) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .body(Body::from(body))
        .unwrap()
        .into_response()
}

/// Frame a successful function result body as the terminal `data` + `done` SSE
/// stream the SDK consumes. The body is emitted as a parsed JSON *value* so the
/// SDK's single `JSON.parse(data)` yields the object (not a double-encoded
/// string); a non-JSON body falls back to a JSON string. Shared by the fresh-run
/// and cache-hit paths so they frame identically.
fn success_sse_body(body_text: &str) -> String {
    let body_value = serde_json::from_str::<serde_json::Value>(body_text)
        .unwrap_or_else(|_| serde_json::Value::String(body_text.to_string()));
    format!(
        "{}{}",
        sse_event("data", &body_value),
        sse_event("done", &serde_json::json!({ "status": 200 }))
    )
}

// ── Idempotency, reaper & invocation-slot acquisition ──────────────────────

/// Seconds after which a still-`running` invocation row is treated as orphaned
/// (an instance crash / thread detach left it stuck) — well beyond the 300s max
/// function timeout + grace.
const REAP_AFTER_SECS: i64 = 900;
/// Reap at most once per this interval per process, so the sweep isn't an
/// UPDATE-scan on every single invocation (it clears orphans, which appear
/// rarely). The design doc's periodic global-driver sweep supersedes this.
const REAP_INTERVAL_SECS: u64 = 60;

/// Process-local throttle so `reap_stuck_invocations` runs at most every
/// [`REAP_INTERVAL_SECS`], not once per request.
fn should_reap_now() -> bool {
    use std::sync::Mutex;
    use std::time::Instant;
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap();
    let now = Instant::now();
    match *last {
        Some(t) if now.duration_since(t).as_secs() < REAP_INTERVAL_SECS => false,
        _ => {
            *last = Some(now);
            true
        }
    }
}

/// Best-effort: transition orphaned `running` rows to `timeout` so the audit
/// trail is truthful and they stop looking in-flight to cancellation/idempotency.
/// Throttled — see [`should_reap_now`].
async fn reap_stuck_invocations(db: &sea_orm::DatabaseConnection) {
    if !should_reap_now() {
        return;
    }
    use sea_orm::sea_query::Expr;
    let cutoff: chrono::DateTime<chrono::FixedOffset> =
        (chrono::Utc::now() - chrono::Duration::seconds(REAP_AFTER_SECS)).into();
    if let Err(e) = app_function_invocations::Entity::update_many()
        .col_expr(
            app_function_invocations::Column::Status,
            Expr::value("timeout"),
        )
        .col_expr(
            app_function_invocations::Column::Error,
            Expr::value("reaped: still running past max lifetime (instance died or detached)"),
        )
        .filter(app_function_invocations::Column::Status.eq("running"))
        .filter(app_function_invocations::Column::CreatedAt.lt(cutoff))
        .exec(db)
        .await
    {
        error!("reaping stuck invocations failed: {e}");
    }
}

/// Stable i64 hash of a request body, stored on a keyed invocation so a key
/// reused with a *different* body is rejected rather than silently replayed.
fn request_body_hash(body: &[u8]) -> i64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    body.hash(&mut h);
    h.finish() as i64
}

/// Outcome of trying to acquire an invocation slot for a (possibly keyed) call.
enum Acquire {
    /// Run the isolate under this invocation-row id (freshly inserted or a
    /// reclaimed prior-failure row).
    Run(Uuid),
    /// Return this response without running (replay / 409 in-progress / 422
    /// body-mismatch). Boxed to keep the enum small.
    Return(Box<Response>),
}

fn json_error(status: StatusCode, error: &str, message: &str) -> Response {
    (
        status,
        axum::Json(serde_json::json!({ "error": error, "message": message })),
    )
        .into_response()
}

async fn find_keyed_invocation(
    db: &sea_orm::DatabaseConnection,
    app_id: Uuid,
    function_name: &str,
    user_id: Uuid,
    key: &str,
) -> Option<app_function_invocations::Model> {
    app_function_invocations::Entity::find()
        .filter(app_function_invocations::Column::AppId.eq(app_id))
        .filter(app_function_invocations::Column::FunctionName.eq(function_name))
        .filter(app_function_invocations::Column::UserId.eq(Some(user_id)))
        .filter(app_function_invocations::Column::IdempotencyKey.eq(key))
        .one(db)
        .await
        .unwrap_or(None)
}

#[allow(clippy::too_many_arguments)]
async fn insert_running_invocation(
    db: &sea_orm::DatabaseConnection,
    id: Uuid,
    app_id: Uuid,
    build_id: Uuid,
    function_name: &str,
    user_id: Uuid,
    key: Option<&str>,
    request_hash: Option<i64>,
) -> Result<(), sea_orm::DbErr> {
    app_function_invocations::ActiveModel {
        id: Set(id),
        app_id: Set(app_id),
        build_id: Set(build_id),
        function_name: Set(function_name.to_string()),
        mode: Set("route".to_string()),
        user_id: Set(Some(user_id)),
        status: Set("running".to_string()),
        duration_ms: Set(None),
        error: Set(None),
        cancel_requested_at: Set(None),
        created_at: Set(chrono::Utc::now().into()),
        idempotency_key: Set(key.map(str::to_string)),
        result_body: Set(None),
        request_hash: Set(request_hash),
    }
    .insert(db)
    .await
    .map(|_| ())
}

/// Atomically re-claim a settled/stale keyed row for a retry: flip it back to
/// `running` **only if** it isn't already running, so two concurrent retries
/// can't both execute. Winning → run under this id; losing the race → 409.
async fn reclaim_or_conflict(
    db: &sea_orm::DatabaseConnection,
    id: Uuid,
    stale_cutoff: chrono::DateTime<chrono::FixedOffset>,
) -> Acquire {
    use sea_orm::sea_query::Expr;
    let now: chrono::DateTime<chrono::FixedOffset> = chrono::Utc::now().into();
    let res = app_function_invocations::Entity::update_many()
        .col_expr(
            app_function_invocations::Column::Status,
            Expr::value("running"),
        )
        .col_expr(
            app_function_invocations::Column::Error,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            app_function_invocations::Column::ResultBody,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            app_function_invocations::Column::CreatedAt,
            Expr::value(now),
        )
        .filter(app_function_invocations::Column::Id.eq(id))
        // Reclaim a settled row (`status != 'running'`) OR an orphaned/stale
        // `running` row (created before the staleness cutoff — a hard crash left
        // it literally "running"). Race-safe: Postgres re-evaluates this WHERE
        // under the row lock, so a concurrent winner that resets
        // status='running' + created_at=now makes every loser match neither
        // clause → 0 rows → 409 (exactly one retry re-runs).
        .filter(
            sea_orm::Condition::any()
                .add(app_function_invocations::Column::Status.ne("running"))
                .add(app_function_invocations::Column::CreatedAt.lt(stale_cutoff)),
        )
        .exec(db)
        .await;
    match res {
        Ok(r) if r.rows_affected == 1 => Acquire::Run(id),
        _ => Acquire::Return(Box::new(json_error(
            StatusCode::CONFLICT,
            "IdempotencyKeyInProgress",
            "a request with this Idempotency-Key is already in progress",
        ))),
    }
}

/// Decide the fate of an existing keyed row: replay a matching success, reject a
/// body mismatch (422) or a concurrent in-flight (409), or reclaim a settled
/// failure / orphaned-running row so the caller can retry under the same key.
async fn resolve_keyed_row(
    db: &sea_orm::DatabaseConnection,
    row: app_function_invocations::Model,
    request_hash: i64,
) -> Acquire {
    if row.request_hash != Some(request_hash) {
        return Acquire::Return(Box::new(json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "IdempotencyKeyReused",
            "Idempotency-Key was reused with a different request body",
        )));
    }
    let cutoff: chrono::DateTime<chrono::FixedOffset> =
        (chrono::Utc::now() - chrono::Duration::seconds(REAP_AFTER_SECS)).into();
    let stale = row.created_at < cutoff;
    match (row.status.as_str(), row.result_body) {
        // A completed success replays its stored result.
        ("success", Some(body)) => Acquire::Return(Box::new(sse_response(success_sse_body(&body)))),
        // Genuinely in flight → duplicate in progress.
        ("running", _) if !stale => Acquire::Return(Box::new(json_error(
            StatusCode::CONFLICT,
            "IdempotencyKeyInProgress",
            "a request with this Idempotency-Key is already in progress",
        ))),
        // Settled failure, or a stale/orphaned `running` row → retryable: the
        // same `cutoff` makes the reclaim UPDATE match the stale-running case
        // that `Status.ne("running")` alone would miss.
        _ => reclaim_or_conflict(db, row.id, cutoff).await,
    }
}

/// Acquire an invocation slot. Keyless → a fresh row (no dedup). Keyed →
/// exactly-once **with retry**: replay a prior success, reject a concurrent
/// duplicate / body mismatch, or reclaim a prior failure so it can run again.
#[allow(clippy::too_many_arguments)]
async fn acquire_invocation(
    db: &sea_orm::DatabaseConnection,
    app_id: Uuid,
    build_id: Uuid,
    function_name: &str,
    user_id: Uuid,
    key: Option<&str>,
    request_hash: i64,
) -> Acquire {
    let Some(key) = key else {
        let id = Uuid::new_v4();
        if let Err(e) =
            insert_running_invocation(db, id, app_id, build_id, function_name, user_id, None, None)
                .await
        {
            error!("failed to write app_function_invocations row: {e}");
        }
        return Acquire::Run(id);
    };
    // Fast path: an existing keyed row decides the outcome.
    if let Some(row) = find_keyed_invocation(db, app_id, function_name, user_id, key).await {
        return resolve_keyed_row(db, row, request_hash).await;
    }
    // No row yet: claim by inserting. A lost race (concurrent first call) hits
    // the unique index; resolve against the winner.
    let id = Uuid::new_v4();
    match insert_running_invocation(
        db,
        id,
        app_id,
        build_id,
        function_name,
        user_id,
        Some(key),
        Some(request_hash),
    )
    .await
    {
        Ok(()) => Acquire::Run(id),
        Err(_) => match find_keyed_invocation(db, app_id, function_name, user_id, key).await {
            Some(row) => resolve_keyed_row(db, row, request_hash).await,
            // Not a uniqueness conflict (some other DB error): run without a row.
            None => Acquire::Run(id),
        },
    }
}

// ── Handler ──────────────────────────────────────────────────────────────

/// Entry point called from `customer_apps_serve::serve_pretty` when `rest`
/// begins with `fn/`.
pub async fn handle_function_request(
    org_slug: &str,
    app_slug: &str,
    function_name: &str,
    method: Method,
    headers: HeaderMap,
    body: axum::body::Bytes,
    refresh: bool,
) -> Response {
    // §11.10 — POST-only, before any gate/runtime work.
    if method != Method::POST {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }

    let outcome = match authenticate_and_authorize(&headers, org_slug, app_slug).await {
        Ok(o) => o,
        Err(status) => return status.into_response(),
    };
    let app = outcome.app;

    let db = match establish_connection().await {
        Ok(db) => db,
        Err(e) => {
            error!("db connection failed: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let Some(build_id) = app.published_build_id.or(app.draft_build_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let Some(func_row) = (match AppFunctions::find()
        .filter(app_functions::Column::BuildId.eq(build_id))
        .filter(app_functions::Column::Name.eq(function_name))
        .one(&db)
        .await
    {
        Ok(row) => row,
        Err(e) => {
            error!("app_functions lookup failed: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let manifest: FunctionManifestEntry = func_row
        .manifest_json
        .as_ref()
        .and_then(|j| serde_json::from_value(j.clone()).ok())
        .unwrap_or_default();
    // `route` defaults to active unless schedule/airwayStep are the only
    // declared surfaces — the SDK validator already enforced "at least one
    // surface active" at publish time, so a missing/true `route` here means
    // this row is route-invocable.
    if manifest.route == Some(false) {
        return StatusCode::NOT_FOUND.into_response();
    }

    // Caller-supplied idempotency key (route mode): enables exactly-once, with
    // retry, for side-effectful writes. Bounded length; empty → treated as
    // absent. The body is hashed so a key reused with a different body is
    // rejected rather than silently replaying the first result. Replay / retry /
    // conflict are all decided by `acquire_invocation` below.
    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 200)
        .map(str::to_string);
    let request_hash = request_body_hash(&body);

    // Opt-in result cache (manifest `cache.ttlSeconds`). A hit skips the isolate
    // run entirely; `?refresh` bypasses it. Side-effectful functions don't
    // declare a cache and never reach this. Keyed per (build, function, user,
    // body), so it can't leak across users or survive a redeploy.
    let cache_ttl = manifest.cache_ttl();
    if cache_ttl.is_some()
        && !refresh
        && let Some(cached) = result_cache::get(build_id, function_name, outcome.user_id, &body)
    {
        return sse_response(success_sse_body(&cached));
    }

    // §11.6 — rate limit per (user, app, function).
    let limit = manifest
        .rate_limit_per_minute
        .unwrap_or(DEFAULT_RATE_PER_MIN);
    if would_exceed_rate(outcome.user_id, app.id, function_name, limit) {
        // Return JSON, not an SSE frame: the client throws on any non-2xx
        // *before* it starts reading the event stream and JSON-parses the body,
        // so an `event: error\ndata: …` blob would surface as a garbled error.
        // Every other pre-stream error here is a bare status / JSON — the 429
        // was the lone SSE-framed outlier.
        return (
            StatusCode::TOO_MANY_REQUESTS,
            axum::Json(
                serde_json::json!({ "error": "RateLimitExceeded", "message": "too many requests" }),
            ),
        )
            .into_response();
    }

    // Resolve the build's string `build_id` (build store key) from its uuid.
    let Some(build) = (match AppBuilds::find_by_id(build_id).one(&db).await {
        Ok(b) => b,
        Err(e) => {
            error!("app_builds lookup failed: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let artifact_rel = format!("functions/{function_name}.js");
    let artifact =
        match customer_apps_build_store::get_object(app.id, &build.build_id, &artifact_rel).await {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return StatusCode::NOT_FOUND.into_response(),
            Err(e) => {
                error!("build store fetch failed: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };
    let artifact_js = String::from_utf8_lossy(&artifact).into_owned();

    // Lazy crash reaper (throttled): transition rows left `running` past any
    // function's max lifetime (orphaned by an instance death / thread detach)
    // so the audit trail is truthful and they stop looking in-flight.
    reap_stuck_invocations(&db).await;

    // §11.12 / §11.14 — acquire the invocation row (written up front so
    // cancellation has something to poll). For a keyed call this replays a prior
    // success, rejects a concurrent duplicate (409) or body mismatch (422), or
    // reclaims a prior failure so it can retry — see `acquire_invocation`.
    let invocation_id = match acquire_invocation(
        &db,
        app.id,
        build_id,
        function_name,
        outcome.user_id,
        idempotency_key.as_deref(),
        request_hash,
    )
    .await
    {
        Acquire::Run(id) => id,
        Acquire::Return(resp) => return *resp,
    };

    let timeout = resolve_timeout(&manifest);
    let started = Instant::now();

    // Shared log buffer: the isolate appends `console.*`/`ctx.log` during the
    // run; we drain it after and send it back with the response so a developer
    // never has to open the oxy server logs to see what a function printed.
    let logs: std::sync::Arc<Mutex<Vec<LogLine>>> = std::sync::Arc::new(Mutex::new(Vec::new()));

    #[cfg(feature = "customer-app-functions")]
    let (status_str, error_msg, body_text) = run_with_runtime(RunArgs {
        db: &db,
        app: &app,
        artifact_js,
        body: body.to_vec(),
        user_id: outcome.user_id,
        user_email: outcome.user_email.clone(),
        org_id: app.org_id,
        invocation_id,
        timeout,
        write_destinations: manifest.write_destinations(),
        logs: logs.clone(),
    })
    .await;

    #[cfg(not(feature = "customer-app-functions"))]
    let (status_str, error_msg, body_text) = {
        let _ = (&artifact_js, &body, timeout);
        (
            "error",
            Some("customer-app-functions feature not enabled".to_string()),
            String::new(),
        )
    };

    let duration_ms = started.elapsed().as_millis() as i64;
    let mut update: app_function_invocations::ActiveModel = app_function_invocations::ActiveModel {
        id: Set(invocation_id),
        ..Default::default()
    };
    update.status = Set(status_str.to_string());
    update.duration_ms = Set(Some(duration_ms));
    update.error = Set(error_msg.clone());
    // Persist the result for idempotent replay — only when a key was supplied,
    // so the audit table isn't bloated with every function's output.
    if idempotency_key.is_some() && status_str == "success" {
        update.result_body = Set(Some(body_text.clone()));
    }
    if let Err(e) = update.update(&db).await {
        error!("failed to update app_function_invocations row: {e}");
    }

    // Cache the successful result for functions that opted into `cache`.
    if status_str == "success"
        && let Some(ttl) = cache_ttl
    {
        result_cache::put(
            build_id,
            function_name,
            outcome.user_id,
            &body,
            body_text.clone(),
            ttl,
        );
    }

    // Drain captured logs and send them as `log` frames ahead of the terminal
    // frame (collected during the run and flushed together with the response —
    // batched, not live-tailed), so the app can show what the function printed —
    // on success and, crucially, on error (the console output before the throw).
    let captured: Vec<LogLine> = logs
        .lock()
        .map(|mut v| std::mem::take(&mut *v))
        .unwrap_or_default();
    let mut sse_body = String::new();
    for line in &captured {
        sse_body.push_str(&sse_event(
            "log",
            &serde_json::json!({ "level": line.level, "message": line.message }),
        ));
    }
    sse_body.push_str(&match error_msg {
        Some(msg) => sse_event(
            "error",
            &serde_json::json!({ "error": status_str, "message": msg }),
        ),
        None => success_sse_body(&body_text),
    });
    sse_response(sse_body)
}

/// Outcome triple shared by both feature arms: `(status, error_msg, body)`.
type RunOutcome = (&'static str, Option<String>, String);

/// A captured `ctx.log` / `console.*` line from a function run, surfaced back to
/// the app (as SSE `log` frames) so a developer sees output without opening the
/// oxy server logs. Defined here (un-gated) so the handler can hold the buffer
/// regardless of the `customer-app-functions` feature.
#[derive(Clone, serde::Serialize)]
pub struct LogLine {
    pub level: String,
    pub message: String,
}

#[cfg(feature = "customer-app-functions")]
struct RunArgs<'a> {
    db: &'a sea_orm::DatabaseConnection,
    app: &'a entity::apps::Model,
    artifact_js: String,
    body: Vec<u8>,
    user_id: Uuid,
    user_email: String,
    org_id: Uuid,
    invocation_id: Uuid,
    timeout: Duration,
    /// §11.3 allowlist — databases `ctx.warehouse.*` may write to (empty = none).
    write_destinations: Vec<String>,
    /// Shared log buffer: the isolate appends `console.*`/`ctx.log`; the handler
    /// drains it after the run and sends it back as `log` frames (batched with
    /// the response, not live-tailed).
    logs: std::sync::Arc<Mutex<Vec<LogLine>>>,
}

/// Build the project context + host, spawn the cancel watchdog, and drive
/// the isolate. Kept off the main handler body to hold it under the
/// function-size budget.
#[cfg(feature = "customer-app-functions")]
async fn run_with_runtime(args: RunArgs<'_>) -> RunOutcome {
    use crate::server::api::customer_apps_gates::build_project_context;
    use entity::prelude::Workspaces;

    // Resolve the project context (connectors) from the app's workspace.
    let workspace = match Workspaces::find_by_id(args.app.project_id)
        .one(args.db)
        .await
    {
        Ok(Some(ws)) => ws,
        Ok(None) => return ("error", Some("workspace not found".into()), String::new()),
        Err(e) => {
            return (
                "error",
                Some(format!("workspace lookup failed: {e}")),
                String::new(),
            );
        }
    };
    let proj_ctx = match build_project_context(&workspace, args.user_id, args.app.project_id).await
    {
        Ok(c) => c,
        Err(_) => {
            return (
                "error",
                Some("could not build workspace context".into()),
                String::new(),
            );
        }
    };
    let host = host::into_arc(host::ProjectFunctionHost::new(
        proj_ctx,
        args.db.clone(),
        args.write_destinations.clone(),
    ));

    let env = resolve_function_env(args.db, args.app.project_id, args.app.id).await;

    let ctx = runtime::InvocationCtx {
        user: runtime::CtxUser {
            id: args.user_id.to_string(),
            email: args.user_email,
            org_id: args.org_id.to_string(),
        },
        env,
    };

    // §11.4 — cancellation watchdog: poll `cancel_requested_at` every 1s and
    // fire the cancel signal when it's set (dashboard cancel / client gone).
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    let watchdog = tokio::spawn(spawn_cancel_watchdog(
        args.db.clone(),
        args.invocation_id,
        cancel_tx,
    ));

    let result = runtime::run(
        args.artifact_js,
        ctx,
        args.body,
        host,
        cancel_rx,
        args.timeout,
        args.logs,
    )
    .await;
    watchdog.abort();

    match result {
        Ok(resp) => ("success", None, resp.body),
        Err(runtime::RuntimeError::Cancelled) => (
            "cancelled",
            Some("function was cancelled".into()),
            String::new(),
        ),
        Err(runtime::RuntimeError::Timeout) => (
            "timeout",
            Some("function execution timed out".into()),
            String::new(),
        ),
        Err(e) => ("error", Some(e.to_string()), String::new()),
    }
}

/// Resolve `ctx.env` for an invocation (§11.4): secrets stored under the
/// `apps/<app_id>/<KEY>` prefix in the project's secret manager, exposed to
/// the isolate with the prefix stripped (so the function sees plain `KEY`).
/// Resolved fresh per invocation — never cached in the artifact.
#[cfg(feature = "customer-app-functions")]
async fn resolve_function_env(
    db: &sea_orm::DatabaseConnection,
    project_id: Uuid,
    app_id: Uuid,
) -> std::collections::BTreeMap<String, String> {
    use oxy::service::secret_manager::SecretManagerService;

    let secret_manager = SecretManagerService::new(project_id);
    let prefix = format!("apps/{app_id}/");

    let names = match secret_manager.list_secrets(db).await {
        Ok(secrets) => secrets
            .into_iter()
            .filter_map(|s| {
                // Take the suffix as an owned `String` first so the borrow of
                // `s.name` ends before we move `s.name` into the tuple.
                let key = s.name.strip_prefix(&prefix)?.to_string();
                Some((s.name, key))
            })
            .collect::<Vec<_>>(),
        Err(e) => {
            error!("ctx.env: failed to list secrets for app {app_id}: {e}");
            return Default::default();
        }
    };

    let mut env = std::collections::BTreeMap::new();
    for (full_name, key) in names {
        match secret_manager.get_secret(&full_name).await {
            Some(value) => {
                env.insert(key, value);
            }
            None => error!("ctx.env: failed to resolve secret '{full_name}'"),
        }
    }
    env
}

/// Poll `app_function_invocations.cancel_requested_at` once per second;
/// resolve `cancel_tx` the first time it's non-null so the runtime can
/// terminate the isolate.
#[cfg(feature = "customer-app-functions")]
async fn spawn_cancel_watchdog(
    db: sea_orm::DatabaseConnection,
    invocation_id: Uuid,
    cancel_tx: tokio::sync::oneshot::Sender<()>,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        match AppFunctionInvocations::find_by_id(invocation_id)
            .one(&db)
            .await
        {
            Ok(Some(row)) if row.cancel_requested_at.is_some() => {
                let _ = cancel_tx.send(());
                return;
            }
            Ok(_) => {}
            Err(e) => {
                error!("cancel watchdog poll failed: {e}");
                return;
            }
        }
    }
}
