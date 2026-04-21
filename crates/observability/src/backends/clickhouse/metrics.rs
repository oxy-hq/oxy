//! Metric usage queries against ClickHouse.

use clickhouse::Row;
use oxy_shared::errors::OxyError;
use serde::{Deserialize, Serialize};

use super::ClickHouseObservabilityStorage;
use crate::types::{
    ContextTypeBreakdownData, MetricAnalyticsData, MetricDetailData, MetricListItem,
    MetricUsageRecord, MetricsListData, RecentUsageData, RelatedMetricData,
    SourceTypeBreakdownData, UsageTrendPointData,
};

fn escape_sql_literal(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

#[derive(Debug, Serialize, Row)]
struct MetricUsageInsertRow {
    metric_name: String,
    source_type: String,
    source_ref: String,
    context: String,
    context_types: String,
    trace_id: String,
}

#[derive(Debug, Deserialize, Row)]
struct AggregateRow {
    total: u64,
    uniq: u64,
    avg: f64,
}

#[derive(Debug, Deserialize, Row)]
struct CountOnly {
    count: u64,
}

#[derive(Debug, Deserialize, Row)]
struct PopularRow {
    metric_name: String,
    cnt: u64,
}

#[derive(Debug, Deserialize, Row)]
struct SourceTypeRow {
    source_type: String,
    cnt: u64,
}

#[derive(Debug, Deserialize, Row)]
struct ContextTypeRow {
    context_type: String,
    cnt: u64,
}

#[derive(Debug, Deserialize, Row)]
struct MetricListQueryRow {
    metric_name: String,
    cnt: u64,
    last_used: String,
}

#[derive(Debug, Deserialize, Row)]
struct TrendPointRow {
    date: String,
    cnt: u64,
}

#[derive(Debug, Deserialize, Row)]
struct RelatedMetricRow {
    metric_name: String,
    co_count: u64,
}

#[derive(Debug, Deserialize, Row)]
struct RecentUsageRow {
    source_type: String,
    source_ref: String,
    context_types: String,
    trace_id: String,
    created_at: String,
    context: String,
}

pub(super) async fn store_metric_usages(
    storage: &ClickHouseObservabilityStorage,
    metrics: Vec<MetricUsageRecord>,
) -> Result<(), OxyError> {
    if metrics.is_empty() {
        return Ok(());
    }

    let mut insert = storage
        .client()
        .insert::<MetricUsageInsertRow>("observability_metric_usage")
        .await
        .map_err(|e| OxyError::RuntimeError(format!("ClickHouse insert init failed: {e}")))?;

    for m in metrics {
        let row = MetricUsageInsertRow {
            metric_name: m.metric_name,
            source_type: m.source_type,
            source_ref: m.source_ref,
            context: m.context,
            context_types: m.context_types,
            trace_id: m.trace_id,
        };
        insert
            .write(&row)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Metric write failed: {e}")))?;
    }

    insert
        .end()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Metric insert end failed: {e}")))?;

    Ok(())
}

pub(super) async fn get_metrics_analytics(
    storage: &ClickHouseObservabilityStorage,
    days: u32,
) -> Result<MetricAnalyticsData, OxyError> {
    let agg_sql = format!(
        "SELECT
            count() AS total,
            uniqExact(metric_name) AS uniq,
            if(uniqExact(metric_name) > 0,
               count() / uniqExact(metric_name),
               0.0) AS avg
        FROM observability_metric_usage
        WHERE created_at >= now() - INTERVAL {days} DAY"
    );

    let agg: AggregateRow = storage
        .client()
        .query(&agg_sql)
        .fetch_one()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Aggregate query failed: {e}")))?;

    let popular_sql = format!(
        "SELECT metric_name, count() AS cnt
        FROM observability_metric_usage
        WHERE created_at >= now() - INTERVAL {days} DAY
        GROUP BY metric_name
        ORDER BY cnt DESC
        LIMIT 1"
    );

    let popular = storage
        .client()
        .query(&popular_sql)
        .fetch_optional::<PopularRow>()
        .await
        .ok()
        .flatten();

    let (most_popular, most_popular_count) = match popular {
        Some(p) => (Some(p.metric_name), Some(p.cnt)),
        None => (None, None),
    };

    let prev_sql = format!(
        "SELECT count() AS count
        FROM observability_metric_usage
        WHERE created_at >= now() - INTERVAL {} DAY
          AND created_at < now() - INTERVAL {days} DAY",
        days * 2
    );

    let prev_count = storage
        .client()
        .query(&prev_sql)
        .fetch_optional::<CountOnly>()
        .await
        .ok()
        .flatten()
        .map(|r| r.count)
        .unwrap_or(0);

    let trend = if prev_count > 0 {
        let pct =
            ((agg.total as i64 - prev_count as i64) as f64 / prev_count as f64 * 100.0).round();
        if pct >= 0.0 {
            Some(format!("+{pct}%"))
        } else {
            Some(format!("{pct}%"))
        }
    } else if agg.total > 0 {
        Some("new".to_string())
    } else {
        None
    };

    let source_sql = format!(
        "SELECT source_type, count() AS cnt
        FROM observability_metric_usage
        WHERE created_at >= now() - INTERVAL {days} DAY
        GROUP BY source_type"
    );

    let source_rows: Vec<SourceTypeRow> = storage
        .client()
        .query(&source_sql)
        .fetch_all()
        .await
        .unwrap_or_default();

    let mut agent = 0u64;
    let mut workflow = 0u64;
    let mut task = 0u64;
    for r in &source_rows {
        match r.source_type.as_str() {
            "agent" => agent = r.cnt,
            "workflow" => workflow = r.cnt,
            "task" => task = r.cnt,
            _ => {}
        }
    }

    let ctx_sql = format!(
        "SELECT ct AS context_type, count() AS cnt
        FROM observability_metric_usage
        ARRAY JOIN JSONExtractArrayRaw(context_types) AS ct
        WHERE created_at >= now() - INTERVAL {days} DAY
        GROUP BY context_type"
    );

    let ctx_rows: Vec<ContextTypeRow> = storage
        .client()
        .query(&ctx_sql)
        .fetch_all()
        .await
        .unwrap_or_default();

    let mut sql_count = 0u64;
    let mut semantic_query = 0u64;
    let mut question = 0u64;
    let mut response = 0u64;
    for r in &ctx_rows {
        let ct = r.context_type.trim_matches('"');
        match ct {
            "SQL" | "sql" => sql_count = r.cnt,
            "SemanticQuery" | "semantic_query" => semantic_query = r.cnt,
            "Question" | "question" => question = r.cnt,
            "Response" | "response" => response = r.cnt,
            _ => {}
        }
    }

    Ok(MetricAnalyticsData {
        total_queries: agg.total,
        unique_metrics: agg.uniq,
        avg_per_metric: agg.avg,
        most_popular,
        most_popular_count,
        trend_vs_last_period: trend,
        by_source_type: SourceTypeBreakdownData {
            agent,
            workflow,
            task,
        },
        by_context_type: ContextTypeBreakdownData {
            sql: sql_count,
            semantic_query,
            question,
            response,
        },
    })
}

pub(super) async fn get_metrics_list(
    storage: &ClickHouseObservabilityStorage,
    days: u32,
    limit: usize,
    offset: usize,
) -> Result<MetricsListData, OxyError> {
    let count_sql = format!(
        "SELECT uniqExact(metric_name) AS count
        FROM observability_metric_usage
        WHERE created_at >= now() - INTERVAL {days} DAY"
    );

    let total = storage
        .client()
        .query(&count_sql)
        .fetch_one::<CountOnly>()
        .await
        .map(|r| r.count)
        .map_err(|e| OxyError::RuntimeError(format!("Count query failed: {e}")))?;

    let list_sql = format!(
        "SELECT
            metric_name,
            count() AS cnt,
            formatDateTime(max(created_at), '%Y-%m-%d') AS last_used
        FROM observability_metric_usage
        WHERE created_at >= now() - INTERVAL {days} DAY
        GROUP BY metric_name
        ORDER BY cnt DESC
        LIMIT {limit} OFFSET {offset}"
    );

    let rows: Vec<MetricListQueryRow> = storage
        .client()
        .query(&list_sql)
        .fetch_all()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Metrics list query failed: {e}")))?;

    let metrics = rows
        .into_iter()
        .map(|r| MetricListItem {
            name: r.metric_name,
            count: r.cnt,
            last_used: r.last_used,
        })
        .collect();

    Ok(MetricsListData {
        metrics,
        total,
        limit,
        offset,
    })
}

pub(super) async fn get_metric_detail(
    storage: &ClickHouseObservabilityStorage,
    metric_name: &str,
    days: u32,
) -> Result<MetricDetailData, OxyError> {
    let escaped = escape_sql_literal(metric_name);

    let total_sql = format!(
        "SELECT count() AS count
        FROM observability_metric_usage
        WHERE metric_name = '{escaped}'
          AND created_at >= now() - INTERVAL {days} DAY"
    );
    let total_queries = storage
        .client()
        .query(&total_sql)
        .fetch_one::<CountOnly>()
        .await
        .map(|r| r.count)
        .map_err(|e| OxyError::RuntimeError(format!("Total query failed: {e}")))?;

    let prev_sql = format!(
        "SELECT count() AS count
        FROM observability_metric_usage
        WHERE metric_name = '{escaped}'
          AND created_at >= now() - INTERVAL {} DAY
          AND created_at < now() - INTERVAL {days} DAY",
        days * 2
    );
    let prev_count = storage
        .client()
        .query(&prev_sql)
        .fetch_optional::<CountOnly>()
        .await
        .ok()
        .flatten()
        .map(|r| r.count)
        .unwrap_or(0);

    let trend = if prev_count > 0 {
        let pct =
            ((total_queries as i64 - prev_count as i64) as f64 / prev_count as f64 * 100.0).round();
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

    let via_agent_sql = format!(
        "SELECT count() AS count
        FROM observability_metric_usage
        WHERE metric_name = '{escaped}' AND source_type = 'agent'
          AND created_at >= now() - INTERVAL {days} DAY"
    );
    let via_agent = storage
        .client()
        .query(&via_agent_sql)
        .fetch_optional::<CountOnly>()
        .await
        .ok()
        .flatten()
        .map(|r| r.count)
        .unwrap_or(0);

    let via_workflow_sql = format!(
        "SELECT count() AS count
        FROM observability_metric_usage
        WHERE metric_name = '{escaped}' AND source_type = 'workflow'
          AND created_at >= now() - INTERVAL {days} DAY"
    );
    let via_workflow = storage
        .client()
        .query(&via_workflow_sql)
        .fetch_optional::<CountOnly>()
        .await
        .ok()
        .flatten()
        .map(|r| r.count)
        .unwrap_or(0);

    let trend_sql = format!(
        "SELECT
            formatDateTime(created_at, '%Y-%m-%d') AS date,
            count() AS cnt
        FROM observability_metric_usage
        WHERE metric_name = '{escaped}'
          AND created_at >= now() - INTERVAL {days} DAY
        GROUP BY date
        ORDER BY date ASC"
    );
    let usage_trend = storage
        .client()
        .query(&trend_sql)
        .fetch_all::<TrendPointRow>()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| UsageTrendPointData {
            date: r.date,
            count: r.cnt,
        })
        .collect();

    let related_sql = format!(
        "SELECT m2.metric_name AS metric_name, count() AS co_count
        FROM observability_metric_usage m1
        INNER JOIN observability_metric_usage m2
            ON m1.trace_id = m2.trace_id AND m1.metric_name != m2.metric_name
        WHERE m1.metric_name = '{escaped}'
          AND m1.created_at >= now() - INTERVAL {days} DAY
        GROUP BY m2.metric_name
        ORDER BY co_count DESC
        LIMIT 10"
    );
    let related_metrics = storage
        .client()
        .query(&related_sql)
        .fetch_all::<RelatedMetricRow>()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| RelatedMetricData {
            name: r.metric_name,
            co_occurrence_count: r.co_count,
        })
        .collect();

    let recent_sql = format!(
        "SELECT
            source_type,
            source_ref,
            context_types,
            trace_id,
            formatDateTime(created_at, '%Y-%m-%d %H:%M:%S') AS created_at,
            context
        FROM observability_metric_usage
        WHERE metric_name = '{escaped}'
          AND created_at >= now() - INTERVAL {days} DAY
        ORDER BY created_at DESC
        LIMIT 20"
    );
    let recent_usage = storage
        .client()
        .query(&recent_sql)
        .fetch_all::<RecentUsageRow>()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| RecentUsageData {
            source_type: r.source_type,
            source_ref: r.source_ref,
            context_types: r.context_types,
            trace_id: r.trace_id,
            created_at: r.created_at,
            context: r.context,
        })
        .collect();

    Ok(MetricDetailData {
        name: metric_name.to_string(),
        total_queries,
        trend_vs_last_period: trend,
        via_agent,
        via_workflow,
        usage_trend,
        related_metrics,
        recent_usage,
    })
}
