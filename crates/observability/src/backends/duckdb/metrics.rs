//! Metric usage query implementations for DuckDB storage.
//!
//! Provides methods for storing and querying metric usage data, including
//! analytics summaries, paginated lists, and per-metric detail views.

use std::sync::Arc;

use oxy_shared::errors::OxyError;

use super::DuckDBStorage;
use crate::types::{
    ContextTypeBreakdownData, MetricAnalyticsData, MetricDetailData, MetricListItem,
    MetricUsageRecord, MetricsListData, RecentUsageData, RelatedMetricData,
    SourceTypeBreakdownData, UsageTrendPointData,
};

// ── Queries ────────────────────────────────────────────────────────────────

impl DuckDBStorage {
    /// Store metric usage records via the writer.
    pub async fn store_metric_usages(
        &self,
        metrics: Vec<MetricUsageRecord>,
    ) -> Result<(), OxyError> {
        self.writer().send_metrics(metrics);
        Ok(())
    }

    /// Get analytics summary for the last N days.
    pub async fn get_metrics_analytics(&self, days: u32) -> Result<MetricAnalyticsData, OxyError> {
        let conn = Arc::clone(self.conn());

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            let interval = format!("{days} DAY");

            // Basic aggregates for the current period.
            let (total_queries, unique_metrics, avg_per_metric): (i64, i64, f64) = conn
                .query_row(
                    &format!(
                        "SELECT
                            count(*) as total,
                            count(DISTINCT metric_name) as uniq,
                            CASE WHEN count(DISTINCT metric_name) > 0
                                 THEN CAST(count(*) AS DOUBLE) / count(DISTINCT metric_name)
                                 ELSE 0.0 END as avg
                        FROM metric_usage
                        WHERE created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'"
                    ),
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .map_err(|e| OxyError::RuntimeError(format!("Aggregate query failed: {e}")))?;

            // Most popular metric.
            let most_popular_result: Result<(String, i64), _> = conn.query_row(
                &format!(
                    "SELECT metric_name, count(*) as cnt
                    FROM metric_usage
                    WHERE created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'
                    GROUP BY metric_name
                    ORDER BY cnt DESC
                    LIMIT 1"
                ),
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            );
            let (most_popular, most_popular_count) = match most_popular_result {
                Ok((name, count)) => (Some(name), Some(count as u64)),
                Err(_) => (None, None),
            };

            // Trend vs last period.
            let prev_count: i64 = conn
                .query_row(
                    &format!(
                        "SELECT count(*)
                        FROM metric_usage
                        WHERE created_at >= current_timestamp::TIMESTAMP - INTERVAL '{} DAY'
                          AND created_at < current_timestamp::TIMESTAMP - INTERVAL '{interval}'",
                        days * 2
                    ),
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            let trend = if prev_count > 0 {
                let pct = ((total_queries - prev_count) as f64 / prev_count as f64 * 100.0).round();
                if pct >= 0.0 {
                    Some(format!("+{pct}%"))
                } else {
                    Some(format!("{pct}%"))
                }
            } else if total_queries > 0 {
                Some("new".to_string())
            } else {
                None
            };

            // Source type breakdown.
            let by_source_type = {
                let mut agent = 0u64;
                let mut workflow = 0u64;
                let mut task = 0u64;
                let mut analytics = 0u64;

                let mut stmt = conn
                    .prepare(&format!(
                        "SELECT source_type, count(*) as cnt
                        FROM metric_usage
                        WHERE created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'
                        GROUP BY source_type"
                    ))
                    .map_err(|e| OxyError::RuntimeError(format!("Prepare failed: {e}")))?;

                let rows = stmt
                    .query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                    })
                    .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

                for row in rows {
                    let (st, cnt) =
                        row.map_err(|e| OxyError::RuntimeError(format!("Row read failed: {e}")))?;
                    match st.as_str() {
                        "agent" => agent = cnt as u64,
                        "workflow" => workflow = cnt as u64,
                        "task" => task = cnt as u64,
                        "analytics" => analytics = cnt as u64,
                        _ => {}
                    }
                }

                SourceTypeBreakdownData {
                    agent,
                    workflow,
                    task,
                    analytics,
                }
            };

            // Context type breakdown via lateral join on JSON array.
            let by_context_type = {
                let mut sql_count = 0u64;
                let mut semantic_query = 0u64;
                let mut question = 0u64;
                let mut response = 0u64;

                let mut stmt = conn
                    .prepare(&format!(
                        "SELECT ct.value as context_type, count(*) as cnt
                        FROM metric_usage, json_each(context_types) ct
                        WHERE created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'
                        GROUP BY context_type"
                    ))
                    .map_err(|e| OxyError::RuntimeError(format!("Prepare failed: {e}")))?;

                let rows = stmt
                    .query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                    })
                    .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

                for row in rows {
                    let (ct, cnt) =
                        row.map_err(|e| OxyError::RuntimeError(format!("Row read failed: {e}")))?;
                    // Context type values may be quoted from json_each; strip quotes.
                    let ct = ct.trim_matches('"').to_string();
                    match ct.as_str() {
                        "SQL" | "sql" => sql_count = cnt as u64,
                        "SemanticQuery" | "semantic_query" => semantic_query = cnt as u64,
                        "Question" | "question" => question = cnt as u64,
                        "Response" | "response" => response = cnt as u64,
                        _ => {}
                    }
                }

                ContextTypeBreakdownData {
                    sql: sql_count,
                    semantic_query,
                    question,
                    response,
                }
            };

            Ok(MetricAnalyticsData {
                total_queries: total_queries as u64,
                unique_metrics: unique_metrics as u64,
                avg_per_metric,
                most_popular,
                most_popular_count,
                trend_vs_last_period: trend,
                by_source_type,
                by_context_type,
            })
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }

    /// Get paginated metrics list.
    pub async fn get_metrics_list(
        &self,
        days: u32,
        limit: usize,
        offset: usize,
    ) -> Result<MetricsListData, OxyError> {
        let conn = Arc::clone(self.conn());

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            let interval = format!("{days} DAY");
            let limit_val = limit as i64;
            let offset_val = offset as i64;

            // Total count of distinct metrics.
            let total: i64 = conn
                .query_row(
                    &format!(
                        "SELECT count(DISTINCT metric_name)
                        FROM metric_usage
                        WHERE created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'"
                    ),
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| OxyError::RuntimeError(format!("Count query failed: {e}")))?;

            // Paginated metric list.
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT
                        metric_name,
                        count(*) as cnt,
                        strftime(max(created_at)::TIMESTAMP, '%Y-%m-%d') as last_used
                    FROM metric_usage
                    WHERE created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'
                    GROUP BY metric_name
                    ORDER BY cnt DESC
                    LIMIT ? OFFSET ?"
                ))
                .map_err(|e| OxyError::RuntimeError(format!("Prepare failed: {e}")))?;

            let rows = stmt
                .query_map(duckdb::params![limit_val, offset_val], |row| {
                    Ok(MetricListItem {
                        name: row.get(0)?,
                        count: row.get::<_, i64>(1)? as u64,
                        last_used: row.get(2)?,
                    })
                })
                .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

            let metrics: Vec<MetricListItem> = rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| OxyError::RuntimeError(format!("Row read failed: {e}")))?;

            Ok(MetricsListData {
                metrics,
                total: total as u64,
                limit,
                offset,
            })
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }

    /// Get detail for a specific metric.
    pub async fn get_metric_detail(
        &self,
        metric_name: &str,
        days: u32,
    ) -> Result<MetricDetailData, OxyError> {
        let conn = Arc::clone(self.conn());
        let metric_name = metric_name.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            let interval = format!("{days} DAY");
            let double_interval = format!("{} DAY", days * 2);

            // Combined aggregate: total, prev period count, by source type — single query.
            let (total_queries, prev_count, via_agent, via_workflow): (i64, i64, i64, i64) = conn
                .query_row(
                    &format!(
                        "SELECT
                            count_if(created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'),
                            count_if(
                                created_at >= current_timestamp::TIMESTAMP - INTERVAL '{double_interval}'
                                AND created_at < current_timestamp::TIMESTAMP - INTERVAL '{interval}'
                            ),
                            count_if(
                                source_type = 'agent'
                                AND created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'
                            ),
                            count_if(
                                source_type = 'workflow'
                                AND created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'
                            )
                        FROM metric_usage
                        WHERE metric_name = ?"
                    ),
                    duckdb::params![metric_name],
                    |row| Ok((
                        row.get::<_, Option<i64>>(0)?.unwrap_or(0),
                        row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                        row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                        row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                    )),
                )
                .map_err(|e| OxyError::RuntimeError(format!("Aggregate query failed: {e}")))?;

            let trend = if prev_count > 0 {
                let pct = ((total_queries - prev_count) as f64 / prev_count as f64 * 100.0).round();
                if pct >= 0.0 {
                    Some(format!("+{pct}%"))
                } else {
                    Some(format!("{pct}%"))
                }
            } else if total_queries > 0 {
                Some("new".to_string())
            } else {
                None
            };

            // Usage trend (daily counts).
            let mut trend_stmt = conn
                .prepare(&format!(
                    "SELECT
                        strftime(created_at::TIMESTAMP, '%Y-%m-%d') as date,
                        count(*) as cnt
                    FROM metric_usage
                    WHERE metric_name = ?
                      AND created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'
                    GROUP BY date
                    ORDER BY date ASC"
                ))
                .map_err(|e| OxyError::RuntimeError(format!("Prepare failed: {e}")))?;

            let trend_rows = trend_stmt
                .query_map(duckdb::params![metric_name], |row| {
                    Ok(UsageTrendPointData {
                        date: row.get(0)?,
                        count: row.get::<_, i64>(1)? as u64,
                    })
                })
                .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

            let usage_trend: Vec<UsageTrendPointData> =
                trend_rows
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| OxyError::RuntimeError(format!("Row read failed: {e}")))?;

            // Related metrics (co-occurring in the same trace).
            let mut related_stmt = conn
                .prepare(&format!(
                    "SELECT m2.metric_name, count(*) as co_count
                    FROM metric_usage m1
                    INNER JOIN metric_usage m2
                        ON m1.trace_id = m2.trace_id AND m1.metric_name != m2.metric_name
                    WHERE m1.metric_name = ?
                      AND m1.created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'
                    GROUP BY m2.metric_name
                    ORDER BY co_count DESC
                    LIMIT 10"
                ))
                .map_err(|e| OxyError::RuntimeError(format!("Prepare failed: {e}")))?;

            let related_rows = related_stmt
                .query_map(duckdb::params![metric_name], |row| {
                    Ok(RelatedMetricData {
                        name: row.get(0)?,
                        co_occurrence_count: row.get::<_, i64>(1)? as u64,
                    })
                })
                .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

            let related_metrics: Vec<RelatedMetricData> = related_rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| OxyError::RuntimeError(format!("Row read failed: {e}")))?;

            // Recent usage.
            let mut recent_stmt = conn
                .prepare(&format!(
                    "SELECT
                        source_type,
                        source_ref,
                        context_types,
                        trace_id,
                        strftime(created_at::TIMESTAMP, '%Y-%m-%d %H:%M:%S') as created_at,
                        context
                    FROM metric_usage
                    WHERE metric_name = ?
                      AND created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'
                    ORDER BY created_at DESC
                    LIMIT 20"
                ))
                .map_err(|e| OxyError::RuntimeError(format!("Prepare failed: {e}")))?;

            let recent_rows = recent_stmt
                .query_map(duckdb::params![metric_name], |row| {
                    Ok(RecentUsageData {
                        source_type: row.get(0)?,
                        source_ref: row.get(1)?,
                        context_types: row.get(2)?,
                        trace_id: row.get(3)?,
                        created_at: row.get(4)?,
                        context: row.get(5)?,
                    })
                })
                .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

            let recent_usage: Vec<RecentUsageData> = recent_rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| OxyError::RuntimeError(format!("Row read failed: {e}")))?;

            Ok(MetricDetailData {
                name: metric_name,
                total_queries: total_queries as u64,
                trend_vs_last_period: trend,
                via_agent: via_agent as u64,
                via_workflow: via_workflow as u64,
                usage_trend,
                related_metrics,
                recent_usage,
            })
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }
}
