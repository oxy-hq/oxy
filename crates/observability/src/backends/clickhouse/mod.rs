//! ClickHouse-backed implementation of [`crate::store::ObservabilityStore`].
//!
//! Uses the `clickhouse` crate's HTTP API to execute SQL against the
//! `observability_*` tables.

mod execution_analytics;
mod intents;
mod latency_cost;
mod metrics;
pub mod schema;
mod traces;

use async_trait::async_trait;
use clickhouse::{Client, Row};
use oxy_shared::errors::OxyError;
use serde::Deserialize;

/// Single-column `count()` result used by the rollup backfill guard.
#[derive(Debug, Deserialize, Row)]
struct RowCount {
    c: u64,
}

/// Render a stored instant column as an unambiguous ISO-8601 / RFC3339 **UTC**
/// string, e.g. `2026-07-12T14:30:00.123456789Z`.
///
/// The trailing `Z` is a literal (ClickHouse copies non-`%` chars verbatim) and
/// the `'UTC'` timezone arg forces UTC output. Without both, `formatDateTime`
/// emits a zoneless `2026-07-12 14:30:00…` form that the frontend `new Date(…)`
/// parses as *browser-local* time — silently shifting every timestamp, and
/// throwing `RangeError: Invalid time value` on an unparseable one. Kept in one
/// place so every serving query renders timestamps identically.
pub(super) fn iso_utc(col: &str) -> String {
    format!("formatDateTime({col}, '%Y-%m-%dT%H:%M:%S.%fZ', 'UTC')")
}

/// Hard client-side ceiling for a single serving query. Set slightly above the
/// server-side `max_execution_time` (see [`ClickHouseObservabilityStorage::read_client`])
/// so a *reachable* ClickHouse aborts the query itself first; this only fires as
/// a backstop when ClickHouse is unreachable and the HTTP call would otherwise
/// hang forever — the default `clickhouse` client sets no timeout, so an
/// unbounded await would pin the request task and eventually 503 the instance.
const SERVING_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(35);

/// Run a serving query future under [`SERVING_QUERY_TIMEOUT`], mapping an
/// elapsed timeout to a clean error instead of a hung task. `what` names the
/// query for the error message (e.g. `"trace detail"`).
pub(super) async fn with_query_timeout<T, F>(what: &str, fut: F) -> Result<T, OxyError>
where
    F: std::future::Future<Output = Result<T, OxyError>>,
{
    match tokio::time::timeout(SERVING_QUERY_TIMEOUT, fut).await {
        Ok(res) => res,
        Err(_) => Err(OxyError::RuntimeError(format!(
            "ClickHouse {what} query exceeded {}s",
            SERVING_QUERY_TIMEOUT.as_secs()
        ))),
    }
}

#[cfg(test)]
mod iso_utc_tests {
    use super::iso_utc;

    /// Regression guard for the "Invalid time value" trace crash: serving
    /// timestamps must render as ISO-8601 **UTC** (`…Z`, forced `'UTC'` tz) so
    /// the frontend `new Date(…)` parses them as UTC rather than browser-local.
    #[test]
    fn renders_iso_8601_utc_with_zulu_suffix() {
        assert_eq!(
            iso_utc("timestamp"),
            "formatDateTime(timestamp, '%Y-%m-%dT%H:%M:%S.%fZ', 'UTC')"
        );
    }
}

use crate::intent_types::IntentCluster;
use crate::store::ObservabilityStore;
use crate::types::{
    AgentExecutionStatsData, ClusterInfoRow, ClusterMapDataRow, ExecutionListData,
    ExecutionSummaryData, ExecutionTimeBucketData, IntentAnalyticsRow, LatencyHistogramData,
    LatencyPercentilesData, MetricAnalyticsData, MetricDetailData, MetricUsageRecord,
    MetricsListData, ModelUsageData, SpanRecord, TraceDetailRow, TraceEnrichmentRow, TraceRow,
};

/// ClickHouse observability storage backend.
pub struct ClickHouseObservabilityStorage {
    client: Client,
    /// Target database (from `OXY_CLICKHOUSE_DATABASE`). Kept so `ensure_schema`
    /// can `CREATE DATABASE` it before the unqualified table DDL runs.
    database: String,
}

impl std::fmt::Debug for ClickHouseObservabilityStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClickHouseObservabilityStorage")
            .finish_non_exhaustive()
    }
}

impl ClickHouseObservabilityStorage {
    /// Construct from explicit ClickHouse connection parameters.
    pub fn new(url: &str, user: &str, password: &str, database: &str) -> Result<Self, OxyError> {
        let client = Client::default()
            .with_url(url)
            .with_user(user)
            .with_password(password)
            .with_database(database);
        Ok(Self {
            client,
            database: database.to_string(),
        })
    }

    /// Construct from standard `OXY_CLICKHOUSE_*` environment variables.
    pub async fn from_env() -> Result<Self, OxyError> {
        let url = std::env::var("OXY_CLICKHOUSE_URL")
            .unwrap_or_else(|_| "http://localhost:8123".to_string());
        let user = std::env::var("OXY_CLICKHOUSE_USER").unwrap_or_else(|_| "default".to_string());
        let password = std::env::var("OXY_CLICKHOUSE_PASSWORD").unwrap_or_default();
        let database = std::env::var("OXY_CLICKHOUSE_DATABASE")
            .unwrap_or_else(|_| "observability".to_string());
        Self::new(&url, &user, &password, &database)
    }

    /// Accessor for the underlying ClickHouse client.
    pub(crate) fn client(&self) -> &Client {
        &self.client
    }

    /// A client clone carrying server-side guards for user-facing **read**
    /// queries: it caps wall-clock (`max_execution_time`) and result size, and
    /// makes an oversized result an error rather than an unbounded stream. A
    /// slow or pathological query then fails fast as a clean error instead of
    /// pinning a request task and starving the instance until the load balancer
    /// 503s it. Intentionally not used for DDL/backfill, which legitimately run
    /// longer than a serving request should.
    pub(super) fn read_client(&self) -> Client {
        self.client
            .clone()
            .with_option("max_execution_time", "30")
            .with_option("max_result_rows", "1000000")
            .with_option("max_result_bytes", "268435456") // 256 MiB
            .with_option("result_overflow_mode", "throw")
    }

    /// Ensure all observability tables exist.
    ///
    /// Safe to call on every startup; uses `CREATE TABLE IF NOT EXISTS` DDL.
    pub async fn ensure_schema(&self) -> Result<(), OxyError> {
        // The unqualified `CREATE TABLE` DDL below targets the connection's
        // database, which must already exist. A CHI / managed ClickHouse (unlike
        // the local docker image's `CLICKHOUSE_DB`) does not pre-create it, so
        // create it here via a client scoped to the always-present `default` db.
        // `database` is operator config (backtick-quoted defensively).
        self.client
            .clone()
            .with_database("default")
            .query(&format!(
                "CREATE DATABASE IF NOT EXISTS `{}`",
                self.database
            ))
            .execute()
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!("ClickHouse CREATE DATABASE failed: {e}"))
            })?;

        for ddl in schema::ALL_DDL {
            self.client.query(ddl).execute().await.map_err(|e| {
                OxyError::RuntimeError(format!("ClickHouse schema DDL failed: {e}"))
            })?;
        }

        // The execution rollup MV shares its flatten logic with the backfill via
        // `EXECUTIONS_SELECT`, so it's assembled here rather than sitting in
        // `ALL_DDL` as a frozen string.
        let mv = format!(
            "{} {}",
            schema::CREATE_EXECUTIONS_MV_PREFIX,
            schema::EXECUTIONS_SELECT
        );
        self.client.query(&mv).execute().await.map_err(|e| {
            OxyError::RuntimeError(format!("ClickHouse executions MV DDL failed: {e}"))
        })?;

        self.backfill_executions_history().await?;
        Ok(())
    }

    /// Seed `observability_executions` from spans already on disk.
    ///
    /// The materialized view only captures spans inserted *after* it exists, so
    /// on the first deploy the Execution Analytics panels would read an empty
    /// rollup until 90 days of fresh traffic accrued. This one-shot backfill
    /// flattens the existing retention window using the same `EXECUTIONS_SELECT`
    /// the MV uses (so rows are identical).
    ///
    /// The guard is deliberately *not* `count() == 0`: the MV is already live by
    /// the time we reach here, so a single span inserted between MV creation and
    /// the check would flip an empty-table guard and silently skip the whole
    /// historical seed — forever, since the guard would then stay non-zero. We
    /// instead ask whether any rollup row is older than an hour: a fresh MV row
    /// carries a ~now() timestamp so it can't trip this guard, but a prior
    /// backfill (or an hour of live traffic) does. A concurrent insert during
    /// the backfill window is deduped by `ReplacingMergeTree` on the `span_id`
    /// key (and reads use `FINAL`).
    async fn backfill_executions_history(&self) -> Result<(), OxyError> {
        let seeded = self
            .client
            .query(
                "SELECT count() AS c FROM observability_executions \
                 WHERE timestamp < now() - INTERVAL 1 HOUR",
            )
            .fetch_one::<RowCount>()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("executions rollup count failed: {e}")))?;
        if seeded.c > 0 {
            return Ok(());
        }

        let backfill = format!(
            "INSERT INTO observability_executions {} AND timestamp >= now() - INTERVAL {} DAY",
            schema::EXECUTIONS_SELECT,
            crate::RETENTION_DAYS
        );
        self.client.query(&backfill).execute().await.map_err(|e| {
            OxyError::RuntimeError(format!("executions rollup backfill failed: {e}"))
        })?;
        tracing::info!("observability: backfilled execution rollup from existing spans");
        Ok(())
    }

    /// Apply or remove TTL on event tables so ClickHouse's background merge
    /// expires old rows automatically. `retention_days = 0` removes any
    /// existing TTL ("REMOVE TTL"); non-zero sets
    /// `TTL <column> + INTERVAL N DAY DELETE`. Intent clusters never get a TTL
    /// because they're aggregated labels, not event data.
    pub async fn apply_retention_ttl(&self, retention_days: u32) -> Result<(), OxyError> {
        let tables: &[(&str, &str)] = &[
            ("observability_spans", "timestamp"),
            ("observability_executions", "timestamp"),
            ("observability_intent_classifications", "classified_at"),
            ("observability_metric_usage", "created_at"),
        ];

        for (table, column) in tables {
            let sql = if retention_days == 0 {
                format!("ALTER TABLE {table} REMOVE TTL")
            } else {
                format!(
                    "ALTER TABLE {table} MODIFY TTL {column} + INTERVAL {retention_days} DAY DELETE"
                )
            };
            self.client.query(&sql).execute().await.map_err(|e| {
                OxyError::RuntimeError(format!("ClickHouse TTL update on {table} failed: {e}"))
            })?;
        }
        Ok(())
    }
}

#[async_trait]
impl ObservabilityStore for ClickHouseObservabilityStorage {
    async fn list_traces(
        &self,
        limit: i64,
        offset: i64,
        agent_ref: Option<&str>,
        status: Option<&str>,
        duration_filter: Option<&str>,
    ) -> Result<(Vec<TraceRow>, i64), OxyError> {
        traces::list_traces(
            self,
            limit,
            offset,
            agent_ref,
            status,
            duration_filter,
            None,
            None,
            None,
        )
        .await
    }

    async fn search_traces(
        &self,
        limit: i64,
        offset: i64,
        agent_ref: Option<&str>,
        status: Option<&str>,
        duration_filter: Option<&str>,
        search: Option<&str>,
        from_ts: Option<i64>,
        to_ts: Option<i64>,
    ) -> Result<(Vec<TraceRow>, i64), OxyError> {
        traces::list_traces(
            self,
            limit,
            offset,
            agent_ref,
            status,
            duration_filter,
            search,
            from_ts,
            to_ts,
        )
        .await
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
        with_query_timeout(
            "get_cluster_map_data",
            traces::get_cluster_map_data(self, days, limit, source),
        )
        .await
    }

    async fn get_cluster_infos(&self) -> Result<Vec<ClusterInfoRow>, OxyError> {
        with_query_timeout("get_cluster_infos", traces::get_cluster_infos(self)).await
    }

    async fn get_trace_enrichments(
        &self,
        trace_ids: &[String],
    ) -> Result<Vec<TraceEnrichmentRow>, OxyError> {
        with_query_timeout(
            "get_trace_enrichments",
            traces::get_trace_enrichments(self, trace_ids),
        )
        .await
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
        // ReplacingMergeTree ordered by (trace_id, question) handles upsert-
        // style semantics; reuse the store path.
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
        with_query_timeout(
            "get_intent_analytics",
            intents::get_intent_analytics(self, days),
        )
        .await
    }

    async fn get_outliers(&self, limit: usize) -> Result<Vec<(String, String)>, OxyError> {
        with_query_timeout("get_outliers", intents::get_outliers(self, limit)).await
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
        with_query_timeout(
            "get_metrics_analytics",
            metrics::get_metrics_analytics(self, days),
        )
        .await
    }

    async fn get_metrics_list(
        &self,
        days: u32,
        limit: usize,
        offset: usize,
    ) -> Result<MetricsListData, OxyError> {
        with_query_timeout(
            "get_metrics_list",
            metrics::get_metrics_list(self, days, limit, offset),
        )
        .await
    }

    async fn get_metric_detail(
        &self,
        metric_name: &str,
        days: u32,
    ) -> Result<MetricDetailData, OxyError> {
        with_query_timeout(
            "get_metric_detail",
            metrics::get_metric_detail(self, metric_name, days),
        )
        .await
    }

    async fn get_execution_summary(&self, days: u32) -> Result<ExecutionSummaryData, OxyError> {
        with_query_timeout(
            "get_execution_summary",
            execution_analytics::get_execution_summary(self, days),
        )
        .await
    }

    async fn get_execution_time_series(
        &self,
        days: u32,
    ) -> Result<Vec<ExecutionTimeBucketData>, OxyError> {
        with_query_timeout(
            "get_execution_time_series",
            execution_analytics::get_execution_time_series(self, days),
        )
        .await
    }

    async fn get_execution_agent_stats(
        &self,
        days: u32,
        limit: usize,
    ) -> Result<Vec<AgentExecutionStatsData>, OxyError> {
        with_query_timeout(
            "get_execution_agent_stats",
            execution_analytics::get_execution_agent_stats(self, days, limit),
        )
        .await
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
        with_query_timeout(
            "get_execution_list",
            execution_analytics::get_execution_list(
                self,
                days,
                limit,
                offset,
                execution_type,
                is_verified,
                source_ref,
                status,
            ),
        )
        .await
    }

    async fn get_latency_percentiles(&self, days: u32) -> Result<LatencyPercentilesData, OxyError> {
        with_query_timeout(
            "get_latency_percentiles",
            latency_cost::get_latency_percentiles(self, days),
        )
        .await
    }

    async fn get_latency_histogram(&self, days: u32) -> Result<LatencyHistogramData, OxyError> {
        with_query_timeout(
            "get_latency_histogram",
            latency_cost::get_latency_histogram(self, days),
        )
        .await
    }

    async fn get_model_usage(&self, days: u32) -> Result<Vec<ModelUsageData>, OxyError> {
        with_query_timeout("get_model_usage", latency_cost::get_model_usage(self, days)).await
    }

    async fn insert_spans(&self, spans: Vec<SpanRecord>) -> Result<(), OxyError> {
        traces::insert_spans(self, spans).await
    }

    async fn purge_older_than(&self, _retention_days: u32) -> Result<u64, OxyError> {
        // ClickHouse handles retention natively via TTL clauses configured at
        // startup via `apply_retention_ttl()`. Background merges delete expired
        // rows automatically; no app-level DELETE needed.
        Ok(0)
    }

    async fn shutdown(&self) {
        // HTTP client has no long-lived resources.
        tracing::debug!("ClickHouseObservabilityStorage shutdown");
    }
}
