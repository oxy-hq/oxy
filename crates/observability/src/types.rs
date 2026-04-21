//! Shared types used across all observability backends.
//!
//! These types form the "row" and "record" shapes exchanged between the
//! [`crate::store::ObservabilityStore`] trait and its backend implementations.
//! They are intentionally free of any backend-specific types so they can live
//! at a neutral location and be imported by DuckDB, Postgres, and ClickHouse
//! backends alike.

// ── Write records ──────────────────────────────────────────────────────────

/// A single span record to be inserted into the backing store.
#[derive(Debug, Clone)]
pub struct SpanRecord {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: String,
    pub span_name: String,
    pub service_name: String,
    /// JSON object string, e.g. `{"key": "value"}`
    pub span_attributes: String,
    pub duration_ns: i64,
    pub status_code: String,
    pub status_message: String,
    /// JSON array of event objects, e.g. `[{"name":"evt","attributes":{}}]`
    pub event_data: String,
    /// ISO 8601 timestamp string
    pub timestamp: String,
}

/// A metric usage record to be inserted into the metric usage table.
#[derive(Debug, Clone)]
pub struct MetricUsageRecord {
    pub metric_name: String,
    pub source_type: String,
    pub source_ref: String,
    pub context: String,
    /// JSON array string, e.g. `["type1", "type2"]`
    pub context_types: String,
    pub trace_id: String,
}

/// An intent classification record.
#[derive(Debug, Clone)]
pub struct ClassificationRecord {
    pub trace_id: String,
    pub question: String,
    pub cluster_id: i32,
    pub intent_name: String,
    pub confidence: f32,
    pub embedding: Vec<f32>,
    pub source_type: String,
    pub source: String,
}

/// An intent cluster record (upserted via `INSERT OR REPLACE`).
#[derive(Debug, Clone)]
pub struct ClusterRecord {
    pub cluster_id: i32,
    pub intent_name: String,
    pub intent_description: String,
    pub centroid: Vec<f32>,
    /// JSON array string of sample questions
    pub sample_questions: String,
    pub question_count: i64,
}

// ── Trace query rows ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TraceRow {
    pub trace_id: String,
    pub span_id: String,
    pub timestamp: String,
    pub span_name: String,
    pub service_name: String,
    pub duration_ns: i64,
    pub status_code: String,
    pub status_message: String,
    pub span_attributes: String,
    pub event_data: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
}

#[derive(Debug, Clone)]
pub struct TraceDetailRow {
    pub timestamp: String,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: String,
    pub span_name: String,
    pub service_name: String,
    pub span_attributes: String,
    pub duration_ns: i64,
    pub status_code: String,
    pub status_message: String,
    pub event_data: String,
}

#[derive(Debug, Clone)]
pub struct ClusterMapDataRow {
    pub trace_id: String,
    pub question: String,
    pub embedding: Vec<f32>,
    pub cluster_id: i32,
    pub intent_name: String,
    pub confidence: f32,
    pub classified_at: String,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct ClusterInfoRow {
    pub cluster_id: i32,
    pub intent_name: String,
    pub intent_description: String,
    pub sample_questions: String,
}

#[derive(Debug, Clone)]
pub struct TraceEnrichmentRow {
    pub trace_id: String,
    pub status_code: String,
    pub duration_ns: i64,
}

// ── Intent analytics rows ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct IntentAnalyticsRow {
    pub intent_name: String,
    pub count: u64,
}

// ── Metric analytics result types ──────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MetricAnalyticsData {
    pub total_queries: u64,
    pub unique_metrics: u64,
    pub avg_per_metric: f64,
    pub most_popular: Option<String>,
    pub most_popular_count: Option<u64>,
    pub trend_vs_last_period: Option<String>,
    pub by_source_type: SourceTypeBreakdownData,
    pub by_context_type: ContextTypeBreakdownData,
}

#[derive(Debug, Clone)]
pub struct SourceTypeBreakdownData {
    pub agent: u64,
    pub workflow: u64,
    pub task: u64,
    pub analytics: u64,
}

#[derive(Debug, Clone)]
pub struct ContextTypeBreakdownData {
    pub sql: u64,
    pub semantic_query: u64,
    pub question: u64,
    pub response: u64,
}

#[derive(Debug, Clone)]
pub struct MetricListItem {
    pub name: String,
    pub count: u64,
    pub last_used: String,
}

#[derive(Debug, Clone)]
pub struct MetricsListData {
    pub metrics: Vec<MetricListItem>,
    pub total: u64,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone)]
pub struct UsageTrendPointData {
    pub date: String,
    pub count: u64,
}

#[derive(Debug, Clone)]
pub struct RelatedMetricData {
    pub name: String,
    pub co_occurrence_count: u64,
}

#[derive(Debug, Clone)]
pub struct RecentUsageData {
    pub source_type: String,
    pub source_ref: String,
    pub context_types: String,
    pub trace_id: String,
    pub created_at: String,
    pub context: String,
}

#[derive(Debug, Clone)]
pub struct MetricDetailData {
    pub name: String,
    pub total_queries: u64,
    pub trend_vs_last_period: Option<String>,
    pub via_agent: u64,
    pub via_workflow: u64,
    pub usage_trend: Vec<UsageTrendPointData>,
    pub related_metrics: Vec<RelatedMetricData>,
    pub recent_usage: Vec<RecentUsageData>,
}

// ── Execution analytics result types ───────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ExecutionSummaryData {
    pub total_executions: u64,
    pub verified_count: u64,
    pub generated_count: u64,
    pub success_count_verified: u64,
    pub success_count_generated: u64,
    pub semantic_query_count: u64,
    pub omni_query_count: u64,
    pub sql_generated_count: u64,
    pub workflow_count: u64,
    pub agent_tool_count: u64,
}

#[derive(Debug, Clone)]
pub struct ExecutionTimeBucketData {
    pub date: String,
    pub verified_count: u64,
    pub generated_count: u64,
    pub semantic_query_count: u64,
    pub omni_query_count: u64,
    pub sql_generated_count: u64,
    pub workflow_count: u64,
    pub agent_tool_count: u64,
}

#[derive(Debug, Clone)]
pub struct AgentExecutionStatsData {
    pub agent_ref: String,
    pub total_executions: u64,
    pub verified_count: u64,
    pub generated_count: u64,
    pub success_count: u64,
    pub semantic_query_count: u64,
    pub omni_query_count: u64,
    pub sql_generated_count: u64,
    pub workflow_count: u64,
    pub agent_tool_count: u64,
}

#[derive(Debug, Clone)]
pub struct ExecutionDetailData {
    pub trace_id: String,
    pub span_id: String,
    pub timestamp: String,
    pub execution_type: String,
    pub is_verified: String,
    pub source_type: String,
    pub source_ref: String,
    pub status: String,
    pub duration_ns: i64,
    pub database: String,
    pub topic: String,
    pub semantic_query_params: String,
    pub generated_sql: String,
    pub integration: String,
    pub endpoint: String,
    pub sql: String,
    pub sql_ref: String,
    pub user_question: String,
    pub workflow_ref: String,
    pub agent_ref: String,
    pub tool_input: String,
    pub input: String,
    pub output: String,
    pub error: String,
}

#[derive(Debug, Clone)]
pub struct ExecutionListData {
    pub executions: Vec<ExecutionDetailData>,
    pub total: u64,
    pub limit: usize,
    pub offset: usize,
}
