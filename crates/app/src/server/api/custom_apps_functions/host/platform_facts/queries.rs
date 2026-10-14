//! One read per source, every one a control-plane table Oxy already writes.
//!
//! **One snapshot.** Every statement runs in a single read-only `REPEATABLE
//! READ` transaction, so the counts, the apps and the orgs all describe the
//! same instant: a view landing mid-read cannot show in `appViews` and not in
//! `lastActiveAt`.
//!
//! Snapshot reads (orgs, billing, partners, sign-ins, apps, first thread) are
//! one pass over their table. "Latest" reads are one index probe per app
//! (`ORDER BY … DESC LIMIT 1` in a `LATERAL`), never a `MAX` over every row an
//! app ever wrote. The customer-activity probes are also **floored** at
//! [`super::LAST_ACTIVITY_HORIZON_DAYS`]: they skip staff's and unattended
//! rows, so a probe with no floor walks every row since the customer last
//! acted, and for a churned customer whose webhooks keep arriving that only
//! grows. The activity reads are **windowed**: each joins its events to
//! [`Windows`] (`unnest(starts, ends) WITH ORDINALITY`) and groups
//! by `(org, window)`, so one statement answers every week and `last7d` at
//! once, and a distinct-people count is taken per window rather than summed.
//!
//! As in the usage report, every windowed app-keyed read names its apps
//! (`app_id = ANY`) and cuts time to the windows' outer bounds, which is what
//! lets it use each table's `(app_id, …, time)` index: the invocation table is
//! never pruned, so a scan by time alone would read all of it.
//!
//! Windowed parameters, the same in every statement: `$1` window starts, `$2`
//! window ends, `$3` the internal orgs (whose members are staff), `$4` and
//! `$5` the outer bounds, `$6` every app.

use chrono::{DateTime, Utc};
use sea_orm::{
    AccessMode, ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, FromQueryResult,
    IsolationLevel, Statement, TransactionTrait, Value,
};
use uuid::Uuid;

use super::Windows;
use crate::server::api::admin::workspace_health::queries::exclude_daemon_runs;

const ORGS_SQL: &str = "\
SELECT o.id, o.slug, o.name, o.created_at, \
       (SELECT COUNT(*) FROM org_members m WHERE m.org_id = o.id)::bigint AS members, \
       (SELECT COUNT(*) FROM workspaces w WHERE w.org_id = o.id)::bigint AS workspaces, \
       (SELECT MIN(w.created_at) FROM workspaces w WHERE w.org_id = o.id) AS first_workspace_at \
FROM organizations o";

const BILLING_SQL: &str = "\
SELECT org_id, status, seats_paid, grace_period_ends_at FROM org_billing";

const PARTNERS_SQL: &str = "SELECT managed_org_id, partner_org_id FROM partner_orgs";

const SIGN_INS_SQL: &str = "\
SELECT m.org_id, MAX(u.last_login_at) AS at \
FROM org_members m JOIN users u ON u.id = m.user_id \
GROUP BY m.org_id";

const APPS_SQL: &str = "SELECT id, org_id, slug, name, published_at FROM apps";

// Each app's last production open, staff's included — the app's "last active"
// as the admin apps list shows it. One probe of `(app_id, viewed_at DESC)` per
// app (`custom_apps_activity::last_active_at_by_app` answers the same with a
// `MAX` over every view, which the admin list's page of apps can afford).
const LAST_VIEWED_SQL: &str = "\
SELECT a.id AS app_id, last.viewed_at AS at \
FROM apps a \
CROSS JOIN LATERAL (SELECT v.viewed_at FROM custom_app_view_event v \
                    WHERE v.app_id = a.id AND v.environment = 'production' \
                    ORDER BY v.viewed_at DESC LIMIT 1) last";

// $1 the internal orgs, $2 the horizon's start (`LAST_ACTIVITY_HORIZON_DAYS`
// before now). Each org's latest CUSTOMER activity since $2: a production
// view by a non-staff person, a thread a non-staff person started, or a
// production invocation a non-staff person made. Staff opening a quiet
// customer's app, an unattended run (a schedule, a webhook: no user) and a
// thread nobody is attributed to never make the customer look active. Every
// branch stops at $2, so each probe reads at most the horizon's rows.
const LAST_ACTIVITY_SQL: &str = "\
WITH staff AS (SELECT DISTINCT user_id FROM org_members WHERE org_id = ANY($1::uuid[])) \
SELECT x.org_id, MAX(x.at) AS at FROM ( \
    SELECT a.org_id, lv.at FROM apps a \
    CROSS JOIN LATERAL (SELECT v.viewed_at AS at FROM custom_app_view_event v \
                        WHERE v.app_id = a.id AND v.environment = 'production' \
                          AND v.viewed_at >= $2 \
                          AND NOT EXISTS (SELECT 1 FROM staff s WHERE s.user_id = v.user_id) \
                        ORDER BY v.viewed_at DESC LIMIT 1) lv \
    UNION ALL \
    SELECT a.org_id, li.at FROM apps a \
    CROSS JOIN LATERAL (SELECT i.created_at AS at FROM app_function_invocations i \
                        WHERE i.app_id = a.id AND i.environment = 'production' \
                          AND i.created_at >= $2 AND i.user_id IS NOT NULL \
                          AND NOT EXISTS (SELECT 1 FROM staff s WHERE s.user_id = i.user_id) \
                        ORDER BY i.created_at DESC LIMIT 1) li \
    UNION ALL \
    SELECT w.org_id, MAX(t.created_at) AS at \
    FROM threads t JOIN workspaces w ON w.id = t.project_id \
    WHERE w.org_id IS NOT NULL AND t.user_id IS NOT NULL AND t.created_at >= $2 \
      AND NOT EXISTS (SELECT 1 FROM staff s WHERE s.user_id = t.user_id) \
    GROUP BY w.org_id \
) x GROUP BY x.org_id";

// The first promotion to production, or — for an app published before the
// event log — its `published_at`, which re-publishing only ever moves later.
const FIRST_PUBLISH_SQL: &str = "\
SELECT a.org_id, LEAST(MIN(e.at), MIN(a.published_at)) AS at \
FROM apps a LEFT JOIN app_environment_events e \
  ON e.app_id = a.id AND e.environment = 'production' AND e.action = 'promote' \
GROUP BY a.org_id";

const FIRST_THREAD_SQL: &str = "\
SELECT w.org_id, MIN(t.created_at) AS at \
FROM threads t JOIN workspaces w ON w.id = t.project_id \
WHERE w.org_id IS NOT NULL \
GROUP BY w.org_id";

/// The CTEs every windowed read starts with: the windows, numbered from 0, and
/// the staff (members of an internal org).
const WITH_WINDOWS: &str = "\
WITH win AS (SELECT w.starts_at, w.ends_at, w.idx - 1 AS idx \
             FROM unnest($1::timestamptz[], $2::timestamptz[]) \
                  WITH ORDINALITY AS w(starts_at, ends_at, idx)), \
     staff AS (SELECT DISTINCT user_id FROM org_members WHERE org_id = ANY($3::uuid[]))";

const VIEWS_SQL: &str = "\
SELECT a.org_id, win.idx, COUNT(*)::bigint AS events, 0::bigint AS failures, \
       COUNT(DISTINCT v.user_id) FILTER (WHERE s.user_id IS NULL)::bigint AS people \
FROM custom_app_view_event v \
JOIN apps a ON a.id = v.app_id \
JOIN win ON v.viewed_at >= win.starts_at AND v.viewed_at < win.ends_at \
LEFT JOIN staff s ON s.user_id = v.user_id \
WHERE v.app_id = ANY($6) AND v.environment = 'production' \
  AND v.viewed_at >= $4 AND v.viewed_at < $5 \
GROUP BY a.org_id, win.idx";

const THREADS_SQL: &str = "\
SELECT w.org_id, win.idx, COUNT(*)::bigint AS events, 0::bigint AS failures, \
       COUNT(DISTINCT t.user_id) FILTER (WHERE s.user_id IS NULL)::bigint AS people \
FROM threads t \
JOIN workspaces w ON w.id = t.project_id \
JOIN win ON t.created_at >= win.starts_at AND t.created_at < win.ends_at \
LEFT JOIN staff s ON s.user_id = t.user_id \
WHERE w.org_id IS NOT NULL AND t.created_at >= $4 AND t.created_at < $5 \
GROUP BY w.org_id, win.idx";

// A person with a production view, a thread, or both, counted once.
const ACTIVE_SQL: &str = "\
, acts AS ( \
    SELECT a.org_id, v.user_id, v.viewed_at AS at \
    FROM custom_app_view_event v JOIN apps a ON a.id = v.app_id \
    WHERE v.app_id = ANY($6) AND v.environment = 'production' \
      AND v.viewed_at >= $4 AND v.viewed_at < $5 \
    UNION ALL \
    SELECT w.org_id, t.user_id, t.created_at AS at \
    FROM threads t JOIN workspaces w ON w.id = t.project_id \
    WHERE w.org_id IS NOT NULL AND t.user_id IS NOT NULL \
      AND t.created_at >= $4 AND t.created_at < $5) \
SELECT acts.org_id, win.idx, 0::bigint AS events, 0::bigint AS failures, \
       COUNT(DISTINCT acts.user_id)::bigint AS people \
FROM acts \
JOIN win ON acts.at >= win.starts_at AND acts.at < win.ends_at \
WHERE NOT EXISTS (SELECT 1 FROM staff s WHERE s.user_id = acts.user_id) \
GROUP BY acts.org_id, win.idx";

// Top-level runs only: a delegated child is part of its parent's run. The
// customer-run predicate is appended (see `runs_sql`).
const RUNS_SQL: &str = "\
SELECT w.org_id, win.idx, COUNT(*)::bigint AS events, \
       COUNT(*) FILTER (WHERE r.task_status IN ('failed', 'timed_out'))::bigint AS failures, \
       0::bigint AS people \
FROM agentic_runs r \
JOIN workspaces w ON w.id = r.workspace_id \
JOIN win ON r.created_at >= win.starts_at AND r.created_at < win.ends_at \
WHERE w.org_id IS NOT NULL AND r.parent_run_id IS NULL \
  AND r.created_at >= $4 AND r.created_at < $5";

const CALLS_SQL: &str = "\
SELECT a.org_id, win.idx, COUNT(*)::bigint AS events, \
       COUNT(*) FILTER (WHERE i.status IN ('error', 'timeout'))::bigint AS failures, \
       0::bigint AS people \
FROM app_function_invocations i \
JOIN apps a ON a.id = i.app_id \
JOIN win ON i.created_at >= win.starts_at AND i.created_at < win.ends_at \
WHERE i.app_id = ANY($6) AND i.environment = 'production' \
  AND i.created_at >= $4 AND i.created_at < $5 \
GROUP BY a.org_id, win.idx";

const RELEASES_SQL: &str = "\
SELECT a.org_id, win.idx, COUNT(*)::bigint AS events, 0::bigint AS failures, \
       0::bigint AS people \
FROM app_environment_events e \
JOIN apps a ON a.id = e.app_id \
JOIN win ON e.at >= win.starts_at AND e.at < win.ends_at \
WHERE e.app_id = ANY($6) AND e.environment = 'production' AND e.action = 'promote' \
  AND e.at >= $4 AND e.at < $5 \
GROUP BY a.org_id, win.idx";

#[derive(Clone, Debug, FromQueryResult)]
pub(super) struct OrgRow {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub members: i64,
    pub workspaces: i64,
    pub first_workspace_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, FromQueryResult)]
pub(super) struct BillingRow {
    pub org_id: Uuid,
    pub status: String,
    pub seats_paid: i32,
    pub grace_period_ends_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, FromQueryResult)]
pub(super) struct PartnerRow {
    pub managed_org_id: Uuid,
    pub partner_org_id: Uuid,
}

/// An instant per org (a sign-in, a first publish, a last activity).
#[derive(Clone, Debug, FromQueryResult)]
pub(super) struct OrgAtRow {
    pub org_id: Uuid,
    pub at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, FromQueryResult)]
pub(super) struct AppRow {
    pub id: Uuid,
    pub org_id: Uuid,
    pub slug: String,
    pub name: String,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, FromQueryResult)]
pub(super) struct AppAtRow {
    pub app_id: Uuid,
    pub at: DateTime<Utc>,
}

/// One org's count in one window. Each read fills the columns it has and
/// selects 0 for the rest.
#[derive(Clone, Debug, Default, FromQueryResult)]
pub(super) struct WindowRow {
    pub org_id: Uuid,
    pub idx: i64,
    pub events: i64,
    pub failures: i64,
    pub people: i64,
}

/// Everything the reads returned, before it is shaped into orgs.
#[derive(Default)]
pub(super) struct Sources {
    pub orgs: Vec<OrgRow>,
    pub billing: Vec<BillingRow>,
    pub partners: Vec<PartnerRow>,
    pub sign_ins: Vec<OrgAtRow>,
    pub apps: Vec<AppRow>,
    /// Each app's newest production view, staff's included.
    pub last_viewed: Vec<AppAtRow>,
    /// Each org's newest customer activity.
    pub last_activity: Vec<OrgAtRow>,
    pub first_publishes: Vec<OrgAtRow>,
    pub first_threads: Vec<OrgAtRow>,
    pub views: Vec<WindowRow>,
    pub threads: Vec<WindowRow>,
    pub active: Vec<WindowRow>,
    pub runs: Vec<WindowRow>,
    pub calls: Vec<WindowRow>,
    pub releases: Vec<WindowRow>,
}

/// Every source, read fresh from one snapshot. `staff_orgs` is the internal orgs.
pub(super) async fn read_all(
    db: &DatabaseConnection,
    staff_orgs: &[Uuid],
    windows: &Windows,
    activity_since: DateTime<Utc>,
) -> Result<Sources, DbErr> {
    let txn = db
        .begin_with_config(
            Some(IsolationLevel::RepeatableRead),
            Some(AccessMode::ReadOnly),
        )
        .await?;
    // An error drops `txn`, which rolls it back.
    let sources = read_sources(&txn, staff_orgs, windows, activity_since).await?;
    txn.commit().await?;
    Ok(sources)
}

async fn read_sources(
    db: &DatabaseTransaction,
    staff_orgs: &[Uuid],
    windows: &Windows,
    activity_since: DateTime<Utc>,
) -> Result<Sources, DbErr> {
    let apps: Vec<AppRow> = rows(db, APPS_SQL, vec![]).await?;
    let app_ids: Vec<Uuid> = apps.iter().map(|a| a.id).collect();
    let windowed = || -> Vec<Value> {
        vec![
            windows.starts.clone().into(),
            windows.ends.clone().into(),
            staff_orgs.to_vec().into(),
            windows.lo().into(),
            windows.hi().into(),
            app_ids.clone().into(),
        ]
    };
    Ok(Sources {
        orgs: rows(db, ORGS_SQL, vec![]).await?,
        billing: rows(db, BILLING_SQL, vec![]).await?,
        partners: rows(db, PARTNERS_SQL, vec![]).await?,
        sign_ins: rows(db, SIGN_INS_SQL, vec![]).await?,
        last_viewed: rows(db, LAST_VIEWED_SQL, vec![]).await?,
        last_activity: rows(
            db,
            LAST_ACTIVITY_SQL,
            vec![staff_orgs.to_vec().into(), activity_since.into()],
        )
        .await?,
        first_publishes: rows(db, FIRST_PUBLISH_SQL, vec![]).await?,
        first_threads: rows(db, FIRST_THREAD_SQL, vec![]).await?,
        views: rows(db, &with_windows(VIEWS_SQL), windowed()).await?,
        threads: rows(db, &with_windows(THREADS_SQL), windowed()).await?,
        active: rows(db, &format!("{WITH_WINDOWS} {ACTIVE_SQL}"), windowed()).await?,
        runs: rows(db, &with_windows(&runs_sql()), windowed()).await?,
        calls: rows(db, &with_windows(CALLS_SQL), windowed()).await?,
        releases: rows(db, &with_windows(RELEASES_SQL), windowed()).await?,
        apps,
    })
}

fn with_windows(body: &str) -> String {
    format!("{WITH_WINDOWS} {body}")
}

/// [`RUNS_SQL`] with the customer-run predicate workspace health counts by: no
/// platform daemon (pre-aggregation, health evals, previews, sandbox upkeep)
/// and no staff dry run. Shared, so the two cannot drift.
fn runs_sql() -> String {
    format!(
        "{RUNS_SQL}{} GROUP BY w.org_id, win.idx",
        exclude_daemon_runs("r.")
    )
}

async fn rows<T: FromQueryResult>(
    db: &impl ConnectionTrait,
    sql: &str,
    values: Vec<Value>,
) -> Result<Vec<T>, DbErr> {
    let statement = Statement::from_sql_and_values(db.get_database_backend(), sql, values);
    T::find_by_statement(statement).all(db).await
}
