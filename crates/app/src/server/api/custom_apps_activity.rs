//! HTTP surface for custom-app usage tracking.
//!
//! Two consumers:
//!
//! 1. **Bundle SDK** — `POST /api/customer-apps/<app_id>/events` lets
//!    `useTrackEvent("export-clicked", {...})` post engineer-tagged
//!    events. Reuses the existing custom-app gate chain (cookie OR
//!    bearer auth, org-membership / app-admin access check), then
//!    validates the event shape and inserts via
//!    [`custom_apps_tracking::record_event`]. Rate limited per
//!    (user, app) to keep a misconfigured `useEffect(track, [])`
//!    loop from DoSing the server.
//!
//! 2. **Admin Activity tab** — three read endpoints serving the
//!    headline card, visitors table, and events table. All gated by
//!    app-admin (mirrors the rest of `/api/admin/apps/` access).
//!
//! Storage is PostgreSQL — see the design (now embedded in
//! `internal-docs/customer-apps.md` §13) for the rationale.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Path, Query};
use axum::http::{HeaderMap, StatusCode};
use chrono::{DateTime, FixedOffset, Utc};
use entity::{apps, custom_app_event, custom_app_view_event};
use oxy::database::client::establish_connection;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, FromQueryResult, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Statement,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::custom_apps_gates::check_custom_app_gates;
use super::custom_apps_tracking::{self, EventError};

// ── Rate limit (per user+app) ───────────────────────────────────────────

/// 60 events / minute / (user, app) — generous for "track every
/// meaningful interaction" but blocks a misconfigured
/// `useEffect(track, [])` loop. Window is rolling, computed via a
/// per-key bucket reset each minute.
const RATE_PER_MIN: u64 = 60;

/// How long an idle bucket sticks around in the rate table before
/// being garbage-collected. Two windows = enough that a borderline-
/// rate-limited user doesn't lose their counter mid-burst from a
/// concurrent eviction; small enough that the table stays bounded
/// when distinct (user, app) pairs churn over time.
const RATE_BUCKET_TTL: Duration = Duration::from_secs(120);

#[derive(Debug)]
struct RateBucket {
    window_start: Instant,
    count: u64,
}

/// Per-(user, app) bucket map. Read-modify-write happens entirely
/// inside the table mutex, so a `RateBucket` lives by value and we
/// don't need Arc/atomic gymnastics. The critical section is a
/// handful of nanoseconds per request, which is fine at the
/// event-ingest volume this limiter is meant to handle.
///
/// **History.** The earlier version stored `Arc<RateBucket>` and
/// tried to mutate `window_start` via `Arc::get_mut` — but the
/// outer `bucket = …clone()` kept the strong count ≥ 2 for the rest
/// of the call, so `get_mut` always returned `None` and the limiter
/// silently never tripped. Replacing it with this plain-value layout
/// removes the failure mode entirely.
fn rate_table() -> &'static Mutex<HashMap<(Uuid, Uuid), RateBucket>> {
    static TABLE: std::sync::OnceLock<Mutex<HashMap<(Uuid, Uuid), RateBucket>>> =
        std::sync::OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Returns `true` when the (user, app) pair has burned through its
/// per-minute allowance, `false` when there's still budget. Records
/// the new tick on the way through.
///
/// Also opportunistically evicts buckets whose window expired more
/// than [`RATE_BUCKET_TTL`] ago — keeps the table bounded under
/// churn (lots of distinct callers over time) without needing a
/// separate sweep task.
fn would_exceed_rate(user_id: Uuid, app_id: Uuid) -> bool {
    let now = Instant::now();
    let mut table = rate_table().lock().unwrap();

    // Opportunistic GC. O(n) over the table but n is bounded by
    // "active (user, app) pairs in the last 2 minutes" — tiny.
    table.retain(|_, b| now.duration_since(b.window_start) < RATE_BUCKET_TTL);

    let bucket = table
        .entry((user_id, app_id))
        .or_insert_with(|| RateBucket {
            window_start: now,
            count: 0,
        });
    if now.duration_since(bucket.window_start).as_secs() >= 60 {
        bucket.window_start = now;
        bucket.count = 0;
    }
    bucket.count += 1;
    bucket.count > RATE_PER_MIN
}

#[cfg(test)]
mod rate_tests {
    use super::*;

    /// Reset the rate table between tests — they share a single
    /// process-wide map via the `OnceLock`, so leaving rows behind
    /// would let later tests inherit "already-exceeded" state from
    /// earlier ones.
    fn reset_table() {
        rate_table().lock().unwrap().clear();
    }

    #[test]
    fn allows_up_to_the_limit_then_blocks() {
        reset_table();
        let u = Uuid::new_v4();
        let a = Uuid::new_v4();
        for i in 1..=RATE_PER_MIN {
            assert!(
                !would_exceed_rate(u, a),
                "request {i} of {RATE_PER_MIN} should be allowed"
            );
        }
        // The (N+1)-th request trips the limit.
        assert!(
            would_exceed_rate(u, a),
            "request {} should be blocked",
            RATE_PER_MIN + 1
        );
    }

    #[test]
    fn isolates_buckets_per_user_app_pair() {
        reset_table();
        let user_a = Uuid::new_v4();
        let user_b = Uuid::new_v4();
        let app = Uuid::new_v4();
        for _ in 0..RATE_PER_MIN {
            assert!(!would_exceed_rate(user_a, app));
        }
        // user_a is now at the limit; user_b should still have full budget.
        assert!(!would_exceed_rate(user_b, app));
    }
}

// ── Bundle endpoint: POST /events ───────────────────────────────────────

/// Bundle SDK request body. `payload` is engineer-defined JSON; the
/// server validates only the shape (object, ≤ 4 KiB). `session_id` is
/// optional — when absent the server falls back to the session cookie
/// (set by the HTML serve) or mints a fresh one. The cookie path is
/// the common case; explicit field is for SSR-side calls where the
/// SDK has the session id in hand from `window.__OXY_APP__`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventRequest {
    pub event_name: String,
    #[serde(default = "default_payload")]
    pub payload: serde_json::Value,
    #[serde(default)]
    pub session_id: Option<Uuid>,
}

fn default_payload() -> serde_json::Value {
    serde_json::json!({})
}

#[derive(Debug, Serialize)]
pub struct EventAck {
    pub id: Uuid,
}

#[derive(Serialize)]
struct ApiErr {
    message: String,
}

fn err(status: StatusCode, msg: impl Into<String>) -> axum::response::Response {
    use axum::response::IntoResponse;
    (
        status,
        Json(ApiErr {
            message: msg.into(),
        }),
    )
        .into_response()
}

/// `POST /api/customer-apps/<app_id>/events` — bundle SDK ingest.
#[tracing::instrument(skip_all, fields(project_id = %project_id))]
pub async fn post_event(
    Path(project_id): Path<Uuid>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> axum::response::Response {
    let ctx = match check_custom_app_gates(&headers, project_id).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    let req: EventRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return err(StatusCode::BAD_REQUEST, format!("bad body: {e}")),
    };

    // Find the app by project_id (the gates context has the workspace
    // membership locked in; we look up the app row to get its id +
    // confirm it exists). Note: the existing gate model uses
    // project_id as the URL key for compatibility with the rest of the
    // bundle SDK surface (/query, /semantic-query, …). For event
    // tracking we need the actual `apps.id` — derive it via the
    // workspace.
    let db = match establish_connection().await {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("event ingest DB connect failed: {e}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "DB unavailable");
        }
    };
    let app = match apps::Entity::find()
        .filter(apps::Column::ProjectId.eq(ctx.project_id))
        .one(&db)
        .await
    {
        Ok(Some(a)) => a,
        Ok(None) => return err(StatusCode::NOT_FOUND, "no app registered for this project"),
        Err(e) => {
            tracing::error!("event ingest app lookup: {e}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "lookup failed");
        }
    };

    if would_exceed_rate(ctx.user.id, app.id) {
        return err(
            StatusCode::TOO_MANY_REQUESTS,
            format!("rate limit: {RATE_PER_MIN} events / minute / app"),
        );
    }

    // Session id precedence: explicit body field → cookie → fresh mint.
    let session_id = req
        .session_id
        .or_else(|| custom_apps_tracking::session_id_from_headers(&headers))
        .unwrap_or_else(Uuid::new_v4);

    let result = custom_apps_tracking::record_event(
        &db,
        app.id,
        ctx.user.id,
        ctx.user.email.clone(),
        session_id,
        req.event_name,
        req.payload,
    )
    .await;

    match result {
        Ok(id) => {
            use axum::response::IntoResponse;
            (StatusCode::CREATED, Json(EventAck { id })).into_response()
        }
        Err(EventError::BadName(_)) | Err(EventError::NotAnObject) => {
            err(StatusCode::BAD_REQUEST, result.unwrap_err().to_string())
        }
        Err(EventError::PayloadTooLarge) => err(
            StatusCode::PAYLOAD_TOO_LARGE,
            result.unwrap_err().to_string(),
        ),
        Err(EventError::Db(msg)) => {
            tracing::error!("event ingest insert failed: {msg}");
            err(StatusCode::INTERNAL_SERVER_ERROR, "insert failed")
        }
    }
}

// ── Admin endpoints: activity queries ───────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ActivitySummary {
    pub total_views_7d: i64,
    pub unique_users_7d: i64,
    pub total_events_7d: i64,
    pub last_viewed_at: Option<DateTime<FixedOffset>>,
}

/// `GET /api/admin/apps/<app_id>/activity/summary`
pub async fn get_summary(
    Path(app_id): Path<Uuid>,
) -> Result<Json<ActivitySummary>, (StatusCode, String)> {
    let db = establish_connection()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let cutoff = (Utc::now() - chrono::Duration::days(7)).fixed_offset();

    let total_views_7d = custom_app_view_event::Entity::find()
        .filter(custom_app_view_event::Column::AppId.eq(app_id))
        .filter(custom_app_view_event::Column::ViewedAt.gte(cutoff))
        .count(&db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        as i64;

    let unique_users_7d = unique_user_count(&db, app_id, cutoff)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total_events_7d = custom_app_event::Entity::find()
        .filter(custom_app_event::Column::AppId.eq(app_id))
        .filter(custom_app_event::Column::OccurredAt.gte(cutoff))
        .count(&db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        as i64;

    let last_viewed_at = custom_app_view_event::Entity::find()
        .filter(custom_app_view_event::Column::AppId.eq(app_id))
        .order_by_desc(custom_app_view_event::Column::ViewedAt)
        .one(&db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map(|r| r.viewed_at);

    Ok(Json(ActivitySummary {
        total_views_7d,
        unique_users_7d,
        total_events_7d,
        last_viewed_at,
    }))
}

async fn unique_user_count(
    db: &DatabaseConnection,
    app_id: Uuid,
    cutoff: DateTime<FixedOffset>,
) -> Result<i64, sea_orm::DbErr> {
    #[derive(FromQueryResult)]
    struct Count {
        n: i64,
    }
    let backend = db.get_database_backend();
    let stmt = Statement::from_sql_and_values(
        backend,
        r#"SELECT COUNT(DISTINCT user_id)::bigint AS n
           FROM custom_app_view_event
           WHERE app_id = $1 AND viewed_at >= $2"#,
        [app_id.into(), cutoff.into()],
    );
    let row = Count::find_by_statement(stmt).one(db).await?;
    Ok(row.map(|r| r.n).unwrap_or(0))
}

#[derive(Debug, Deserialize)]
pub struct VisitorsQuery {
    #[serde(default = "default_days")]
    pub days: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_days() -> i64 {
    7
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Serialize)]
pub struct VisitorRow {
    pub user_id: Uuid,
    pub user_email: String,
    pub sessions: i64,
    pub views: i64,
    pub first_seen_at: DateTime<FixedOffset>,
    pub last_seen_at: DateTime<FixedOffset>,
    /// The visitor's app role on their **most recent view that recorded one**,
    /// not their role today — see the query below for why the two differ.
    pub app_role: Option<String>,
    /// Their org role, same basis.
    pub org_role: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VisitorsResponse {
    pub rows: Vec<VisitorRow>,
}

/// `GET /api/admin/apps/<app_id>/activity/visitors`
pub async fn get_visitors(
    Path(app_id): Path<Uuid>,
    Query(q): Query<VisitorsQuery>,
) -> Result<Json<VisitorsResponse>, (StatusCode, String)> {
    let db = establish_connection()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let cutoff = (Utc::now() - chrono::Duration::days(q.days)).fixed_offset();

    #[derive(FromQueryResult)]
    struct Row {
        user_id: Uuid,
        user_email: String,
        sessions: i64,
        views: i64,
        first_seen_at: DateTime<FixedOffset>,
        last_seen_at: DateTime<FixedOffset>,
        app_role: Option<String>,
        org_role: Option<String>,
    }

    // Roles are per-view snapshots, so a roll-up has to pick one. It picks the
    // latest **recorded** value (`FILTER (WHERE … IS NOT NULL)`) rather than the
    // value on the latest row, because NULL means "not recorded" — a row from
    // before the columns existed, or one whose lookup failed — and letting that
    // blank out a visitor whose role is perfectly well known would read as "no
    // role" and be wrong in the direction that matters.
    //
    // Deliberately NOT a join to `app_members` / `org_members`: that would show
    // today's role against last month's activity and silently rewrite the log.
    // A visitor whose role changed mid-window shows their newest one here; the
    // change itself is still visible row-by-row in the underlying table.
    let backend = db.get_database_backend();
    let stmt = Statement::from_sql_and_values(
        backend,
        r#"SELECT
             user_id,
             MAX(user_email) AS user_email,
             COUNT(DISTINCT session_id)::bigint AS sessions,
             COUNT(*)::bigint AS views,
             MIN(viewed_at) AS first_seen_at,
             MAX(viewed_at) AS last_seen_at,
             (array_agg(app_role ORDER BY viewed_at DESC)
                FILTER (WHERE app_role IS NOT NULL))[1] AS app_role,
             (array_agg(org_role ORDER BY viewed_at DESC)
                FILTER (WHERE org_role IS NOT NULL))[1] AS org_role
           FROM custom_app_view_event
           WHERE app_id = $1 AND viewed_at >= $2
           GROUP BY user_id
           ORDER BY last_seen_at DESC
           LIMIT $3"#,
        [app_id.into(), cutoff.into(), q.limit.into()],
    );
    let rows: Vec<Row> = Row::find_by_statement(stmt)
        .all(&db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(VisitorsResponse {
        rows: rows
            .into_iter()
            .map(|r| VisitorRow {
                user_id: r.user_id,
                user_email: r.user_email,
                sessions: r.sessions,
                views: r.views,
                first_seen_at: r.first_seen_at,
                last_seen_at: r.last_seen_at,
                app_role: r.app_role,
                org_role: r.org_role,
            })
            .collect(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    #[serde(default = "default_days")]
    pub days: i64,
    #[serde(default)]
    pub event_name: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

#[derive(Debug, Serialize)]
pub struct EventGroupRow {
    pub event_name: String,
    pub count: i64,
    pub last_fired_at: DateTime<FixedOffset>,
}

#[derive(Debug, Serialize)]
pub struct EventOccurrenceRow {
    pub id: Uuid,
    pub event_name: String,
    pub user_email: String,
    pub payload: serde_json::Value,
    pub occurred_at: DateTime<FixedOffset>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum EventsResponse {
    Grouped { groups: Vec<EventGroupRow> },
    Occurrences { rows: Vec<EventOccurrenceRow> },
}

/// `GET /api/admin/apps/<app_id>/activity/events`
///
/// Without `event_name`: returns per-event-name counts + last-fired
/// timestamps (the rolled-up Events table). With `event_name`:
/// returns the recent occurrences for that one name (the drill-down).
pub async fn get_events(
    Path(app_id): Path<Uuid>,
    Query(q): Query<EventsQuery>,
) -> Result<Json<EventsResponse>, (StatusCode, String)> {
    let db = establish_connection()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let cutoff = (Utc::now() - chrono::Duration::days(q.days)).fixed_offset();

    if let Some(name) = q.event_name {
        let rows = custom_app_event::Entity::find()
            .filter(custom_app_event::Column::AppId.eq(app_id))
            .filter(custom_app_event::Column::EventName.eq(name))
            .filter(custom_app_event::Column::OccurredAt.gte(cutoff))
            .order_by_desc(custom_app_event::Column::OccurredAt)
            .limit(q.limit as u64)
            .all(&db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(Json(EventsResponse::Occurrences {
            rows: rows
                .into_iter()
                .map(|r| EventOccurrenceRow {
                    id: r.id,
                    event_name: r.event_name,
                    user_email: r.user_email,
                    payload: r.payload,
                    occurred_at: r.occurred_at,
                })
                .collect(),
        }))
    } else {
        #[derive(FromQueryResult)]
        struct Row {
            event_name: String,
            count: i64,
            last_fired_at: DateTime<FixedOffset>,
        }
        let backend = db.get_database_backend();
        let stmt = Statement::from_sql_and_values(
            backend,
            r#"SELECT
                 event_name,
                 COUNT(*)::bigint AS count,
                 MAX(occurred_at) AS last_fired_at
               FROM custom_app_event
               WHERE app_id = $1 AND occurred_at >= $2
               GROUP BY event_name
               ORDER BY count DESC
               LIMIT $3"#,
            [app_id.into(), cutoff.into(), q.limit.into()],
        );
        let groups: Vec<Row> = Row::find_by_statement(stmt)
            .all(&db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(Json(EventsResponse::Grouped {
            groups: groups
                .into_iter()
                .map(|r| EventGroupRow {
                    event_name: r.event_name,
                    count: r.count,
                    last_fired_at: r.last_fired_at,
                })
                .collect(),
        }))
    }
}

// ── List-level: last_active_at per app ──────────────────────────────────

/// Returns `MAX(viewed_at)` per app id, for the list-level
/// "last active" column on the Custom apps admin page. Empty map entry
/// when the app has no view events. Single query — N+1 would kill the
/// list page for an org with 100+ apps.
pub async fn last_active_at_by_app(
    db: &DatabaseConnection,
    app_ids: &[Uuid],
) -> Result<HashMap<Uuid, DateTime<FixedOffset>>, sea_orm::DbErr> {
    if app_ids.is_empty() {
        return Ok(HashMap::new());
    }
    #[derive(FromQueryResult)]
    struct Row {
        app_id: Uuid,
        last_active_at: DateTime<FixedOffset>,
    }
    // Build the IN placeholder list ($1, $2, …). We can't pass Vec
    // directly through SeaORM's statement params API for ANY/IN, so
    // expand inline.
    let placeholders = (1..=app_ids.len())
        .map(|i| format!("${i}"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        r#"SELECT app_id, MAX(viewed_at) AS last_active_at
           FROM custom_app_view_event
           WHERE app_id IN ({placeholders})
           GROUP BY app_id"#
    );
    let values: Vec<sea_orm::Value> = app_ids.iter().map(|&id| id.into()).collect();
    let backend = db.get_database_backend();
    let stmt = Statement::from_sql_and_values(backend, &sql, values);
    let rows: Vec<Row> = Row::find_by_statement(stmt).all(db).await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.app_id, r.last_active_at))
        .collect())
}
