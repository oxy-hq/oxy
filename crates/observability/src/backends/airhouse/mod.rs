//! Airhouse-backed implementation of [`crate::store::ObservabilityStore`].
//!
//! Connects to an Airhouse instance via the pgwire protocol (same transport as
//! the `AirhouseConnector` in the `airhouse` crate) and executes DuckDB-dialect
//! SQL against the `oxy_obs_*` observability tables.
//!
//! # Configuration
//!
//! | Variable | Default | Description |
//! |---|---|---|
//! | `AIRHOUSE_WIRE_HOST` | — | Airhouse wire-protocol host (required) |
//! | `AIRHOUSE_WIRE_PORT` | `5445` | Airhouse wire-protocol port |
//! | `OXY_AIRHOUSE_OBS_USER` | — | pgwire username (required) |
//! | `OXY_AIRHOUSE_OBS_PASSWORD` | — | pgwire password (required) |
//! | `OXY_AIRHOUSE_OBS_DATABASE` | — | Tenant/database name (required) |
//! | `OXY_AIRHOUSE_OBS_INSECURE` | unset | Set to `true` to skip TLS (use only on localhost) |
//!
//! Credentials are supplied to [`AirhouseObservabilityStorage::connect`] via a
//! [`CredentialFn`] callback that is re-invoked on every reconnect. The
//! standard wiring in `oxy-app` uses [`credentials_from_env`] to read the
//! three `OXY_AIRHOUSE_OBS_*` vars; the callback shape supports swapping in a
//! refresh-on-mint provider once a dedicated observability tenant exists in
//! the airhouse control plane. The SA-backed token broker is **not** a valid
//! source here: it mints per-(workspace, subject, role) credentials and
//! observability has no workspace context.
//!
//! # SQL approach
//!
//! Airhouse speaks DuckDB SQL over the pgwire simple-query protocol. There are
//! no prepared statements or `$N` placeholders — every value is embedded
//! directly in the SQL string. Use [`esc`] to escape any user-controlled string
//! value before embedding it.

mod execution_analytics;
mod intents;
mod metrics;
pub mod schema;
mod traces;

use std::fmt::Write as _;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::RwLock;
use tokio_postgres::{Client, NoTls, SimpleQueryMessage};
use tokio_postgres_rustls::MakeRustlsConnect;

use oxy_shared::errors::OxyError;

use crate::intent_types::IntentCluster;
use crate::store::ObservabilityStore;
use crate::types::{
    AgentExecutionStatsData, ClusterInfoRow, ClusterMapDataRow, ExecutionListData,
    ExecutionSummaryData, ExecutionTimeBucketData, IntentAnalyticsRow, MetricAnalyticsData,
    MetricDetailData, MetricUsageRecord, MetricsListData, SpanRecord, TraceDetailRow,
    TraceEnrichmentRow, TraceRow,
};

// ── Credential provider ───────────────────────────────────────────────────────

/// Async callback that returns `(user, password, database)`.
///
/// Called once at construction and again on every reconnect so ephemeral
/// (token-broker-minted) credentials are transparently refreshed.
pub type CredentialFn = Arc<
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<(String, String, String), OxyError>> + Send>>
        + Send
        + Sync,
>;

/// Build a [`CredentialFn`] that reads `OXY_AIRHOUSE_OBS_USER`,
/// `OXY_AIRHOUSE_OBS_PASSWORD`, and `OXY_AIRHOUSE_OBS_DATABASE` from the
/// process environment on every call. Returns `None` if any of the three are
/// missing or empty so the caller can surface a clear "not configured" error
/// before constructing the storage.
pub fn credentials_from_env() -> Option<CredentialFn> {
    fn read(var: &str) -> Option<String> {
        std::env::var(var).ok().filter(|v| !v.is_empty())
    }
    // Validate eagerly so callers fail fast at boot rather than at first
    // reconnect.
    read("OXY_AIRHOUSE_OBS_USER")?;
    read("OXY_AIRHOUSE_OBS_PASSWORD")?;
    read("OXY_AIRHOUSE_OBS_DATABASE")?;
    Some(Arc::new(|| {
        Box::pin(async move {
            let user = read("OXY_AIRHOUSE_OBS_USER").ok_or_else(|| {
                OxyError::ConfigurationError("OXY_AIRHOUSE_OBS_USER is not set or is empty".into())
            })?;
            let password = read("OXY_AIRHOUSE_OBS_PASSWORD").ok_or_else(|| {
                OxyError::ConfigurationError(
                    "OXY_AIRHOUSE_OBS_PASSWORD is not set or is empty".into(),
                )
            })?;
            let database = read("OXY_AIRHOUSE_OBS_DATABASE").ok_or_else(|| {
                OxyError::ConfigurationError(
                    "OXY_AIRHOUSE_OBS_DATABASE is not set or is empty".into(),
                )
            })?;
            Ok((user, password, database))
        })
    }))
}

// ── Storage struct ────────────────────────────────────────────────────────────

/// A boxed `Connection` future (erased TLS-stream type for reconnect reuse).
type BoxConn = Pin<Box<dyn Future<Output = Result<(), tokio_postgres::Error>> + Send + 'static>>;

pub struct AirhouseObservabilityStorage {
    /// Swapped on reconnect; read-locked during every query.
    client: Arc<RwLock<Arc<Client>>>,
}

impl std::fmt::Debug for AirhouseObservabilityStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AirhouseObservabilityStorage")
            .finish_non_exhaustive()
    }
}

// ── TLS helpers ───────────────────────────────────────────────────────────────

fn get_rustls_connector() -> MakeRustlsConnect {
    use std::sync::OnceLock;
    static TLS: OnceLock<MakeRustlsConnect> = OnceLock::new();
    TLS.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let cfg = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        MakeRustlsConnect::new(cfg)
    })
    .clone()
}

async fn try_connect(
    config: &tokio_postgres::Config,
    insecure: bool,
) -> Result<(Client, BoxConn), tokio_postgres::Error> {
    if insecure {
        let (c, conn) = config.connect(NoTls).await?;
        Ok((c, Box::pin(conn)))
    } else {
        let (c, conn) = config.connect(get_rustls_connector()).await?;
        Ok((c, Box::pin(conn)))
    }
}

// ── Reconnect driver ──────────────────────────────────────────────────────────

fn make_pg_config(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    database: &str,
) -> tokio_postgres::Config {
    let mut cfg = tokio_postgres::Config::new();
    cfg.host(host);
    cfg.port(port);
    cfg.user(user);
    cfg.password(password);
    cfg.dbname(database);
    cfg
}

/// Drives the pgwire connection future and reconnects with exponential backoff
/// on drop. Runs inside a single spawned task; an inner loop handles all
/// reconnect attempts so no additional tasks are spawned per reconnect.
fn spawn_driver(
    initial_conn: BoxConn,
    client_ref: Arc<RwLock<Arc<Client>>>,
    host: String,
    port: u16,
    insecure: bool,
    get_credentials: CredentialFn,
) {
    tokio::spawn(async move {
        let mut conn = initial_conn;
        loop {
            if let Err(e) = conn.await {
                tracing::warn!(
                    "Airhouse observability connection dropped: {}",
                    pg_err_chain(&e)
                );
            } else {
                // Clean server-initiated close — still try to reconnect.
                tracing::info!("Airhouse observability connection closed cleanly; reconnecting");
            }
            let mut delay = Duration::from_millis(200);
            loop {
                tokio::time::sleep(delay).await;
                let creds = match get_credentials().await {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!(
                            "Airhouse observability credential refresh failed: {e}, retrying in {delay:?}"
                        );
                        delay = (delay * 2).min(Duration::from_secs(30));
                        continue;
                    }
                };
                let (user, password, database) = creds;
                let config = make_pg_config(&host, port, &user, &password, &database);
                match try_connect(&config, insecure).await {
                    Ok((new_client, new_conn)) => {
                        *client_ref.write().await = Arc::new(new_client);
                        tracing::info!("Airhouse observability reconnected");
                        conn = new_conn;
                        break; // back to outer loop to drive new_conn
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Airhouse observability reconnect failed: {}, retrying in {delay:?}",
                            pg_err_chain(&e)
                        );
                        delay = (delay * 2).min(Duration::from_secs(30));
                    }
                }
            }
        }
    });
}

// ── Constructor ───────────────────────────────────────────────────────────────

impl AirhouseObservabilityStorage {
    /// Connect to Airhouse via the pgwire simple-query protocol.
    ///
    /// `get_credentials` is called once now and again on every reconnect to
    /// supply a fresh `(user, password, database)` triple — use the Airhouse
    /// token broker in the caller so ephemeral credentials are refreshed
    /// automatically.
    ///
    /// Set `insecure = true` only for localhost / trusted-network deployments.
    pub async fn connect(
        host: &str,
        port: u16,
        insecure: bool,
        get_credentials: CredentialFn,
    ) -> Result<Self, OxyError> {
        let (user, password, database) = get_credentials().await.map_err(|e| {
            OxyError::RuntimeError(format!(
                "Airhouse observability credential fetch failed: {e}"
            ))
        })?;

        let config = make_pg_config(host, port, &user, &password, &database);
        let (client, conn) = try_connect(&config, insecure).await.map_err(|e| {
            OxyError::RuntimeError(format!(
                "Airhouse observability connect failed: {}",
                pg_err_chain(&e)
            ))
        })?;

        let client_ref = Arc::new(RwLock::new(Arc::new(client)));
        spawn_driver(
            conn,
            Arc::clone(&client_ref),
            host.to_string(),
            port,
            insecure,
            get_credentials,
        );

        Ok(Self { client: client_ref })
    }

    /// Execute a SQL string and return all messages (caller filters rows).
    ///
    /// The `RwLock` is held only long enough to clone the inner `Arc<Client>`;
    /// the actual query runs without holding the lock, so concurrent callers
    /// do not serialize.
    pub(crate) async fn query(&self, sql: &str) -> Result<Vec<SimpleQueryMessage>, OxyError> {
        let client = Arc::clone(&*self.client.read().await);
        client.simple_query(sql).await.map_err(|e| {
            OxyError::RuntimeError(format!("Airhouse query failed: {}", pg_err_chain(&e)))
        })
    }

    /// Execute a SQL statement, ignoring the result messages.
    pub(crate) async fn execute(&self, sql: &str) -> Result<(), OxyError> {
        self.query(sql).await.map(|_| ())
    }

    /// Run all `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF NOT EXISTS` DDL.
    pub async fn ensure_schema(&self) -> Result<(), OxyError> {
        for ddl in schema::ALL_DDL {
            let stmt_hint = ddl.trim().lines().next().unwrap_or("(unknown)");
            self.execute(ddl).await.map_err(|e| {
                OxyError::RuntimeError(format!("Airhouse schema DDL failed at [{stmt_hint}]: {e}"))
            })?;
        }
        Ok(())
    }
}

// ── SQL helpers ───────────────────────────────────────────────────────────────

/// Format a `tokio_postgres::Error` with its full source chain.
///
/// `tokio_postgres::Error` Display shows only a short kind label (e.g.
/// `"db error"`); the actual server message lives in the `source()` chain.
/// Traversing the chain produces `"db error: FATAL: password authentication
/// failed for user \"foo\""` which is actionable.
fn pg_err_chain(e: &tokio_postgres::Error) -> String {
    use std::error::Error as StdError;
    let mut msg = e.to_string();
    let mut src: Option<&dyn StdError> = e.source();
    while let Some(s) = src {
        msg.push_str(": ");
        msg.push_str(&s.to_string());
        src = s.source();
    }
    msg
}

/// Escape a string for safe embedding in DuckDB SQL.
/// Strips NUL bytes (pgwire is NUL-terminated) and doubles single quotes.
pub(crate) fn esc(s: &str) -> String {
    s.replace('\0', "").replace('\'', "''")
}

pub(crate) fn get_str(row: &tokio_postgres::SimpleQueryRow, col: &str) -> String {
    row.get(col).unwrap_or_default().to_string()
}

pub(crate) fn get_i64(row: &tokio_postgres::SimpleQueryRow, col: &str) -> i64 {
    row.get(col)
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0)
}

pub(crate) fn get_u64(row: &tokio_postgres::SimpleQueryRow, col: &str) -> u64 {
    row.get(col)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
}

pub(crate) fn get_f64(row: &tokio_postgres::SimpleQueryRow, col: &str) -> f64 {
    row.get(col)
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// Parse a DuckDB `FLOAT[]` text representation (`[1.0,2.0,3.0]`) into `Vec<f32>`.
pub(crate) fn parse_float_array(s: &str) -> Vec<f32> {
    let trimmed = s.trim().trim_start_matches('[').trim_end_matches(']');
    if trimmed.is_empty() {
        return Vec::new();
    }
    trimmed
        .split(',')
        .filter_map(|v| v.trim().parse::<f32>().ok())
        .collect()
}

/// Format a float slice as a DuckDB array literal: `[1.0,2.0]::FLOAT[]`.
pub(crate) fn format_float_array(arr: &[f32]) -> String {
    let mut s = String::with_capacity(arr.len() * 10 + 12);
    s.push('[');
    for (i, v) in arr.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        write!(s, "{v}").unwrap();
    }
    s.push_str("]::FLOAT[]");
    s
}

// ── ObservabilityStore impl ───────────────────────────────────────────────────

#[async_trait]
impl ObservabilityStore for AirhouseObservabilityStorage {
    async fn list_traces(
        &self,
        limit: i64,
        offset: i64,
        agent_ref: Option<&str>,
        status: Option<&str>,
        duration_filter: Option<&str>,
    ) -> Result<(Vec<TraceRow>, i64), OxyError> {
        traces::list_traces(self, limit, offset, agent_ref, status, duration_filter).await
    }

    async fn get_trace_detail(&self, trace_id: &str) -> Result<Vec<TraceDetailRow>, OxyError> {
        traces::get_trace_detail(self, trace_id).await
    }

    async fn get_cluster_map_data(
        &self,
        days: u32,
        limit: usize,
        source: Option<&str>,
    ) -> Result<Vec<ClusterMapDataRow>, OxyError> {
        traces::get_cluster_map_data(self, days, limit, source).await
    }

    async fn get_cluster_infos(&self) -> Result<Vec<ClusterInfoRow>, OxyError> {
        traces::get_cluster_infos(self).await
    }

    async fn get_trace_enrichments(
        &self,
        trace_ids: &[String],
    ) -> Result<Vec<TraceEnrichmentRow>, OxyError> {
        traces::get_trace_enrichments(self, trace_ids).await
    }

    async fn fetch_unprocessed_questions(
        &self,
        limit: usize,
    ) -> Result<Vec<(String, String, String)>, OxyError> {
        intents::fetch_unprocessed_questions(self, limit).await
    }

    async fn load_embeddings(
        &self,
    ) -> Result<Vec<(String, String, Vec<f32>, String, String)>, OxyError> {
        intents::load_embeddings(self).await
    }

    async fn store_clusters(&self, clusters: &[IntentCluster]) -> Result<(), OxyError> {
        intents::store_clusters(self, clusters).await
    }

    async fn load_clusters(&self) -> Result<Vec<IntentCluster>, OxyError> {
        intents::load_clusters(self).await
    }

    async fn store_classification(
        &self,
        trace_id: &str,
        question: &str,
        cluster_id: u32,
        intent_name: &str,
        confidence: f32,
        embedding: &[f32],
        source_type: &str,
        source: &str,
    ) -> Result<(), OxyError> {
        intents::store_classification(
            self,
            trace_id,
            question,
            cluster_id,
            intent_name,
            confidence,
            embedding,
            source_type,
            source,
        )
        .await
    }

    async fn update_classification(
        &self,
        trace_id: &str,
        question: &str,
        cluster_id: u32,
        intent_name: &str,
        confidence: f32,
        embedding: &[f32],
        source_type: &str,
        source: &str,
    ) -> Result<(), OxyError> {
        // Delete-then-insert on (trace_id, question) — same as store_classification.
        intents::store_classification(
            self,
            trace_id,
            question,
            cluster_id,
            intent_name,
            confidence,
            embedding,
            source_type,
            source,
        )
        .await
    }

    async fn get_intent_analytics(&self, days: u32) -> Result<Vec<IntentAnalyticsRow>, OxyError> {
        intents::get_intent_analytics(self, days).await
    }

    async fn get_outliers(&self, limit: usize) -> Result<Vec<(String, String)>, OxyError> {
        intents::get_outliers(self, limit).await
    }

    async fn load_unknown_classifications(
        &self,
    ) -> Result<Vec<(String, String, Vec<f32>, String)>, OxyError> {
        intents::load_unknown_classifications(self).await
    }

    async fn get_unknown_count(&self) -> Result<usize, OxyError> {
        intents::get_unknown_count(self).await
    }

    async fn update_cluster_record(&self, cluster: &IntentCluster) -> Result<(), OxyError> {
        intents::update_cluster_record(self, cluster).await
    }

    async fn get_next_cluster_id(&self) -> Result<u32, OxyError> {
        intents::get_next_cluster_id(self).await
    }

    async fn store_metric_usages(&self, metrics: Vec<MetricUsageRecord>) -> Result<(), OxyError> {
        metrics::store_metric_usages(self, metrics).await
    }

    async fn get_metrics_analytics(&self, days: u32) -> Result<MetricAnalyticsData, OxyError> {
        metrics::get_metrics_analytics(self, days).await
    }

    async fn get_metrics_list(
        &self,
        days: u32,
        limit: usize,
        offset: usize,
    ) -> Result<MetricsListData, OxyError> {
        metrics::get_metrics_list(self, days, limit, offset).await
    }

    async fn get_metric_detail(
        &self,
        metric_name: &str,
        days: u32,
    ) -> Result<MetricDetailData, OxyError> {
        metrics::get_metric_detail(self, metric_name, days).await
    }

    async fn get_execution_summary(&self, days: u32) -> Result<ExecutionSummaryData, OxyError> {
        execution_analytics::get_execution_summary(self, days).await
    }

    async fn get_execution_time_series(
        &self,
        days: u32,
    ) -> Result<Vec<ExecutionTimeBucketData>, OxyError> {
        execution_analytics::get_execution_time_series(self, days).await
    }

    async fn get_execution_agent_stats(
        &self,
        days: u32,
        limit: usize,
    ) -> Result<Vec<AgentExecutionStatsData>, OxyError> {
        execution_analytics::get_execution_agent_stats(self, days, limit).await
    }

    async fn get_execution_list(
        &self,
        days: u32,
        limit: usize,
        offset: usize,
        execution_type: Option<&str>,
        is_verified: Option<bool>,
        source_ref: Option<&str>,
        status: Option<&str>,
    ) -> Result<ExecutionListData, OxyError> {
        execution_analytics::get_execution_list(
            self,
            days,
            limit,
            offset,
            execution_type,
            is_verified,
            source_ref,
            status,
        )
        .await
    }

    async fn insert_spans(&self, spans: Vec<SpanRecord>) -> Result<(), OxyError> {
        if spans.is_empty() {
            return Ok(());
        }
        // Batch into chunks to keep SQL strings manageable.
        for chunk in spans.chunks(500) {
            let mut sql = String::from(
                "INSERT INTO oxy_obs_spans \
                 (trace_id, span_id, parent_span_id, span_name, service_name, \
                  span_attributes, duration_ns, status_code, status_message, \
                  event_data, timestamp) VALUES ",
            );
            for (i, span) in chunk.iter().enumerate() {
                if i > 0 {
                    sql.push(',');
                }
                write!(
                    sql,
                    "('{}','{}','{}','{}','{}','{}',{},'{}','{}','{}','{}' ::TIMESTAMPTZ)",
                    esc(&span.trace_id),
                    esc(&span.span_id),
                    esc(&span.parent_span_id),
                    esc(&span.span_name),
                    esc(&span.service_name),
                    esc(&span.span_attributes),
                    span.duration_ns,
                    esc(&span.status_code),
                    esc(&span.status_message),
                    esc(&span.event_data),
                    esc(&span.timestamp),
                )
                .unwrap();
            }
            self.execute(&sql).await?;
        }
        Ok(())
    }

    async fn purge_older_than(&self, retention_days: u32) -> Result<u64, OxyError> {
        if retention_days == 0 {
            return Ok(0);
        }
        let tables = [
            ("oxy_obs_spans", "timestamp"),
            ("oxy_obs_intent_classifications", "classified_at"),
            ("oxy_obs_metric_usage", "created_at"),
        ];
        let mut total = 0u64;
        for (table, column) in tables {
            let sql = format!(
                "DELETE FROM {table} \
                 WHERE {column} < current_timestamp::TIMESTAMP - INTERVAL '{retention_days} DAY'"
            );
            let msgs = self.query(&sql).await?;
            for msg in &msgs {
                if let SimpleQueryMessage::CommandComplete(n) = msg {
                    total = total.saturating_add(*n);
                }
            }
        }
        Ok(total)
    }

    async fn shutdown(&self) {
        tracing::debug!("AirhouseObservabilityStorage shutdown");
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── esc ───────────────────────────────────────────────────────────────────

    #[test]
    fn esc_empty() {
        assert_eq!(esc(""), "");
    }

    #[test]
    fn esc_no_special_chars() {
        assert_eq!(esc("hello world"), "hello world");
    }

    #[test]
    fn esc_single_quote() {
        assert_eq!(esc("it's"), "it''s");
    }

    #[test]
    fn esc_multiple_quotes() {
        assert_eq!(esc("a'b'c"), "a''b''c");
    }

    #[test]
    fn esc_nul_byte_stripped() {
        assert_eq!(esc("ab\0cd"), "abcd");
    }

    #[test]
    fn esc_nul_and_quote_combined() {
        assert_eq!(esc("a\0'b"), "a''b");
    }

    // ── parse_float_array ─────────────────────────────────────────────────────

    #[test]
    fn parse_float_array_basic() {
        assert_eq!(parse_float_array("[1.0,2.5,3.0]"), vec![1.0f32, 2.5, 3.0]);
    }

    #[test]
    fn parse_float_array_empty_string() {
        assert_eq!(parse_float_array(""), Vec::<f32>::new());
    }

    #[test]
    fn parse_float_array_empty_brackets() {
        assert_eq!(parse_float_array("[]"), Vec::<f32>::new());
    }

    #[test]
    fn parse_float_array_whitespace() {
        assert_eq!(parse_float_array("[ 1.0 , 2.0 ]"), vec![1.0f32, 2.0]);
    }

    #[test]
    fn parse_float_array_invalid_entries_skipped() {
        assert_eq!(parse_float_array("[1.0,bad,3.0]"), vec![1.0f32, 3.0]);
    }

    // ── format_float_array ────────────────────────────────────────────────────

    #[test]
    fn format_float_array_basic() {
        assert_eq!(format_float_array(&[1.0, 2.0, 3.0]), "[1,2,3]::FLOAT[]");
    }

    #[test]
    fn format_float_array_empty() {
        assert_eq!(format_float_array(&[]), "[]::FLOAT[]");
    }

    #[test]
    fn format_float_array_single() {
        assert_eq!(format_float_array(&[0.5]), "[0.5]::FLOAT[]");
    }

    #[test]
    fn format_float_array_roundtrip() {
        let original = vec![1.5f32, -2.25, 0.0, 100.0];
        let formatted = format_float_array(&original);
        let parsed = parse_float_array(&formatted.replace("::FLOAT[]", ""));
        assert_eq!(parsed, original);
    }

    // ── credentials_from_env ──────────────────────────────────────────────────

    /// All three OBS env vars touch the process-wide environment, so the
    /// credentials_from_env tests must serialize against each other.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn clear_obs_env() {
        for k in [
            "OXY_AIRHOUSE_OBS_USER",
            "OXY_AIRHOUSE_OBS_PASSWORD",
            "OXY_AIRHOUSE_OBS_DATABASE",
        ] {
            // SAFETY: serialized via ENV_LOCK; no other thread reads these in
            // tests.
            unsafe { std::env::remove_var(k) };
        }
    }

    #[tokio::test]
    async fn credentials_from_env_returns_none_when_unset() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_obs_env();
        assert!(credentials_from_env().is_none());
    }

    #[tokio::test]
    async fn credentials_from_env_returns_none_when_partial() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_obs_env();
        // SAFETY: serialized via ENV_LOCK.
        unsafe {
            std::env::set_var("OXY_AIRHOUSE_OBS_USER", "obs");
            std::env::set_var("OXY_AIRHOUSE_OBS_PASSWORD", "pw");
            // database missing
        }
        assert!(credentials_from_env().is_none());
        clear_obs_env();
    }

    #[tokio::test]
    async fn credentials_from_env_returns_none_when_empty_string() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_obs_env();
        // SAFETY: serialized via ENV_LOCK.
        unsafe {
            std::env::set_var("OXY_AIRHOUSE_OBS_USER", "obs");
            std::env::set_var("OXY_AIRHOUSE_OBS_PASSWORD", "");
            std::env::set_var("OXY_AIRHOUSE_OBS_DATABASE", "obs_db");
        }
        assert!(credentials_from_env().is_none());
        clear_obs_env();
    }

    #[tokio::test]
    async fn credentials_from_env_reads_triple_on_each_call() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_obs_env();
        // SAFETY: serialized via ENV_LOCK.
        unsafe {
            std::env::set_var("OXY_AIRHOUSE_OBS_USER", "obs_user");
            std::env::set_var("OXY_AIRHOUSE_OBS_PASSWORD", "obs_pw");
            std::env::set_var("OXY_AIRHOUSE_OBS_DATABASE", "obs_db");
        }
        let cred_fn = credentials_from_env().expect("all three set");

        let (u, p, d) = cred_fn().await.unwrap();
        assert_eq!(
            (u.as_str(), p.as_str(), d.as_str()),
            ("obs_user", "obs_pw", "obs_db")
        );

        // Second call sees fresh env — confirms the closure re-reads on every
        // invocation, which is what spawn_driver relies on for ephemeral
        // credential refresh.
        unsafe { std::env::set_var("OXY_AIRHOUSE_OBS_PASSWORD", "rotated_pw") };
        let (_, p2, _) = cred_fn().await.unwrap();
        assert_eq!(p2, "rotated_pw");

        clear_obs_env();
    }
}
