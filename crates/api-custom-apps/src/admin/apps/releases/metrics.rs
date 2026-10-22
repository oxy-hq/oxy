//! The per-release reads that can be slow: function-call metrics, the
//! migration ledger and first-seen failures. Each runs alone ([`bounded`]):
//! its own transaction under `SET LOCAL statement_timeout`, inside a wall-clock
//! budget. A cancelled statement aborts its transaction, so sharing one would
//! turn a slow metrics read into a failed migrations read and a `500`; alone,
//! the slow part answers `unavailable` and the rest are still read.
//!
//! Only bookkeeping columns are read — status, mode, duration, result status,
//! fingerprint — never `error` or `result_body`.

use std::time::Duration;

use chrono::{DateTime, FixedOffset, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, DbErr, FromQueryResult, Statement,
    TransactionTrait, Value,
};
use uuid::Uuid;

use super::timeline::{Applied, FirstSeen, Stretch};

/// Per statement, server side. `LOCAL`: it ends with the transaction, so the
/// pooled connection goes back as it came.
pub const STATEMENT_TIMEOUT_MS: u64 = 4_000;
/// Per part, wall clock: the connection wait included.
pub const BUDGET: Duration = Duration::from_secs(5);
/// Migration ledger rows one read returns.
pub const MAX_MIGRATIONS: usize = 200;
/// New failures one read returns, as `issues::MAX_ISSUES`.
pub const MAX_NEW_ISSUES: usize = 50;

/// Run one read alone. `None`: it failed or ran out of time, already logged.
pub async fn bounded<T: FromQueryResult + Send>(
    db: &DatabaseConnection,
    part: &str,
    sql: &str,
    values: Vec<Value>,
    timeout_ms: u64,
) -> Option<Vec<T>> {
    let read = async {
        let txn = db.begin().await?;
        txn.execute_unprepared(&format!("SET LOCAL statement_timeout = {timeout_ms}"))
            .await?;
        let stmt = Statement::from_sql_and_values(DbBackend::Postgres, sql, values);
        let rows = T::find_by_statement(stmt).all(&txn).await?;
        txn.commit().await?;
        Ok::<_, DbErr>(rows)
    };
    let budget = BUDGET.max(Duration::from_millis(timeout_ms));
    match tokio::time::timeout(budget, read).await {
        Ok(Ok(rows)) => Some(rows),
        Ok(Err(e)) => {
            tracing::warn!(part, error = %e, "release timeline read failed");
            None
        }
        Err(_) => {
            tracing::warn!(part, "release timeline read ran out of time");
            None
        }
    }
}

/// The stretches as the parallel arrays `unnest` takes: index, build, start,
/// end. Only stretches with a build: a pruned one's calls were deleted with it.
pub fn stretch_arrays(stretches: &[Stretch]) -> [Value; 4] {
    let kept: Vec<&Stretch> = stretches.iter().filter(|s| s.comparable()).collect();
    let idx: Vec<i32> = kept.iter().map(|s| s.index as i32).collect();
    let builds: Vec<Uuid> = kept.iter().filter_map(|s| s.build_id).collect();
    let starts: Vec<DateTime<Utc>> = kept.iter().map(|s| s.starts_at).collect();
    let ends: Vec<DateTime<Utc>> = kept.iter().map(|s| s.ends_at).collect();
    [idx.into(), builds.into(), starts.into(), ends.into()]
}

/// The stretches, one row each.
pub const STRETCHES_CTE: &str = "s AS (SELECT * FROM unnest($2::int[], $3::uuid[], \
     $4::timestamptz[], $5::timestamptz[]) AS s(idx, build_id, starts_at, ends_at))";

/// A call counts only inside its own build's stretch. Failed is what Issues
/// and the pager count: a fingerprint, an error or timeout, or a success that
/// answered 5xx. `shed` is load the platform refused, not the app failing,
/// and `running` has not finished: neither is a call here. p50/p95 are route
/// mode only — what a caller waited.
pub fn metrics_sql() -> String {
    const COUNTED: &str = "i.status NOT IN ('shed', 'running')";
    const ROUTE: &str = "i.mode = 'route' AND i.status NOT IN ('shed', 'running')";
    format!(
        "WITH {STRETCHES_CTE} \
         SELECT s.idx::bigint AS idx, \
           COUNT(i.id) FILTER (WHERE {COUNTED})::bigint AS calls, \
           COUNT(i.id) FILTER (WHERE {COUNTED} AND (i.failure_fingerprint IS NOT NULL \
               OR i.status IN ('error', 'timeout') \
               OR (i.status = 'success' AND i.result_status >= 500)))::bigint AS failed, \
           COUNT(i.id) FILTER (WHERE i.status = 'shed')::bigint AS shed, \
           COUNT(i.id) FILTER (WHERE {ROUTE})::bigint AS route_calls, \
           round(percentile_cont(0.5) WITHIN GROUP (ORDER BY i.duration_ms) \
               FILTER (WHERE {ROUTE}))::bigint AS p50_ms, \
           round(percentile_cont(0.95) WITHIN GROUP (ORDER BY i.duration_ms) \
               FILTER (WHERE {ROUTE}))::bigint AS p95_ms \
         FROM s LEFT JOIN app_function_invocations i \
           ON i.app_id = $1 AND i.environment = 'production' AND i.build_id = s.build_id \
          AND i.created_at >= s.starts_at AND i.created_at < s.ends_at \
         GROUP BY s.idx"
    )
}

/// One stretch's function-call numbers.
#[derive(Debug, Clone, Default, PartialEq, Eq, FromQueryResult)]
pub struct MetricsRow {
    pub idx: i64,
    pub calls: i64,
    pub failed: i64,
    pub shed: i64,
    pub route_calls: i64,
    pub p50_ms: Option<i64>,
    pub p95_ms: Option<i64>,
}

pub async fn metrics(
    db: &DatabaseConnection,
    app_id: Uuid,
    stretches: &[Stretch],
) -> Option<Vec<MetricsRow>> {
    let [idx, builds, starts, ends] = stretch_arrays(stretches);
    let values = vec![app_id.into(), idx, builds, starts, ends];
    bounded(db, "metrics", &metrics_sql(), values, STATEMENT_TIMEOUT_MS).await
}

const MIGRATIONS_SQL: &str = "SELECT store, filename, applied_at, applied_by_build \
     FROM custom_app_migrations \
     WHERE app_id = $1 AND target = 'production' AND applied_at >= $2 \
     ORDER BY applied_at, store, filename LIMIT $3";

#[derive(Debug, FromQueryResult)]
struct MigrationRow {
    store: String,
    filename: String,
    applied_at: DateTime<FixedOffset>,
    applied_by_build: Option<Uuid>,
}

/// Production's files applied since `since`, oldest first, and whether more
/// were applied than were read.
pub async fn migrations(
    db: &DatabaseConnection,
    app_id: Uuid,
    since: DateTime<Utc>,
) -> Option<(Vec<Applied>, bool)> {
    let fetch = (MAX_MIGRATIONS + 1) as i64;
    let values = vec![app_id.into(), since.into(), fetch.into()];
    let rows: Vec<MigrationRow> = bounded(
        db,
        "migrations",
        MIGRATIONS_SQL,
        values,
        STATEMENT_TIMEOUT_MS,
    )
    .await?;
    let truncated = rows.len() > MAX_MIGRATIONS;
    let applied = rows
        .into_iter()
        .take(MAX_MIGRATIONS)
        .map(|r| Applied {
            store: r.store,
            filename: r.filename,
            applied_at: r.applied_at.with_timezone(&Utc),
            build_id: r.applied_by_build,
        })
        .collect();
    Some((applied, truncated))
}

/// Each `(function, fingerprint)` that failed in production since `$2` and
/// never before it: its first occurrence's build and time, and how often it
/// happened since. The first row per pair is picked with `DISTINCT ON` over
/// the partial failure index rather than aggregating every row's build, and
/// "never before" is one index probe per pair.
const NEW_ISSUES_SQL: &str = "WITH firsts AS ( \
       SELECT DISTINCT ON (function_name, failure_fingerprint) \
              function_name, failure_fingerprint, build_id, created_at \
       FROM app_function_invocations \
       WHERE app_id = $1 AND environment = 'production' \
         AND failure_fingerprint IS NOT NULL AND created_at >= $2 \
       ORDER BY function_name, failure_fingerprint, created_at, id) \
     SELECT f.function_name, f.failure_fingerprint AS fingerprint, f.build_id, \
            f.created_at AS first_seen, \
            (SELECT COUNT(*) FROM app_function_invocations c \
              WHERE c.app_id = $1 AND c.environment = 'production' \
                AND c.function_name = f.function_name \
                AND c.failure_fingerprint = f.failure_fingerprint \
                AND c.created_at >= $2)::bigint AS occurrences \
     FROM firsts f \
     WHERE NOT EXISTS (SELECT 1 FROM app_function_invocations o \
              WHERE o.app_id = $1 AND o.environment = 'production' \
                AND o.function_name = f.function_name \
                AND o.failure_fingerprint = f.failure_fingerprint \
                AND o.created_at < $2) \
     ORDER BY f.created_at DESC LIMIT $3";

#[derive(Debug, FromQueryResult)]
struct NewIssueRow {
    function_name: String,
    fingerprint: String,
    build_id: Uuid,
    first_seen: DateTime<FixedOffset>,
    occurrences: i64,
}

/// Failures first seen since `since`, newest first, capped.
pub async fn new_issues(
    db: &DatabaseConnection,
    app_id: Uuid,
    since: DateTime<Utc>,
) -> Option<Vec<FirstSeen>> {
    let values = vec![app_id.into(), since.into(), (MAX_NEW_ISSUES as i64).into()];
    let rows: Vec<NewIssueRow> = bounded(
        db,
        "new_issues",
        NEW_ISSUES_SQL,
        values,
        STATEMENT_TIMEOUT_MS,
    )
    .await?;
    Some(
        rows.into_iter()
            .map(|r| FirstSeen {
                function_name: r.function_name,
                fingerprint: r.fingerprint,
                build_id: r.build_id,
                first_seen: r.first_seen.with_timezone(&Utc),
                occurrences: r.occurrences,
            })
            .collect(),
    )
}
