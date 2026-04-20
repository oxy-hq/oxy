//! [`ObservabilityStore`] trait implementation for [`DuckDBStorage`].
//!
//! Each trait method delegates to the corresponding inherent method on
//! `DuckDBStorage`. Rust's method resolution rules mean that within this
//! `impl ObservabilityStore for DuckDBStorage` block, a bare `self.method()`
//! call would be ambiguous. We use fully-qualified syntax
//! (`DuckDBStorage::method(self, ...)`) to unambiguously invoke the inherent
//! implementation.

use async_trait::async_trait;
use oxy_shared::errors::OxyError;

use crate::intent_types::IntentCluster;
use crate::store::ObservabilityStore;
use crate::types::{
    AgentExecutionStatsData, ClusterInfoRow, ClusterMapDataRow, ExecutionListData,
    ExecutionSummaryData, ExecutionTimeBucketData, IntentAnalyticsRow, MetricAnalyticsData,
    MetricDetailData, MetricUsageRecord, MetricsListData, SpanRecord, TraceDetailRow,
    TraceEnrichmentRow, TraceRow,
};

use super::DuckDBStorage;

#[async_trait]
impl ObservabilityStore for DuckDBStorage {
    // ── Traces ────────────────────────────────────────────────────────────

    async fn list_traces(
        &self,
        limit: i64,
        offset: i64,
        agent_ref: Option<&str>,
        status: Option<&str>,
        duration_filter: Option<&str>,
    ) -> Result<(Vec<TraceRow>, i64), OxyError> {
        DuckDBStorage::list_traces(self, limit, offset, agent_ref, status, duration_filter).await
    }

    async fn get_trace_detail(&self, trace_id: &str) -> Result<Vec<TraceDetailRow>, OxyError> {
        DuckDBStorage::get_trace_detail(self, trace_id).await
    }

    async fn get_cluster_map_data(
        &self,
        days: u32,
        limit: usize,
        source: Option<&str>,
    ) -> Result<Vec<ClusterMapDataRow>, OxyError> {
        DuckDBStorage::get_cluster_map_data(self, days, limit, source).await
    }

    async fn get_cluster_infos(&self) -> Result<Vec<ClusterInfoRow>, OxyError> {
        DuckDBStorage::get_cluster_infos(self).await
    }

    async fn get_trace_enrichments(
        &self,
        trace_ids: &[String],
    ) -> Result<Vec<TraceEnrichmentRow>, OxyError> {
        DuckDBStorage::get_trace_enrichments(self, trace_ids).await
    }

    // ── Intents ───────────────────────────────────────────────────────────

    async fn fetch_unprocessed_questions(
        &self,
        limit: usize,
    ) -> Result<Vec<(String, String, String)>, OxyError> {
        DuckDBStorage::fetch_unprocessed_questions(self, limit).await
    }

    async fn load_embeddings(
        &self,
    ) -> Result<Vec<(String, String, Vec<f32>, String, String)>, OxyError> {
        DuckDBStorage::load_embeddings(self).await
    }

    async fn store_clusters(&self, clusters: &[IntentCluster]) -> Result<(), OxyError> {
        DuckDBStorage::store_clusters(self, clusters).await
    }

    async fn load_clusters(&self) -> Result<Vec<IntentCluster>, OxyError> {
        DuckDBStorage::load_clusters(self).await
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
        DuckDBStorage::store_classification(
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
        DuckDBStorage::update_classification(
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
        DuckDBStorage::get_intent_analytics(self, days).await
    }

    async fn get_outliers(&self, limit: usize) -> Result<Vec<(String, String)>, OxyError> {
        DuckDBStorage::get_outliers(self, limit).await
    }

    async fn load_unknown_classifications(
        &self,
    ) -> Result<Vec<(String, String, Vec<f32>, String)>, OxyError> {
        DuckDBStorage::load_unknown_classifications(self).await
    }

    async fn get_unknown_count(&self) -> Result<usize, OxyError> {
        DuckDBStorage::get_unknown_count(self).await
    }

    async fn update_cluster_record(&self, cluster: &IntentCluster) -> Result<(), OxyError> {
        DuckDBStorage::update_cluster_record(self, cluster).await
    }

    async fn get_next_cluster_id(&self) -> Result<u32, OxyError> {
        DuckDBStorage::get_next_cluster_id(self).await
    }

    // ── Metrics ───────────────────────────────────────────────────────────

    async fn store_metric_usages(&self, metrics: Vec<MetricUsageRecord>) -> Result<(), OxyError> {
        DuckDBStorage::store_metric_usages(self, metrics).await
    }

    async fn get_metrics_analytics(&self, days: u32) -> Result<MetricAnalyticsData, OxyError> {
        DuckDBStorage::get_metrics_analytics(self, days).await
    }

    async fn get_metrics_list(
        &self,
        days: u32,
        limit: usize,
        offset: usize,
    ) -> Result<MetricsListData, OxyError> {
        DuckDBStorage::get_metrics_list(self, days, limit, offset).await
    }

    async fn get_metric_detail(
        &self,
        metric_name: &str,
        days: u32,
    ) -> Result<MetricDetailData, OxyError> {
        DuckDBStorage::get_metric_detail(self, metric_name, days).await
    }

    // ── Execution Analytics ───────────────────────────────────────────────

    async fn get_execution_summary(&self, days: u32) -> Result<ExecutionSummaryData, OxyError> {
        DuckDBStorage::get_execution_summary(self, days).await
    }

    async fn get_execution_time_series(
        &self,
        days: u32,
    ) -> Result<Vec<ExecutionTimeBucketData>, OxyError> {
        DuckDBStorage::get_execution_time_series(self, days).await
    }

    async fn get_execution_agent_stats(
        &self,
        days: u32,
        limit: usize,
    ) -> Result<Vec<AgentExecutionStatsData>, OxyError> {
        DuckDBStorage::get_execution_agent_stats(self, days, limit).await
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
        DuckDBStorage::get_execution_list(
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

    // ── Span Ingestion ─────────────────────────────────────────────────────

    async fn insert_spans(&self, spans: Vec<SpanRecord>) -> Result<(), OxyError> {
        // Send spans and flush immediately. The telemetry bridge already
        // batches, so we skip the writer's internal buffering to avoid
        // double-buffering latency.
        self.writer().send_spans(spans);
        self.writer().flush();
        Ok(())
    }

    // ── Retention ─────────────────────────────────────────────────────────

    async fn purge_older_than(&self, retention_days: u32) -> Result<u64, OxyError> {
        if retention_days == 0 {
            return Ok(0);
        }
        DuckDBStorage::purge_older_than(self, retention_days).await
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────

    async fn shutdown(&self) {
        DuckDBStorage::shutdown(self).await;
    }
}
