//! Types for metric usage tracking

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Type of metric being tracked
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricType {
    /// A measure from the semantic layer (e.g., total_revenue, order_count)
    Measure,
    /// A dimension from the semantic layer (e.g., customer_segment, order_date)
    Dimension,
    /// Extracted via LLM from question/response/SQL
    Extracted,
}

impl MetricType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MetricType::Measure => "measure",
            MetricType::Dimension => "dimension",
            MetricType::Extracted => "extracted",
        }
    }
}

impl std::fmt::Display for MetricType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Source type for metric usage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    /// Metric used via agent execution
    Agent,
    /// Metric used via workflow execution
    Workflow,
    /// Metric used via direct task execution
    Task,
}

impl SourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceType::Agent => "Agent",
            SourceType::Workflow => "Workflow",
            SourceType::Task => "Task",
        }
    }
}

impl std::fmt::Display for SourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Context type indicating how the metric was referenced
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextType {
    /// Metric referenced in SQL query
    SQL,
    /// Metric referenced in semantic query params
    SemanticQuery,
    /// Metric referenced in user question
    Question,
    /// Metric referenced in response
    Response,
}

impl ContextType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContextType::SQL => "SQL",
            ContextType::SemanticQuery => "SemanticQuery",
            ContextType::Question => "Question",
            ContextType::Response => "Response",
        }
    }
}

impl std::fmt::Display for ContextType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A context item in the context JSON array
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem {
    /// Type of context (question, response, sql, semantic)
    #[serde(rename = "type")]
    pub context_type: String,
    /// Content of the context
    pub content: serde_json::Value,
}

/// Semantic context item grouping measures and dimensions by topic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticContextItem {
    /// Topic reference from semantic layer
    pub topic: Option<String>,
    /// Measures for this topic
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub measures: Vec<String>,
    /// Dimensions for this topic
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub dimensions: Vec<String>,
}

/// A single metric usage record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricUsage {
    /// Name of the metric (e.g., "orders.revenue", "customer_count")
    pub metric_name: String,
    /// Source type (Agent, Workflow, Task)
    pub source_type: SourceType,
    /// Reference to the source (e.g., "sales_agent", "weekly_report.yml")
    pub source_ref: String,
    /// Context types indicating how the metric was used
    pub context_types: Vec<ContextType>,
    /// OpenTelemetry trace ID for correlation
    pub trace_id: String,
    /// JSON array of all context items (question, response, sql, semantic)
    pub context: Option<String>,
}

/// Analytics summary for a single metric
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricAnalytics {
    /// Metric name
    pub name: String,
    /// Total usage count
    pub count: u64,
    /// Last used timestamp (ISO format)
    pub last_used: Option<String>,
    /// Trend percentage vs last period (e.g., "+18%")
    pub trend: Option<String>,
}

/// Breakdown by source type
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SourceTypeBreakdown {
    pub agent: u64,
    pub workflow: u64,
    pub task: u64,
}

/// Breakdown by context type
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ContextTypeBreakdown {
    pub sql: u64,
    pub semantic_query: u64,
    pub question: u64,
    pub response: u64,
}

/// Overall analytics response (summary stats only)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricAnalyticsResponse {
    /// Total queries tracked
    pub total_queries: u64,
    /// Number of unique metrics
    pub unique_metrics: u64,
    /// Average queries per metric
    pub avg_per_metric: f64,
    /// Most popular metric name
    pub most_popular: Option<String>,
    /// Most popular metric query count
    pub most_popular_count: Option<u64>,
    /// Trend vs last period (e.g., "+15%" or "-10%")
    pub trend_vs_last_period: Option<String>,
    /// Breakdown by source type
    pub by_source_type: SourceTypeBreakdown,
    /// Breakdown by context type
    pub by_context_type: ContextTypeBreakdown,
}

/// Paginated metrics list response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricsListResponse {
    /// Metrics for the current page
    pub metrics: Vec<MetricAnalytics>,
    /// Total number of metrics (for pagination)
    pub total: u64,
    /// Current page size
    pub limit: usize,
    /// Current offset
    pub offset: usize,
}

/// Usage trend data point
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UsageTrendPoint {
    /// Date (YYYY-MM-DD)
    pub date: String,
    /// Usage count on that date
    pub count: u64,
}

/// Related metric with co-occurrence score
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RelatedMetric {
    /// Related metric name
    pub name: String,
    /// Number of times they appear in the same trace
    pub co_occurrence_count: u64,
}

/// Recent usage record
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecentUsage {
    /// Source type
    pub source_type: String,
    /// Source reference
    pub source_ref: String,
    /// Context types (SQL, Question, Response, SemanticQuery) - can have multiple
    pub context_types: Vec<String>,
    /// The full context JSON (contains all context items for UI display)
    pub context: Option<String>,
    /// Trace ID
    pub trace_id: String,
    /// Timestamp
    pub created_at: String,
}

/// Detail response for a single metric
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricDetailResponse {
    /// Metric name
    pub name: String,
    /// Total queries
    pub total_queries: u64,
    /// Trend vs last period
    pub trend_vs_last_period: Option<String>,
    /// Usage via agents
    pub via_agent: u64,
    /// Usage via workflows
    pub via_workflow: u64,
    /// Usage trend over time
    pub usage_trend: Vec<UsageTrendPoint>,
    /// Related metrics
    pub related_metrics: Vec<RelatedMetric>,
    /// Recent usage examples
    pub recent_usage: Vec<RecentUsage>,
}
