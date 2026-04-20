//! ClickHouse-backed implementation of [`crate::store::ObservabilityStore`].
//!
//! Uses the `clickhouse` crate's HTTP API to execute SQL against the
//! `observability_*` tables.

mod execution_analytics;
mod intents;
mod metrics;
pub mod schema;
mod traces;

use async_trait::async_trait;
use clickhouse::Client;
use oxy_shared::errors::OxyError;

use crate::intent_types::IntentCluster;
use crate::store::ObservabilityStore;
use crate::types::{
    AgentExecutionStatsData, ClusterInfoRow, ClusterMapDataRow, ExecutionListData,
    ExecutionSummaryData, ExecutionTimeBucketData, IntentAnalyticsRow, MetricAnalyticsData,
    MetricDetailData, MetricUsageRecord, MetricsListData, SpanRecord, TraceDetailRow,
    TraceEnrichmentRow, TraceRow,
};

/// ClickHouse observability storage backend.
pub struct ClickHouseObservabilityStorage {
    client: Client,
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
        Ok(Self { client })
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

    /// Ensure all observability tables exist.
    ///
    /// Safe to call on every startup; uses `CREATE TABLE IF NOT EXISTS` DDL.
    pub async fn ensure_schema(&self) -> Result<(), OxyError> {
        for ddl in schema::ALL_DDL {
            self.client.query(ddl).execute().await.map_err(|e| {
                OxyError::RuntimeError(format!("ClickHouse schema DDL failed: {e}"))
            })?;
        }
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
