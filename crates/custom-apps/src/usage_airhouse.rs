//! The Airhouse sink behind the custom-app usage drain (W2.3b).
//!
//! A page view and a platform-beacon event are *facts* — something happened and
//! will not change — so `data-placement.md` puts them in the workspace's
//! Airhouse, not in Oxy's Postgres. The drain (`custom_apps_usage_queue`) still
//! writes the Postgres rows the Activity tab and the weekly usage report read
//! today, and then hands the same batch to [`publish`], which appends it to
//! `oxy_custom_app_usage` in each workspace's Airhouse. Nothing that reads
//! usage has moved yet; this is the write half, so that history starts
//! accumulating in the data plane before the reads follow.
//!
//! # Best effort, behind the Postgres write
//!
//! * Postgres is written first. [`publish`] runs after both inserts of a batch
//!   have finished, so an Airhouse problem cannot delay or fail them.
//! * A workspace whose Airhouse cannot be reached costs `tracing::warn!` and
//!   that batch's Airhouse rows for that workspace; the drain moves on. Each
//!   append is bounded by [`APPEND_TIMEOUT`], and a failing workspace is left
//!   alone for [`FAILURE_BACKOFF`] so a dead Airhouse costs the drain one
//!   timeout per window, not one per 500 ms tick. The rows skipped meanwhile are
//!   counted ([`failed_total`]) and not retried: Postgres still has them.
//! * An org without a provisioned Airhouse (or a deployment with the integration
//!   switched off, which is every local dev box) is skipped silently.
//!
//! # Idempotent on the event id
//!
//! The id is minted once per row and written to both stores, so the two copies
//! of a fact share it. DuckLake has no `PRIMARY KEY` or `UNIQUE`, so the insert
//! itself refuses a duplicate: [`insert_sql`] appends only the rows whose
//! `event_id` is not already there, looking back no further than the oldest
//! `occurred_at` in the batch (so the check prunes by time instead of reading
//! the table). Replaying a batch therefore appends nothing. Readers should still
//! keep one row per id (`QUALIFY row_number() OVER (PARTITION BY event_id ORDER
//! BY recorded_at DESC) = 1`), because two replicas racing the same id could
//! both pass the check.
//!
//! # Table
//!
//! One table for both kinds of fact, plain columns and no keys or indexes (a
//! table carrying one fails in DuckLake and leaves its writer inert). It is
//! created with `CREATE TABLE IF NOT EXISTS` as Admin the first time a
//! workspace is written to in this process; a Writer cannot create tables.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, RwLock};
use std::time::{Duration, Instant};

use airhouse::entity::tenants::{self, TenantStatus};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use entity::{custom_app_event, custom_app_view_event};
use oxy_cameras::airhouse::escape::{sql_opt_str, sql_str, sql_ts};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect};
use uuid::Uuid;

/// The workspace table both kinds of fact are appended to.
pub const TABLE: &str = "oxy_custom_app_usage";

/// `kind` of a page-view fact (a `custom_app_view_event` row).
pub const KIND_VIEW: &str = "view";
/// `kind` of an event fact (a `custom_app_event` row).
pub const KIND_EVENT: &str = "event";

/// DDL for [`TABLE`]. No `PRIMARY KEY`, `UNIQUE`, index or foreign key:
/// DuckLake rejects them, and a table that carries one fails and leaves its
/// writer inert. `event_id` is therefore a plain column, and [`insert_sql`]
/// enforces its uniqueness at write time.
pub const CREATE_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS oxy_custom_app_usage (
    event_id         VARCHAR     NOT NULL,
    kind             VARCHAR     NOT NULL,
    occurred_at      TIMESTAMPTZ NOT NULL,
    recorded_at      TIMESTAMPTZ NOT NULL,
    org_id           VARCHAR     NOT NULL,
    app_id           VARCHAR     NOT NULL,
    environment      VARCHAR     NOT NULL,
    user_id          VARCHAR     NOT NULL,
    user_email       VARCHAR,
    session_id       VARCHAR     NOT NULL,
    event_name       VARCHAR,
    payload          VARCHAR,
    referrer         VARCHAR,
    user_agent_class VARCHAR,
    source           VARCHAR,
    app_role         VARCHAR,
    org_role         VARCHAR
)
"#;

/// Column order of [`CREATE_TABLE`], as written by [`insert_sql`].
const COLUMNS: [&str; 17] = [
    "event_id",
    "kind",
    "occurred_at",
    "recorded_at",
    "org_id",
    "app_id",
    "environment",
    "user_id",
    "user_email",
    "session_id",
    "event_name",
    "payload",
    "referrer",
    "user_agent_class",
    "source",
    "app_role",
    "org_role",
];

/// Longest one append (connect, table check and insert) may take.
pub const APPEND_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a workspace whose append failed is left alone.
pub const FAILURE_BACKOFF: Duration = Duration::from_secs(30);
/// How long "this workspace has (no) Airhouse tenant" is remembered.
const TENANT_CACHE_TTL: Duration = Duration::from_secs(60);
/// Most characters of an Airhouse error put in a log line.
const ERROR_LOG_MAX: usize = 500;

/// One usage fact, as it is appended to Airhouse. `view`-only and `event`-only
/// columns are `None` for the other kind.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageFact {
    /// Shared with the Postgres row; the idempotency key.
    pub event_id: Uuid,
    pub kind: &'static str,
    /// When it happened (the navigation or beacon), not when it was written.
    pub occurred_at: DateTime<Utc>,
    /// When the drain wrote it.
    pub recorded_at: DateTime<Utc>,
    pub org_id: Uuid,
    pub app_id: Uuid,
    pub environment: String,
    pub user_id: Uuid,
    pub user_email: String,
    pub session_id: Uuid,
    pub event_name: Option<String>,
    /// The event's JSON payload, serialized.
    pub payload: Option<String>,
    pub referrer: Option<String>,
    pub user_agent_class: Option<String>,
    pub source: Option<String>,
    pub app_role: Option<String>,
    pub org_role: Option<String>,
}

impl UsageFact {
    /// The fact for the `custom_app_view_event` row `row`. `org_id` is the
    /// owning org of the app.
    pub fn from_view(
        row: &custom_app_view_event::ActiveModel,
        org_id: Uuid,
        recorded_at: DateTime<Utc>,
    ) -> Option<Self> {
        Some(Self {
            event_id: *row.id.try_as_ref()?,
            kind: KIND_VIEW,
            occurred_at: row.viewed_at.try_as_ref()?.with_timezone(&Utc),
            recorded_at,
            org_id,
            app_id: *row.app_id.try_as_ref()?,
            environment: row.environment.try_as_ref()?.clone(),
            user_id: *row.user_id.try_as_ref()?,
            user_email: row.user_email.try_as_ref()?.clone(),
            session_id: *row.session_id.try_as_ref()?,
            event_name: None,
            payload: None,
            referrer: row.referrer.try_as_ref()?.clone(),
            user_agent_class: Some(row.user_agent_class.try_as_ref()?.clone()),
            source: Some(row.source.try_as_ref()?.clone()),
            app_role: row.app_role.try_as_ref()?.clone(),
            org_role: row.org_role.try_as_ref()?.clone(),
        })
    }

    /// The fact for the `custom_app_event` row `row`.
    pub fn from_event(
        row: &custom_app_event::ActiveModel,
        org_id: Uuid,
        recorded_at: DateTime<Utc>,
    ) -> Option<Self> {
        Some(Self {
            event_id: *row.id.try_as_ref()?,
            kind: KIND_EVENT,
            occurred_at: row.occurred_at.try_as_ref()?.with_timezone(&Utc),
            recorded_at,
            org_id,
            app_id: *row.app_id.try_as_ref()?,
            environment: row.environment.try_as_ref()?.clone(),
            user_id: *row.user_id.try_as_ref()?,
            user_email: row.user_email.try_as_ref()?.clone(),
            session_id: *row.session_id.try_as_ref()?,
            event_name: Some(row.event_name.try_as_ref()?.clone()),
            payload: Some(row.payload.try_as_ref()?.to_string()),
            referrer: None,
            user_agent_class: None,
            source: None,
            app_role: None,
            org_role: None,
        })
    }

    /// `(…)` values in [`COLUMNS`] order.
    fn values_tuple(&self) -> String {
        let id = self.event_id.to_string();
        let org = self.org_id.to_string();
        let app = self.app_id.to_string();
        let user = self.user_id.to_string();
        let session = self.session_id.to_string();
        let cells = [
            sql_str(&id),
            sql_str(self.kind),
            ts_literal(self.occurred_at),
            ts_literal(self.recorded_at),
            sql_str(&org),
            sql_str(&app),
            sql_str(&self.environment),
            sql_str(&user),
            sql_str(&self.user_email),
            sql_str(&session),
            sql_opt_str(self.event_name.as_deref()),
            sql_opt_str(self.payload.as_deref()),
            sql_opt_str(self.referrer.as_deref()),
            sql_opt_str(self.user_agent_class.as_deref()),
            sql_opt_str(self.source.as_deref()),
            sql_opt_str(self.app_role.as_deref()),
            sql_opt_str(self.org_role.as_deref()),
        ];
        format!("({})", cells.join(", "))
    }
}

/// `TIMESTAMPTZ '2026-10-20T01:02:03.456+00:00'`.
fn ts_literal(t: DateTime<Utc>) -> String {
    format!("TIMESTAMPTZ {}", sql_ts(t))
}

/// The single statement that appends `facts`, skipping any whose `event_id` is
/// already in the table. `None` for an empty slice.
///
/// The existing-row check only looks at rows at or after the batch's oldest
/// `occurred_at`: a replay carries the same ids with the same `occurred_at`, so
/// the window is exact for it, and it lets DuckLake prune files instead of
/// scanning the table.
pub fn insert_sql(facts: &[UsageFact]) -> Option<String> {
    let oldest = facts.iter().map(|f| f.occurred_at).min()?;
    let columns = COLUMNS.join(", ");
    let values = facts
        .iter()
        .map(UsageFact::values_tuple)
        .collect::<Vec<_>>()
        .join(",\n  ");
    Some(format!(
        "INSERT INTO {TABLE} ({columns})\n\
         SELECT {columns} FROM (VALUES\n  {values}\n) AS v({columns})\n\
         WHERE NOT EXISTS (\n  \
             SELECT 1 FROM {TABLE} AS t\n  \
             WHERE t.occurred_at >= {since} AND t.event_id = v.event_id\n\
         )",
        since = ts_literal(oldest),
    ))
}

/// Why an append did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SinkError {
    /// This workspace has no Airhouse (no tenant, or the integration is off).
    /// Expected; never logged.
    NotConfigured,
    /// The workspace has an Airhouse and it could not be written. Logged.
    Failed(String),
}

/// Where usage facts go besides Postgres. Production is [`AirhouseUsageSink`];
/// tests install a fake with [`install_sink`].
#[async_trait]
pub trait UsageSink: Send + Sync {
    /// Whether any workspace could be written to at all. A cheap check the
    /// drain makes first, so a deployment without Airhouse does no extra work
    /// (not even the workspace lookup for events).
    fn enabled(&self) -> bool;

    /// Append `facts` to `workspace_id`'s Airhouse.
    async fn append(
        &self,
        db: &DatabaseConnection,
        workspace_id: Uuid,
        facts: &[UsageFact],
    ) -> Result<(), SinkError>;
}

static SINK: LazyLock<RwLock<Arc<dyn UsageSink>>> =
    LazyLock::new(|| RwLock::new(Arc::new(AirhouseUsageSink::default())));

/// Replace the process's sink. For tests, which have no Airhouse to write to.
pub fn install_sink(sink: Arc<dyn UsageSink>) {
    *SINK.write().unwrap_or_else(|e| e.into_inner()) = sink;
}

fn current_sink() -> Arc<dyn UsageSink> {
    SINK.read().unwrap_or_else(|e| e.into_inner()).clone()
}

static APPENDED: AtomicU64 = AtomicU64::new(0);
static FAILED: AtomicU64 = AtomicU64::new(0);

/// Facts appended to Airhouse since process start.
pub fn appended_total() -> u64 {
    APPENDED.load(Ordering::Relaxed)
}

/// Facts that were not appended because Airhouse could not be reached (or the
/// workspace was in its failure backoff), since process start.
pub fn failed_total() -> u64 {
    FAILED.load(Ordering::Relaxed)
}

/// Workspace → when it may be tried again after a failed append.
static BACKOFF: LazyLock<Mutex<HashMap<Uuid, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn in_backoff(workspace_id: Uuid, now: Instant) -> bool {
    let mut map = BACKOFF.lock().unwrap_or_else(|e| e.into_inner());
    match map.get(&workspace_id) {
        Some(until) if *until > now => true,
        Some(_) => {
            map.remove(&workspace_id);
            false
        }
        None => false,
    }
}

fn start_backoff(workspace_id: Uuid, now: Instant) {
    BACKOFF
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(workspace_id, now + FAILURE_BACKOFF);
}

/// Whether the drain has anywhere to send facts. Checked once per batch.
pub fn enabled() -> bool {
    current_sink().enabled()
}

/// The owning workspace and org of each app in `app_ids`, for the beacon
/// events (a queued event carries only its app id). Apps that no longer exist
/// are absent.
pub async fn app_homes(db: &DatabaseConnection, app_ids: Vec<Uuid>) -> HashMap<Uuid, (Uuid, Uuid)> {
    if app_ids.is_empty() {
        return HashMap::new();
    }
    match entity::apps::Entity::find()
        .select_only()
        .columns([
            entity::apps::Column::Id,
            entity::apps::Column::ProjectId,
            entity::apps::Column::OrgId,
        ])
        .filter(entity::apps::Column::Id.is_in(app_ids))
        .into_tuple::<(Uuid, Uuid, Uuid)>()
        .all(db)
        .await
    {
        Ok(rows) => rows
            .into_iter()
            .map(|(app, workspace, org)| (app, (workspace, org)))
            .collect(),
        Err(e) => {
            tracing::warn!(
                "custom-app usage airhouse: could not resolve workspaces for events: {e}"
            );
            HashMap::new()
        }
    }
}

/// Append a batch's facts to each workspace's Airhouse. Never fails and never
/// panics: it is called after the Postgres write, and whatever happens here
/// must leave that write alone.
///
/// Workspaces are written concurrently, each bounded by [`APPEND_TIMEOUT`].
pub async fn publish(db: &DatabaseConnection, facts: Vec<(Uuid, UsageFact)>) {
    if facts.is_empty() {
        return;
    }
    let sink = current_sink();
    if !sink.enabled() {
        return;
    }
    let mut by_workspace: HashMap<Uuid, Vec<UsageFact>> = HashMap::new();
    for (workspace_id, fact) in facts {
        by_workspace.entry(workspace_id).or_default().push(fact);
    }
    let now = Instant::now();
    let writes = by_workspace
        .into_iter()
        .filter_map(|(workspace_id, facts)| {
            if in_backoff(workspace_id, now) {
                FAILED.fetch_add(facts.len() as u64, Ordering::Relaxed);
                return None;
            }
            Some((workspace_id, facts))
        })
        .map(|(workspace_id, facts)| {
            let sink = Arc::clone(&sink);
            async move {
                let rows = facts.len() as u64;
                let outcome =
                    tokio::time::timeout(APPEND_TIMEOUT, sink.append(db, workspace_id, &facts))
                        .await
                        .unwrap_or_else(|_| {
                            Err(SinkError::Failed(format!(
                                "append timed out after {}s",
                                APPEND_TIMEOUT.as_secs()
                            )))
                        });
                (workspace_id, rows, outcome)
            }
        });
    for (workspace_id, rows, outcome) in futures::future::join_all(writes).await {
        match outcome {
            Ok(()) => {
                APPENDED.fetch_add(rows, Ordering::Relaxed);
            }
            Err(SinkError::NotConfigured) => {}
            Err(SinkError::Failed(reason)) => {
                FAILED.fetch_add(rows, Ordering::Relaxed);
                start_backoff(workspace_id, Instant::now());
                tracing::warn!(
                    %workspace_id,
                    rows,
                    failed_total = failed_total(),
                    "custom-app usage drain: Airhouse append failed; the Postgres rows are kept \
                     and this workspace is skipped for {}s: {reason}",
                    FAILURE_BACKOFF.as_secs()
                );
            }
        }
    }
}

// ── Production sink ─────────────────────────────────────────────────────────

/// Appends through the camera fleet's persistent per-tenant Airhouse clients
/// (`oxy_cameras::airhouse`), minted as [`airhouse::SystemPurpose::UsageIngest`].
#[derive(Default)]
pub struct AirhouseUsageSink {
    /// Workspace → (checked at, has an active tenant).
    tenants: Mutex<HashMap<Uuid, (Instant, bool)>>,
    /// One cell per workspace, filled once its table has been created or found.
    /// The map lock is held only to fetch the cell, never across network I/O,
    /// so a workspace whose Airhouse hangs delays no other workspace; the cell
    /// itself single-flights the first create, and stays empty (so the next
    /// batch retries) when the create fails or is cut off by the append timeout.
    ensured: Mutex<HashMap<Uuid, Arc<tokio::sync::OnceCell<()>>>>,
}

impl AirhouseUsageSink {
    async fn has_tenant(
        &self,
        db: &DatabaseConnection,
        workspace_id: Uuid,
    ) -> Result<bool, SinkError> {
        if let Some((at, has)) = self
            .tenants
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&workspace_id)
            .copied()
            && at.elapsed() < TENANT_CACHE_TTL
        {
            return Ok(has);
        }
        let has = tenants::Entity::find()
            .filter(tenants::Column::WorkspaceId.eq(workspace_id))
            .filter(tenants::Column::Status.eq(TenantStatus::Active))
            .one(db)
            .await
            .map_err(|e| SinkError::Failed(format!("tenant lookup failed: {e}")))?
            .is_some();
        self.tenants
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(workspace_id, (Instant::now(), has));
        Ok(has)
    }

    async fn ensure_table(&self, workspace_id: Uuid) -> Result<(), SinkError> {
        let cell = Arc::clone(
            self.ensured
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .entry(workspace_id)
                .or_default(),
        );
        cell.get_or_try_init(|| async {
            let admin = oxy_cameras::airhouse::connect(
                workspace_id,
                airhouse::UserRole::Admin,
                airhouse::SystemPurpose::UsageIngest,
                oxy_cameras::airhouse::ingest_ttl(),
            )
            .await
            .map_err(|e| SinkError::Failed(truncate(e.to_string())))?;
            admin
                .simple_query(CREATE_TABLE)
                .await
                .map_err(|e| SinkError::Failed(truncate(pg_text(&e))))?;
            Ok(())
        })
        .await
        .map(|_| ())
    }
}

#[async_trait]
impl UsageSink for AirhouseUsageSink {
    fn enabled(&self) -> bool {
        airhouse::token_broker().is_some()
    }

    async fn append(
        &self,
        db: &DatabaseConnection,
        workspace_id: Uuid,
        facts: &[UsageFact],
    ) -> Result<(), SinkError> {
        let Some(sql) = insert_sql(facts) else {
            return Ok(());
        };
        if !self.has_tenant(db, workspace_id).await? {
            return Err(SinkError::NotConfigured);
        }
        self.ensure_table(workspace_id).await?;
        let client = oxy_cameras::airhouse::client::tenant_client(
            workspace_id,
            airhouse::UserRole::Writer,
            airhouse::SystemPurpose::UsageIngest,
            oxy_cameras::airhouse::ingest_ttl(),
            oxy_cameras::airhouse::client::Pool::Serving,
        )
        .await
        .map_err(|e| SinkError::Failed(truncate(e.to_string())))?;
        client
            .simple_query(&sql)
            .await
            .map_err(|e| SinkError::Failed(truncate(pg_text(&e))))?;
        Ok(())
    }
}

/// The server's message with its SQLSTATE, not `tokio_postgres`'s "db error".
/// `DETAIL` is left out: Postgres puts the offending row there.
fn pg_text(e: &tokio_postgres::Error) -> String {
    match e.as_db_error() {
        Some(db) => format!("[{}] {}", db.code().code(), db.message()),
        None => e.to_string(),
    }
}

fn truncate(mut text: String) -> String {
    if text.len() > ERROR_LOG_MAX {
        let cut = (0..=ERROR_LOG_MAX)
            .rev()
            .find(|i| text.is_char_boundary(*i))
            .unwrap_or(0);
        text.truncate(cut);
        text.push('…');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use sea_orm::ActiveValue::Set;

    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_790_000_000 + secs, 0).unwrap()
    }

    fn view_row() -> custom_app_view_event::ActiveModel {
        custom_app_view_event::ActiveModel {
            id: Set(Uuid::from_u128(1)),
            app_id: Set(Uuid::from_u128(2)),
            user_id: Set(Uuid::from_u128(3)),
            user_email: Set("o'brien@example.com".into()),
            session_id: Set(Uuid::from_u128(4)),
            viewed_at: Set(at(0).fixed_offset()),
            referrer: Set(Some("/orders".into())),
            user_agent_class: Set("browser".into()),
            source: Set("subpath".into()),
            app_role: Set(Some("admin".into())),
            org_role: Set(None),
            environment: Set("production".into()),
        }
    }

    fn event_row() -> custom_app_event::ActiveModel {
        custom_app_event::ActiveModel {
            id: Set(Uuid::from_u128(5)),
            app_id: Set(Uuid::from_u128(2)),
            user_id: Set(Uuid::from_u128(3)),
            user_email: Set("a@example.com".into()),
            session_id: Set(Uuid::from_u128(4)),
            event_name: Set("oxy-pageview".into()),
            payload: Set(serde_json::json!({ "path": "/orders" })),
            occurred_at: Set(at(10).fixed_offset()),
            environment: Set("staging".into()),
        }
    }

    #[test]
    fn a_view_row_maps_to_a_view_fact_with_the_rows_own_id() {
        let org = Uuid::from_u128(9);
        let fact = UsageFact::from_view(&view_row(), org, at(60)).unwrap();
        assert_eq!(fact.event_id, Uuid::from_u128(1));
        assert_eq!(fact.kind, KIND_VIEW);
        assert_eq!(
            fact.occurred_at,
            at(0),
            "the navigation's time, not the write's"
        );
        assert_eq!(fact.recorded_at, at(60));
        assert_eq!(fact.org_id, org);
        assert_eq!(fact.app_id, Uuid::from_u128(2));
        assert_eq!(fact.referrer.as_deref(), Some("/orders"));
        assert_eq!(fact.user_agent_class.as_deref(), Some("browser"));
        assert_eq!(fact.app_role.as_deref(), Some("admin"));
        assert_eq!(fact.org_role, None, "an unrecorded role stays unrecorded");
        assert_eq!((fact.event_name, fact.payload), (None, None));
    }

    #[test]
    fn an_event_row_maps_to_an_event_fact_carrying_its_payload() {
        let fact = UsageFact::from_event(&event_row(), Uuid::from_u128(9), at(60)).unwrap();
        assert_eq!(fact.event_id, Uuid::from_u128(5));
        assert_eq!(fact.kind, KIND_EVENT);
        assert_eq!(fact.occurred_at, at(10));
        assert_eq!(fact.environment, "staging");
        assert_eq!(fact.event_name.as_deref(), Some("oxy-pageview"));
        assert_eq!(fact.payload.as_deref(), Some(r#"{"path":"/orders"}"#));
        assert_eq!(
            (fact.referrer, fact.app_role, fact.org_role),
            (None, None, None)
        );
    }

    #[test]
    fn a_row_with_an_unset_column_is_not_mapped_rather_than_panicking() {
        let mut row = view_row();
        row.id = sea_orm::ActiveValue::NotSet;
        assert!(UsageFact::from_view(&row, Uuid::nil(), at(0)).is_none());
    }

    #[test]
    fn the_insert_is_keyed_on_the_event_id_and_looks_back_to_the_oldest_row() {
        let org = Uuid::from_u128(9);
        let facts = vec![
            UsageFact::from_event(&event_row(), org, at(60)).unwrap(),
            UsageFact::from_view(&view_row(), org, at(60)).unwrap(),
        ];
        let sql = insert_sql(&facts).unwrap();
        // The id of each row is what a replay is matched on.
        assert!(sql.contains(&format!("'{}'", Uuid::from_u128(1))));
        assert!(sql.contains(&format!("'{}'", Uuid::from_u128(5))));
        assert!(sql.contains("WHERE NOT EXISTS"));
        assert!(sql.contains("t.event_id = v.event_id"));
        // Looks back from the batch's oldest occurred_at (the view, at(0)).
        assert!(sql.contains(&format!(
            "t.occurred_at >= TIMESTAMPTZ '{}'",
            at(0).to_rfc3339()
        )));
        // Quotes are doubled; absent columns are NULL, not empty strings.
        assert!(sql.contains("'o''brien@example.com'"));
        assert!(sql.contains("NULL"));
        assert!(insert_sql(&[]).is_none());
    }

    #[test]
    fn the_same_batch_always_renders_the_same_statement() {
        let facts = vec![UsageFact::from_view(&view_row(), Uuid::nil(), at(60)).unwrap()];
        assert_eq!(insert_sql(&facts), insert_sql(&facts));
    }

    /// The DDL is what a DuckLake writer is made inert by.
    #[test]
    fn the_table_declares_no_keys_or_indexes() {
        let ddl = CREATE_TABLE.to_ascii_uppercase();
        for forbidden in [
            "PRIMARY KEY",
            "UNIQUE",
            "INDEX",
            "REFERENCES",
            "FOREIGN KEY",
        ] {
            assert!(
                !ddl.contains(forbidden),
                "{forbidden} in the usage table DDL"
            );
        }
        for column in COLUMNS {
            assert!(
                CREATE_TABLE.contains(column),
                "{column} missing from the DDL"
            );
        }
    }

    #[test]
    fn values_tuple_has_one_cell_per_column() {
        let fact = UsageFact::from_view(&view_row(), Uuid::nil(), at(0)).unwrap();
        let tuple = fact.values_tuple();
        // No cell in this fixture contains a comma, so cells = commas + 1.
        let mut fact = fact;
        fact.user_email = "a@example.com".into();
        fact.referrer = None;
        let tuple_commas = fact.values_tuple().matches(", ").count();
        assert_eq!(tuple_commas + 1, COLUMNS.len());
        assert!(tuple.starts_with('(') && tuple.ends_with(')'));
    }

    #[test]
    fn long_errors_are_cut_on_a_character_boundary() {
        let cut = truncate("é".repeat(ERROR_LOG_MAX));
        assert!(cut.ends_with('…'));
        assert!(cut.len() <= ERROR_LOG_MAX + '…'.len_utf8());
    }
}
