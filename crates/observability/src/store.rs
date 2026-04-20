//! Trait definition for observability storage backends.
//!
//! The [`ObservabilityStore`] trait abstracts the storage backend for
//! observability data (traces, intents, metrics, execution analytics).
//! This allows swapping the underlying storage engine (DuckDB, Postgres,
//! ClickHouse, ...) without changing consumer code.

use async_trait::async_trait;
use oxy_shared::errors::OxyError;

use crate::intent_types::IntentCluster;
use crate::types::{
    AgentExecutionStatsData, ClusterInfoRow, ClusterMapDataRow, ExecutionListData,
    ExecutionSummaryData, ExecutionTimeBucketData, IntentAnalyticsRow, MetricAnalyticsData,
    MetricDetailData, MetricUsageRecord, MetricsListData, SpanRecord, TraceDetailRow,
    TraceEnrichmentRow, TraceRow,
};

/// Abstraction over an observability storage backend.
///
/// All methods are async and return `Result<T, OxyError>`. Implementors must
/// be `Send + Sync + Debug` so the trait object can be shared across threads
/// and stored in application state.
#[async_trait]
pub trait ObservabilityStore: Send + Sync + std::fmt::Debug {
    // ── Traces ────────────────────────────────────────────────────────────

    /// List traces with pagination and filtering.
    /// Returns `(traces, total_count)`.
    async fn list_traces(
        &self,
        limit: i64,
        offset: i64,
        agent_ref: Option<&str>,
        status: Option<&str>,
        duration_filter: Option<&str>,
    ) -> Result<(Vec<TraceRow>, i64), OxyError>;

    /// Get all spans for a given trace ID.
    async fn get_trace_detail(&self, trace_id: &str) -> Result<Vec<TraceDetailRow>, OxyError>;

    /// Get embeddings with classification data for cluster map visualization.
    async fn get_cluster_map_data(
        &self,
        days: u32,
        limit: usize,
        source: Option<&str>,
    ) -> Result<Vec<ClusterMapDataRow>, OxyError>;

    /// Get cluster info for visualization.
    async fn get_cluster_infos(&self) -> Result<Vec<ClusterInfoRow>, OxyError>;

    /// Get trace enrichment data (status, duration) for a set of trace IDs.
    async fn get_trace_enrichments(
        &self,
        trace_ids: &[String],
    ) -> Result<Vec<TraceEnrichmentRow>, OxyError>;

    // ── Intents ───────────────────────────────────────────────────────────

    /// Fetch unprocessed questions from spans that lack classifications.
    /// Returns tuples of `(trace_id, question, source)`.
    async fn fetch_unprocessed_questions(
        &self,
        limit: usize,
    ) -> Result<Vec<(String, String, String)>, OxyError>;

    /// Load all embeddings from intent_classifications.
    /// Returns tuples of `(trace_id, question, embedding, intent_name, source)`.
    async fn load_embeddings(
        &self,
    ) -> Result<Vec<(String, String, Vec<f32>, String, String)>, OxyError>;

    /// Store clusters (replace all existing, then insert new ones).
    async fn store_clusters(&self, clusters: &[IntentCluster]) -> Result<(), OxyError>;

    /// Load all clusters.
    async fn load_clusters(&self) -> Result<Vec<IntentCluster>, OxyError>;

    /// Store a classification result.
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
    ) -> Result<(), OxyError>;

    /// Upsert a classification keyed by `(trace_id, question)`. Implementations
    /// use their native upsert primitive (DuckDB `INSERT OR REPLACE`, Postgres
    /// `ON CONFLICT DO UPDATE`, ClickHouse `ReplacingMergeTree`).
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
    ) -> Result<(), OxyError>;

    /// Get intent analytics for the last N days.
    async fn get_intent_analytics(&self, days: u32) -> Result<Vec<IntentAnalyticsRow>, OxyError>;

    /// Get outlier questions (classified as "unknown").
    async fn get_outliers(&self, limit: usize) -> Result<Vec<(String, String)>, OxyError>;

    /// Load unknown classifications for incremental clustering.
    /// Returns tuples of `(trace_id, question, embedding, source)`.
    async fn load_unknown_classifications(
        &self,
    ) -> Result<Vec<(String, String, Vec<f32>, String)>, OxyError>;

    /// Get count of unknown classifications.
    async fn get_unknown_count(&self) -> Result<usize, OxyError>;

    /// Update a single cluster (upsert).
    async fn update_cluster_record(&self, cluster: &IntentCluster) -> Result<(), OxyError>;

    /// Get the next available cluster ID.
    async fn get_next_cluster_id(&self) -> Result<u32, OxyError>;

    // ── Metrics ───────────────────────────────────────────────────────────

    /// Store metric usage records.
    async fn store_metric_usages(&self, metrics: Vec<MetricUsageRecord>) -> Result<(), OxyError>;

    /// Get analytics summary for the last N days.
    async fn get_metrics_analytics(&self, days: u32) -> Result<MetricAnalyticsData, OxyError>;

    /// Get paginated metrics list.
    async fn get_metrics_list(
        &self,
        days: u32,
        limit: usize,
        offset: usize,
    ) -> Result<MetricsListData, OxyError>;

    /// Get detail for a specific metric.
    async fn get_metric_detail(
        &self,
        metric_name: &str,
        days: u32,
    ) -> Result<MetricDetailData, OxyError>;

    // ── Execution Analytics ───────────────────────────────────────────────

    /// Get execution analytics summary.
    async fn get_execution_summary(&self, days: u32) -> Result<ExecutionSummaryData, OxyError>;

    /// Get execution time series (daily buckets).
    async fn get_execution_time_series(
        &self,
        days: u32,
    ) -> Result<Vec<ExecutionTimeBucketData>, OxyError>;

    /// Get per-agent execution stats.
    async fn get_execution_agent_stats(
        &self,
        days: u32,
        limit: usize,
    ) -> Result<Vec<AgentExecutionStatsData>, OxyError>;

    /// Get paginated execution details.
    async fn get_execution_list(
        &self,
        days: u32,
        limit: usize,
        offset: usize,
        execution_type: Option<&str>,
        is_verified: Option<bool>,
        source_ref: Option<&str>,
        status: Option<&str>,
    ) -> Result<ExecutionListData, OxyError>;

    // ── Span Ingestion ─────────────────────────────────────────────────────

    /// Insert span records directly (used by the tracing layer bridge).
    async fn insert_spans(&self, spans: Vec<SpanRecord>) -> Result<(), OxyError>;

    // ── Retention ─────────────────────────────────────────────────────────

    /// Delete observability event data (spans, metric usage, intent
    /// classifications) older than `retention_days`. Intent clusters are
    /// preserved because they are aggregated labels, not event data.
    ///
    /// Returns the approximate number of rows deleted across all tables.
    /// Returns `0` when the backend handles expiry natively (e.g. ClickHouse
    /// via `TTL ... DELETE`). Callers pass `0` days to disable retention.
    async fn purge_older_than(&self, retention_days: u32) -> Result<u64, OxyError>;

    // ── Lifecycle ─────────────────────────────────────────────────────────

    /// Gracefully shut down the storage backend, flushing any buffered data.
    async fn shutdown(&self);
}
