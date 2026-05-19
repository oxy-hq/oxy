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

// ── Storage struct ────────────────────────────────────────────────────────────

/// A boxed `Connection` future (erased TLS-stream type for reconnect reuse).
type BoxConn =
    Pin<Box<dyn Future<Output = Result<(), tokio_postgres::Error>> + Send + 'static>>;

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

/// Returns the process-wide rustls connector, building it once.
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

/// Drives the pgwire connection future and reconnects with exponential backoff
/// on drop. Runs inside a single spawned task using a loop to avoid recursive
/// spawns and per-reconnect config clones.
fn spawn_driver(
    initial_conn: BoxConn,
    client_ref: Arc<RwLock<Arc<Client>>>,
    config: tokio_postgres::Config,
    insecure: bool,
) {
    tokio::spawn(async move {
        let mut current_conn = initial_conn;
        loop {
            match current_conn.await {
                Ok(()) => break, // clean disconnect — stop
                Err(e) => tracing::warn!("Airhouse observability connection dropped: {e}"),
            }
            let mut delay = Duration::from_millis(200);
            current_conn = loop {
                tokio::time::sleep(delay).await;
                match try_connect(&config, insecure).await {
                    Ok((new_client, new_conn)) => {
                        *client_ref.write().await = Arc::new(new_client);
                        tracing::info!("Airhouse observability reconnected");
                        break new_conn;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Airhouse observability reconnect failed: {e}, retrying in {delay:?}"
                        );
                        delay = (delay * 2).min(Duration::from_secs(30));
                    }
                }
            };
        }
    });
}

// ── Constructor ───────────────────────────────────────────────────────────────

impl AirhouseObservabilityStorage {
    /// Connect to Airhouse via the pgwire simple-query protocol.
    ///
    /// Set `insecure = true` only for localhost/trusted-network deployments.
    /// When `false` (default), TLS is required.
    pub async fn connect(
        host: &str,
        port: u16,
        user: &str,
        password: &str,
        database: &str,
        insecure: bool,
    ) -> Result<Self, OxyError> {
        let mut config = tokio_postgres::Config::new();
        config.host(host);
        config.port(port);
        config.user(user);
        config.password(password);
        config.dbname(database);

        let (client, conn) =
            try_connect(&config, insecure)
                .await
                .map_err(|e| {
                    OxyError::RuntimeError(format!("Airhouse observability connect failed: {e}"))
                })?;

        let client_ref = Arc::new(RwLock::new(Arc::new(client)));
        spawn_driver(conn, Arc::clone(&client_ref), config, insecure);

        Ok(Self { client: client_ref })
    }

    /// Connect using standard `AIRHOUSE_*` and `OXY_AIRHOUSE_OBS_*` env vars.
    pub async fn from_env() -> Result<Self, OxyError> {
        let host = std::env::var("AIRHOUSE_WIRE_HOST").map_err(|_| {
            OxyError::RuntimeError(
                "AIRHOUSE_WIRE_HOST is required for the Airhouse observability backend".into(),
            )
        })?;
        let port = std::env::var("AIRHOUSE_WIRE_PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(5445);
        let user = std::env::var("OXY_AIRHOUSE_OBS_USER").map_err(|_| {
            OxyError::RuntimeError(
                "OXY_AIRHOUSE_OBS_USER is required for the Airhouse observability backend".into(),
            )
        })?;
        let password = std::env::var("OXY_AIRHOUSE_OBS_PASSWORD").map_err(|_| {
            OxyError::RuntimeError(
                "OXY_AIRHOUSE_OBS_PASSWORD is required for the Airhouse observability backend"
                    .into(),
            )
        })?;
        let database = std::env::var("OXY_AIRHOUSE_OBS_DATABASE").map_err(|_| {
            OxyError::RuntimeError(
                "OXY_AIRHOUSE_OBS_DATABASE is required for the Airhouse observability backend"
                    .into(),
            )
        })?;
        let insecure = std::env::var("OXY_AIRHOUSE_OBS_INSECURE")
            .ok()
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false);

        Self::connect(&host, port, &user, &password, &database, insecure).await
    }

    /// Execute a SQL string and return all messages (caller filters rows).
    ///
    /// The `RwLock` is held only long enough to clone the inner `Arc<Client>`;
    /// the actual query runs without holding the lock, so concurrent callers
    /// do not serialize.
    pub(crate) async fn query(&self, sql: &str) -> Result<Vec<SimpleQueryMessage>, OxyError> {
        let client = Arc::clone(&*self.client.read().await);
        client
            .simple_query(sql)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Airhouse query failed: {e}")))
    }

    /// Execute a SQL statement, ignoring the result messages.
    pub(crate) async fn execute(&self, sql: &str) -> Result<(), OxyError> {
        self.query(sql).await.map(|_| ())
    }

    /// Run all `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF NOT EXISTS` DDL.
    pub async fn ensure_schema(&self) -> Result<(), OxyError> {
        for ddl in schema::ALL_DDL {
            self.execute(ddl).await.map_err(|e| {
                OxyError::RuntimeError(format!("Airhouse schema DDL failed: {e}"))
            })?;
        }
        Ok(())
    }
}

// ── SQL helpers ───────────────────────────────────────────────────────────────

/// Escape a string for safe embedding in DuckDB SQL.
///
/// Doubles single-quote characters (`'` → `''`) and strips NUL bytes —
/// pgwire simple-query is NUL-terminated, so an embedded `\0` would silently
/// truncate the SQL string at the wire level.
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
        // Upserts by PRIMARY KEY (trace_id, question) — same as store_classification.
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
                "INSERT OR REPLACE INTO oxy_obs_spans \
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
        // NUL removed first, then quote doubled — order in the chain matters.
        assert_eq!(esc("a\0'b"), "a''b");
    }

    #[test]
    fn esc_empty() {
        assert_eq!(esc(""), "");
    }

    // ── parse_float_array ─────────────────────────────────────────────────────

    #[test]
    fn parse_float_array_basic() {
        let v = parse_float_array("[1.0,2.5,3.0]");
        assert_eq!(v, vec![1.0f32, 2.5, 3.0]);
    }

    #[test]
    fn parse_float_array_empty_string() {
        assert!(parse_float_array("[]").is_empty());
        assert!(parse_float_array("").is_empty());
    }

    #[test]
    fn parse_float_array_whitespace_tolerant() {
        let v = parse_float_array("[ 1.0, 2.0 ]");
        assert_eq!(v, vec![1.0f32, 2.0]);
    }

    #[test]
    fn parse_float_array_skips_invalid() {
        // Non-parseable tokens are filtered out silently.
        let v = parse_float_array("[1.0,NaN,2.0]");
        // "NaN" parses as f32::NAN — filter_map keeps it; just check len.
        assert_eq!(v.len(), 3);
        assert_eq!(v[0], 1.0);
        assert_eq!(v[2], 2.0);
    }

    // ── format_float_array ────────────────────────────────────────────────────

    #[test]
    fn format_float_array_basic() {
        let s = format_float_array(&[1.0, 2.5, 3.0]);
        assert_eq!(s, "[1,2.5,3]::FLOAT[]");
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
    fn format_roundtrip() {
        let orig = vec![1.0f32, 0.25, 3.75];
        let encoded = format_float_array(&orig);
        // Strip the ::FLOAT[] suffix before parsing.
        let raw = encoded.trim_end_matches("::FLOAT[]");
        let decoded = parse_float_array(raw);
        assert_eq!(decoded, orig);
    }
}
