//! Postgres-backed implementation of [`crate::store::ObservabilityStore`].
//!
//! Uses the shared Sea-ORM `DatabaseConnection` (PostgreSQL) and raw SQL
//! queries against the `observability_*` tables created by the Sea-ORM
//! migration `m20260416_000001_create_observability_tables`.

mod execution_analytics;
mod intents;
mod metrics;
mod traces;

use async_trait::async_trait;
use oxy_shared::errors::OxyError;
use sea_orm::{ConnectionTrait, DatabaseConnection, Statement};

use crate::intent_types::IntentCluster;
use crate::store::ObservabilityStore;
use crate::types::{
    AgentExecutionStatsData, ClusterInfoRow, ClusterMapDataRow, ExecutionListData,
    ExecutionSummaryData, ExecutionTimeBucketData, IntentAnalyticsRow, MetricAnalyticsData,
    MetricDetailData, MetricUsageRecord, MetricsListData, SpanRecord, TraceDetailRow,
    TraceEnrichmentRow, TraceRow,
};

/// Postgres observability storage backend.
///
/// Wraps a `sea_orm::DatabaseConnection` and implements
/// [`crate::store::ObservabilityStore`] by executing raw SQL against the
/// `observability_*` tables.
pub struct PostgresObservabilityStorage {
    db: DatabaseConnection,
}

impl std::fmt::Debug for PostgresObservabilityStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresObservabilityStorage")
            .finish_non_exhaustive()
    }
}

impl PostgresObservabilityStorage {
    /// Create a new Postgres observability storage from an existing connection.
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Create a Postgres observability storage by connecting via
    /// `OXY_DATABASE_URL`. Intended for standalone contexts where the caller
    /// does not already manage a Sea-ORM connection pool.
    pub async fn from_env() -> Result<Self, OxyError> {
        let url = std::env::var("OXY_DATABASE_URL").map_err(|_| {
            OxyError::RuntimeError(
                "OXY_DATABASE_URL environment variable is required for the Postgres \
                 observability backend."
                    .into(),
            )
        })?;

        if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
            return Err(OxyError::RuntimeError(
                "OXY_DATABASE_URL must be a PostgreSQL connection string (starting with \
                 'postgres://' or 'postgresql://')"
                    .into(),
            ));
        }

        let db = sea_orm::Database::connect(url)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Postgres connect failed: {e}")))?;
        Ok(Self { db })
    }

    /// Accessor for the underlying Sea-ORM connection.
    pub(crate) fn db(&self) -> &DatabaseConnection {
        &self.db
    }
}

// ── Shared helpers ───────────────────────────────────────────────────────

pub(crate) fn pg() -> sea_orm::DatabaseBackend {
    sea_orm::DatabaseBackend::Postgres
}

/// Parse a Postgres REAL[] text representation (e.g. "{1.0,2.0,3.0}") into Vec<f32>.
pub(crate) fn parse_pg_float_array(s: &str) -> Vec<f32> {
    let trimmed = s.trim().trim_start_matches('{').trim_end_matches('}');
    if trimmed.is_empty() {
        return Vec::new();
    }
    trimmed
        .split(',')
        .filter_map(|v| v.trim().parse::<f32>().ok())
        .collect()
}

/// Format a Vec<f32> as a Postgres array literal: ARRAY[1.0,2.0,3.0]::REAL[]
///
/// Returns an error if any value is NaN or infinite.
pub(crate) fn format_pg_float_array(arr: &[f32]) -> Result<String, OxyError> {
    if arr.is_empty() {
        return Ok("ARRAY[]::REAL[]".to_string());
    }
    if arr.iter().any(|v| !v.is_finite()) {
        return Err(OxyError::RuntimeError(
            "Non-finite value in float array".into(),
        ));
    }
    Ok(format!(
        "ARRAY[{}]::REAL[]",
        arr.iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(",")
    ))
}

// ── Trait impl dispatch ───────────────────────────────────────────────────

#[async_trait]
impl ObservabilityStore for PostgresObservabilityStorage {
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

        for span in &spans {
            let sql = "INSERT INTO observability_spans
                 (trace_id, span_id, parent_span_id, span_name, service_name,
                  span_attributes, duration_ns, status_code, status_message,
                  event_data, timestamp)
                 VALUES ($1, $2, $3, $4, $5, $6::JSONB, $7, $8, $9, $10::JSONB, $11::TIMESTAMPTZ)
                 ON CONFLICT (trace_id, span_id) DO UPDATE SET
                    parent_span_id = EXCLUDED.parent_span_id,
                    span_name = EXCLUDED.span_name,
                    service_name = EXCLUDED.service_name,
                    span_attributes = EXCLUDED.span_attributes,
                    duration_ns = EXCLUDED.duration_ns,
                    status_code = EXCLUDED.status_code,
                    status_message = EXCLUDED.status_message,
                    event_data = EXCLUDED.event_data,
                    timestamp = EXCLUDED.timestamp";

            self.db
                .execute(Statement::from_sql_and_values(
                    pg(),
                    sql,
                    vec![
                        span.trace_id.clone().into(),
                        span.span_id.clone().into(),
                        span.parent_span_id.clone().into(),
                        span.span_name.clone().into(),
                        span.service_name.clone().into(),
                        span.span_attributes.clone().into(),
                        span.duration_ns.into(),
                        span.status_code.clone().into(),
                        span.status_message.clone().into(),
                        span.event_data.clone().into(),
                        span.timestamp.clone().into(),
                    ],
                ))
                .await
                .map_err(|e| OxyError::RuntimeError(format!("Insert span failed: {e}")))?;
        }

        Ok(())
    }

    async fn purge_older_than(&self, retention_days: u32) -> Result<u64, OxyError> {
        if retention_days == 0 {
            return Ok(0);
        }

        // retention_days is a trusted config value so `format!` is safe here.
        // Postgres DELETE returns row count via ExecResult::rows_affected().
        let tables = [
            ("observability_spans", "timestamp"),
            ("observability_intent_classifications", "classified_at"),
            ("observability_metric_usage", "created_at"),
        ];

        let mut total: u64 = 0;
        for (table, column) in tables {
            let sql = format!(
                "DELETE FROM {table} WHERE {column} < now() - INTERVAL '{retention_days} days'"
            );
            let res = self
                .db
                .execute(Statement::from_string(pg(), sql))
                .await
                .map_err(|e| OxyError::RuntimeError(format!("Purge {table} failed: {e}")))?;
            total = total.saturating_add(res.rows_affected());
        }
        Ok(total)
    }

    async fn shutdown(&self) {
        // Postgres uses a connection pool; no special shutdown needed.
        tracing::debug!("PostgresObservabilityStorage shutdown");
    }
}
