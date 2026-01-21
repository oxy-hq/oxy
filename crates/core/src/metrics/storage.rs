//! ClickHouse storage for metric usage data

use clickhouse::Row;
use serde::{Deserialize, Serialize};

use crate::storage::{ClickHouseConfig, ClickHouseStorage};
use oxy_shared::errors::OxyError;

use super::types::{
    ContextTypeBreakdown, MetricAnalytics, MetricAnalyticsResponse, MetricDetailResponse,
    MetricUsage, MetricsListResponse, RecentUsage, RelatedMetric, SourceTypeBreakdown,
    UsageTrendPoint,
};

/// Storage client for metric usage data
pub struct MetricStorage {
    storage: ClickHouseStorage,
}

impl std::fmt::Debug for MetricStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MetricStorage")
            .field("storage", &self.storage)
            .finish()
    }
}

/// Row type for writing metric usage
#[derive(Debug, Row, Serialize)]
struct MetricUsageWriteRow {
    // Note: Id and CreatedAt are omitted - they have DEFAULT values in ClickHouse
    // and the clickhouse-rs library handles them automatically
    #[serde(rename = "MetricName")]
    metric_name: String,
    #[serde(rename = "SourceType")]
    source_type: String,
    #[serde(rename = "SourceRef")]
    source_ref: String,
    #[serde(rename = "ContextTypes")]
    context_types: String,
    #[serde(rename = "TraceId")]
    trace_id: String,
    #[serde(rename = "Context")]
    context: String,
}

/// Row type for top metrics query
#[derive(Debug, Row, Deserialize)]
struct TopMetricRow {
    #[serde(rename = "MetricName")]
    metric_name: String,
    #[serde(rename = "Count")]
    count: u64,
    #[serde(rename = "LastUsed")]
    last_used: String,
}

/// Row type for source type breakdown
#[derive(Debug, Row, Deserialize)]
struct SourceBreakdownRow {
    #[serde(rename = "SourceType")]
    source_type: String,
    #[serde(rename = "Count")]
    count: u64,
}

/// Row type for context type breakdown
#[derive(Debug, Row, Deserialize)]
struct ContextBreakdownRow {
    #[serde(rename = "ContextType")]
    context_type: String,
    #[serde(rename = "Count")]
    count: u64,
}

/// Row type for trend data
#[derive(Debug, Row, Deserialize)]
struct TrendRow {
    #[serde(rename = "Date")]
    date: String,
    #[serde(rename = "Count")]
    count: u64,
}

/// Row type for related metrics
#[derive(Debug, Row, Deserialize)]
struct RelatedMetricRow {
    #[serde(rename = "RelatedMetric")]
    related_metric: String,
    #[serde(rename = "CoOccurrence")]
    co_occurrence: u64,
}

/// Row type for recent usage
#[derive(Debug, Row, Deserialize)]
struct RecentUsageRow {
    #[serde(rename = "SourceType")]
    source_type: String,
    #[serde(rename = "SourceRef")]
    source_ref: String,
    #[serde(rename = "ContextTypes")]
    context_types: String,
    #[serde(rename = "TraceId")]
    trace_id: String,
    #[serde(rename = "CreatedAtStr")]
    created_at: String,
    #[serde(rename = "Context")]
    context: String,
}

/// Row for count queries
#[derive(Debug, Row, Deserialize)]
struct CountRow {
    #[serde(rename = "Count")]
    count: u64,
}

/// Row for unique metrics count
#[derive(Debug, Row, Deserialize)]
struct UniqueCountRow {
    #[serde(rename = "UniqueCount")]
    unique_count: u64,
}

impl MetricStorage {
    /// Create a new storage client from config
    pub fn new(config: ClickHouseConfig) -> Self {
        Self {
            storage: ClickHouseStorage::new(config),
        }
    }

    /// Create from environment variables
    pub fn from_env() -> Self {
        Self {
            storage: ClickHouseStorage::from_env(),
        }
    }

    /// Store a batch of metric usage records
    pub async fn store_metrics(&self, metrics: &[MetricUsage]) -> Result<(), OxyError> {
        if metrics.is_empty() {
            return Ok(());
        }

        // Use the insert API which properly handles all escaping including ? characters
        // This avoids issues with the query() method treating ? as parameter placeholders
        let mut insert = self
            .storage
            .client()
            .insert::<MetricUsageWriteRow>("metric_usage")
            .map_err(|e| OxyError::RuntimeError(format!("Failed to create insert: {e}")))?;

        for m in metrics {
            // Convert context_types to JSON array string
            let context_types_json = serde_json::to_string(
                &m.context_types
                    .iter()
                    .map(|ct| ct.as_str())
                    .collect::<Vec<_>>(),
            )
            .unwrap_or_else(|_| "[]".to_string());

            let row = MetricUsageWriteRow {
                metric_name: m.metric_name.clone(),
                source_type: m.source_type.as_str().to_string(),
                source_ref: m.source_ref.clone(),
                context_types: context_types_json,
                trace_id: m.trace_id.clone(),
                context: m.context.clone().unwrap_or_default(),
            };
            insert
                .write(&row)
                .await
                .map_err(|e| OxyError::RuntimeError(format!("Failed to write row: {e}")))?;
        }

        insert
            .end()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to insert metrics: {e}")))?;

        Ok(())
    }

    /// Store a single metric usage record
    pub async fn store_metric(&self, metric: &MetricUsage) -> Result<(), OxyError> {
        self.store_metrics(&[metric.clone()]).await
    }

    /// Get analytics summary for a time period (without metrics list)
    pub async fn get_analytics(&self, days: u32) -> Result<MetricAnalyticsResponse, OxyError> {
        // Get total count for current period
        let total_query = format!(
            "SELECT count() as Count FROM metric_usage WHERE CreatedAt >= now() - INTERVAL {} DAY",
            days
        );
        let total_rows: Vec<CountRow> = self
            .storage
            .client()
            .query(&total_query)
            .fetch_all()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get total count: {e}")))?;
        let total_queries = total_rows.first().map(|r| r.count).unwrap_or(0);

        // Get total count for previous period (for trend calculation)
        let previous_period_query = format!(
            r#"
            SELECT count() as Count
            FROM metric_usage
            WHERE CreatedAt >= now() - INTERVAL {} DAY
              AND CreatedAt < now() - INTERVAL {} DAY
            "#,
            days * 2,
            days
        );
        let previous_rows: Vec<CountRow> = self
            .storage
            .client()
            .query(&previous_period_query)
            .fetch_all()
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!("Failed to get previous period total: {e}"))
            })?;
        let previous_queries = previous_rows.first().map(|r| r.count).unwrap_or(0);

        // Get unique metrics count
        let unique_query = format!(
            "SELECT uniq(MetricName) as UniqueCount FROM metric_usage WHERE CreatedAt >= now() - INTERVAL {} DAY",
            days
        );
        let unique_rows: Vec<UniqueCountRow> = self
            .storage
            .client()
            .query(&unique_query)
            .fetch_all()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get unique count: {e}")))?;
        let unique_metrics = unique_rows.first().map(|r| r.unique_count).unwrap_or(0);

        // Get most popular metric
        let most_popular_query = format!(
            r#"
            SELECT MetricName, count() as Count
            FROM metric_usage
            WHERE CreatedAt >= now() - INTERVAL {} DAY
            GROUP BY MetricName
            ORDER BY Count DESC
            LIMIT 1
            "#,
            days
        );
        #[derive(Debug, Row, Deserialize)]
        struct MostPopularRow {
            #[serde(rename = "MetricName")]
            metric_name: String,
            #[serde(rename = "Count")]
            count: u64,
        }
        let most_popular_rows: Vec<MostPopularRow> = self
            .storage
            .client()
            .query(&most_popular_query)
            .fetch_all()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get most popular: {e}")))?;
        let most_popular = most_popular_rows.first().map(|r| r.metric_name.clone());
        let most_popular_count = most_popular_rows.first().map(|r| r.count);

        // Get source type breakdown
        let source_query = format!(
            r#"
            SELECT SourceType, count() as Count
            FROM metric_usage
            WHERE CreatedAt >= now() - INTERVAL {} DAY
            GROUP BY SourceType
            "#,
            days
        );
        let source_rows: Vec<SourceBreakdownRow> = self
            .storage
            .client()
            .query(&source_query)
            .fetch_all()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get source breakdown: {e}")))?;

        let mut by_source_type = SourceTypeBreakdown {
            agent: 0,
            workflow: 0,
            task: 0,
        };
        for row in source_rows {
            match row.source_type.as_str() {
                "Agent" => by_source_type.agent = row.count,
                "Workflow" => by_source_type.workflow = row.count,
                "Task" => by_source_type.task = row.count,
                _ => {}
            }
        }

        // Get context type breakdown
        // Since ContextTypes is now a JSON array, we need to expand and count
        let context_query = format!(
            r#"
            SELECT 
                arrayJoin(JSONExtractArrayRaw(ContextTypes)) as ContextType, 
                count() as Count
            FROM metric_usage
            WHERE CreatedAt >= now() - INTERVAL {} DAY
            GROUP BY ContextType
            "#,
            days
        );
        let context_rows: Vec<ContextBreakdownRow> = self
            .storage
            .client()
            .query(&context_query)
            .fetch_all()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get context breakdown: {e}")))?;

        let mut by_context_type = ContextTypeBreakdown {
            sql: 0,
            semantic_query: 0,
            question: 0,
            response: 0,
        };
        for row in context_rows {
            // Remove quotes from JSON string values
            let ctx_type = row.context_type.trim_matches('"');
            match ctx_type {
                "SQL" => by_context_type.sql = row.count,
                "SemanticQuery" => by_context_type.semantic_query = row.count,
                "Question" => by_context_type.question = row.count,
                "Response" => by_context_type.response = row.count,
                _ => {}
            }
        }

        let avg_per_metric = if unique_metrics > 0 {
            total_queries as f64 / unique_metrics as f64
        } else {
            0.0
        };

        // Calculate trend vs last period
        let trend_vs_last_period = if previous_queries > 0 {
            let change = ((total_queries as f64 - previous_queries as f64)
                / previous_queries as f64)
                * 100.0;
            let sign = if change >= 0.0 { "+" } else { "" };
            Some(format!("{}{:.0}%", sign, change))
        } else if total_queries > 0 {
            Some("+100%".to_string()) // New data with no previous period
        } else {
            None // No data in either period
        };

        Ok(MetricAnalyticsResponse {
            total_queries,
            unique_metrics,
            avg_per_metric,
            most_popular,
            most_popular_count,
            trend_vs_last_period,
            by_source_type,
            by_context_type,
        })
    }

    /// Get paginated list of metrics
    pub async fn get_metrics_list(
        &self,
        days: u32,
        limit: usize,
        offset: usize,
    ) -> Result<MetricsListResponse, OxyError> {
        // Get unique metrics count for pagination
        let unique_query = format!(
            "SELECT uniq(MetricName) as UniqueCount FROM metric_usage WHERE CreatedAt >= now() - INTERVAL {} DAY",
            days
        );
        let unique_rows: Vec<UniqueCountRow> = self
            .storage
            .client()
            .query(&unique_query)
            .fetch_all()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get unique count: {e}")))?;
        let total = unique_rows.first().map(|r| r.unique_count).unwrap_or(0);

        // Get paginated metrics
        let metrics_query = format!(
            r#"
            SELECT
                MetricName,
                count() as Count,
                formatDateTime(max(CreatedAt), '%Y-%m-%d') as LastUsed
            FROM metric_usage
            WHERE CreatedAt >= now() - INTERVAL {} DAY
            GROUP BY MetricName
            ORDER BY Count DESC
            LIMIT {} OFFSET {}
            "#,
            days, limit, offset
        );
        let metric_rows: Vec<TopMetricRow> = self
            .storage
            .client()
            .query(&metrics_query)
            .fetch_all()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get metrics list: {e}")))?;

        let metrics: Vec<MetricAnalytics> = metric_rows
            .into_iter()
            .map(|r| MetricAnalytics {
                name: r.metric_name,
                count: r.count,
                last_used: Some(r.last_used),
                trend: None,
            })
            .collect();

        Ok(MetricsListResponse {
            metrics,
            total,
            limit,
            offset,
        })
    }

    /// Get detail for a specific metric
    pub async fn get_metric_detail(
        &self,
        metric_name: &str,
        days: u32,
    ) -> Result<MetricDetailResponse, OxyError> {
        let escaped_name = metric_name.replace('\'', "\\'");

        // Get total count for this metric in current period
        let total_query = format!(
            r#"
            SELECT count() as Count
            FROM metric_usage
            WHERE MetricName = '{}'
              AND CreatedAt >= now() - INTERVAL {} DAY
            "#,
            escaped_name, days
        );
        let total_rows: Vec<CountRow> = self
            .storage
            .client()
            .query(&total_query)
            .fetch_all()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get metric total: {e}")))?;
        let total_queries = total_rows.first().map(|r| r.count).unwrap_or(0);

        // Get total count for previous period (for trend calculation)
        let previous_period_query = format!(
            r#"
            SELECT count() as Count
            FROM metric_usage
            WHERE MetricName = '{}'
              AND CreatedAt >= now() - INTERVAL {} DAY
              AND CreatedAt < now() - INTERVAL {} DAY
            "#,
            escaped_name,
            days * 2,
            days
        );
        let previous_rows: Vec<CountRow> = self
            .storage
            .client()
            .query(&previous_period_query)
            .fetch_all()
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!("Failed to get previous period total: {e}"))
            })?;
        let previous_queries = previous_rows.first().map(|r| r.count).unwrap_or(0);

        // Get source breakdown for this metric
        let source_query = format!(
            r#"
            SELECT SourceType, count() as Count
            FROM metric_usage
            WHERE MetricName = '{}'
              AND CreatedAt >= now() - INTERVAL {} DAY
            GROUP BY SourceType
            "#,
            escaped_name, days
        );
        let source_rows: Vec<SourceBreakdownRow> = self
            .storage
            .client()
            .query(&source_query)
            .fetch_all()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get metric sources: {e}")))?;

        let mut via_agent = 0u64;
        let mut via_workflow = 0u64;
        for row in source_rows {
            match row.source_type.as_str() {
                "Agent" => via_agent = row.count,
                "Workflow" => via_workflow = row.count,
                _ => {}
            }
        }

        // Get usage trend
        let trend_query = format!(
            r#"
            SELECT
                formatDateTime(CreatedAt, '%Y-%m-%d') as Date,
                count() as Count
            FROM metric_usage
            WHERE MetricName = '{}'
              AND CreatedAt >= now() - INTERVAL {} DAY
            GROUP BY Date
            ORDER BY Date ASC
            "#,
            escaped_name, days
        );
        let trend_rows: Vec<TrendRow> = self
            .storage
            .client()
            .query(&trend_query)
            .fetch_all()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get metric trend: {e}")))?;

        let usage_trend: Vec<UsageTrendPoint> = trend_rows
            .into_iter()
            .map(|r| UsageTrendPoint {
                date: r.date,
                count: r.count,
            })
            .collect();

        // Get related metrics (co-occurrence in same trace)
        let related_query = format!(
            r#"
            SELECT
                m2.MetricName as RelatedMetric,
                count() as CoOccurrence
            FROM metric_usage m1
            JOIN metric_usage m2 ON m1.TraceId = m2.TraceId
            WHERE m1.MetricName = '{}'
              AND m2.MetricName != '{}'
              AND m1.CreatedAt >= now() - INTERVAL {} DAY
            GROUP BY RelatedMetric
            ORDER BY CoOccurrence DESC
            LIMIT 10
            "#,
            escaped_name, escaped_name, days
        );
        let related_rows: Vec<RelatedMetricRow> = self
            .storage
            .client()
            .query(&related_query)
            .fetch_all()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get related metrics: {e}")))?;

        let related_metrics: Vec<RelatedMetric> = related_rows
            .into_iter()
            .map(|r| RelatedMetric {
                name: r.related_metric,
                co_occurrence_count: r.co_occurrence,
            })
            .collect();

        // Get recent usage
        let recent_query = format!(
            r#"
            SELECT
                SourceType,
                SourceRef,
                ContextTypes,
                TraceId,
                formatDateTime(CreatedAt, '%Y-%m-%d %H:%i:%s') as CreatedAtStr,
                Context
            FROM metric_usage
            WHERE MetricName = '{}'
              AND CreatedAt >= now() - INTERVAL {} DAY
            ORDER BY CreatedAt DESC
            LIMIT 15
            "#,
            escaped_name, days
        );
        let recent_rows: Vec<RecentUsageRow> = self
            .storage
            .client()
            .query(&recent_query)
            .fetch_all()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get recent usage: {e}")))?;

        let recent_usage: Vec<RecentUsage> = recent_rows
            .into_iter()
            .map(|r| {
                // Parse context_types JSON array
                let context_types: Vec<String> =
                    serde_json::from_str(&r.context_types).unwrap_or_default();
                RecentUsage {
                    source_type: r.source_type,
                    source_ref: r.source_ref,
                    context_types,
                    context: if r.context.is_empty() {
                        None
                    } else {
                        Some(r.context)
                    },
                    trace_id: r.trace_id,
                    created_at: r.created_at,
                }
            })
            .collect();

        // Calculate trend vs last period
        let trend_vs_last_period = if previous_queries > 0 {
            let change = ((total_queries as f64 - previous_queries as f64)
                / previous_queries as f64)
                * 100.0;
            let sign = if change >= 0.0 { "+" } else { "" };
            Some(format!("{}{:.0}%", sign, change))
        } else if total_queries > 0 {
            Some("+100%".to_string()) // New metric with no previous data
        } else {
            None // No data in either period
        };

        Ok(MetricDetailResponse {
            name: metric_name.to_string(),
            total_queries,
            trend_vs_last_period,
            via_agent,
            via_workflow,
            usage_trend,
            related_metrics,
            recent_usage,
        })
    }
}
