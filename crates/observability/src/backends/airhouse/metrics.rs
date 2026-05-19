//! Metric usage query functions for the Airhouse observability backend.

use oxy_shared::errors::OxyError;
use tokio_postgres::SimpleQueryMessage;

use super::{AirhouseObservabilityStorage, esc, get_f64, get_i64, get_str, get_u64};
use crate::types::{
    ContextTypeBreakdownData, MetricAnalyticsData, MetricDetailData, MetricListItem,
    MetricUsageRecord, MetricsListData, RecentUsageData, RelatedMetricData,
    SourceTypeBreakdownData, UsageTrendPointData,
};

fn rows(messages: &[SimpleQueryMessage]) -> impl Iterator<Item = &tokio_postgres::SimpleQueryRow> {
    messages.iter().filter_map(|m| match m {
        SimpleQueryMessage::Row(r) => Some(r),
        _ => None,
    })
}

pub async fn store_metric_usages(
    storage: &AirhouseObservabilityStorage,
    metrics: Vec<MetricUsageRecord>,
) -> Result<(), OxyError> {
    for m in metrics {
        let sql = format!(
            "INSERT INTO oxy_obs_metric_usage
             (metric_name, source_type, source_ref, context, context_types, trace_id)
             VALUES ('{}', '{}', '{}', '{}', '{}', '{}')",
            esc(&m.metric_name),
            esc(&m.source_type),
            esc(&m.source_ref),
            esc(&m.context),
            esc(&m.context_types),
            esc(&m.trace_id),
        );
        storage.execute(&sql).await?;
    }
    Ok(())
}

pub async fn get_metrics_analytics(
    storage: &AirhouseObservabilityStorage,
    days: u32,
) -> Result<MetricAnalyticsData, OxyError> {
    let interval = format!("{days} DAY");
    let double_interval = format!("{} DAY", days * 2);

    let agg_sql = format!(
        "SELECT
            count(*) AS total,
            count(DISTINCT metric_name) AS uniq,
            CASE WHEN count(DISTINCT metric_name) > 0
                 THEN CAST(count(*) AS DOUBLE) / count(DISTINCT metric_name)
                 ELSE 0.0 END AS avg_per
        FROM oxy_obs_metric_usage
        WHERE created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'"
    );
    let agg_msgs = storage.query(&agg_sql).await?;
    let (total_queries, unique_metrics, avg_per_metric) = rows(&agg_msgs)
        .next()
        .map(|r| (get_i64(r, "total"), get_i64(r, "uniq"), get_f64(r, "avg_per")))
        .unwrap_or((0, 0, 0.0));

    let popular_sql = format!(
        "SELECT metric_name, count(*) AS cnt
        FROM oxy_obs_metric_usage
        WHERE created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'
        GROUP BY metric_name
        ORDER BY cnt DESC
        LIMIT 1"
    );
    let popular_msgs = storage.query(&popular_sql).await?;
    let (most_popular, most_popular_count) = rows(&popular_msgs)
        .next()
        .map(|r| (Some(get_str(r, "metric_name")), Some(get_u64(r, "cnt"))))
        .unwrap_or((None, None));

    let prev_sql = format!(
        "SELECT count(*) AS n
        FROM oxy_obs_metric_usage
        WHERE created_at >= current_timestamp::TIMESTAMP - INTERVAL '{double_interval}'
          AND created_at < current_timestamp::TIMESTAMP - INTERVAL '{interval}'"
    );
    let prev_msgs = storage.query(&prev_sql).await?;
    let prev_count = rows(&prev_msgs)
        .next()
        .map(|r| get_i64(r, "n"))
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

    let src_sql = format!(
        "SELECT source_type, count(*) AS cnt
        FROM oxy_obs_metric_usage
        WHERE created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'
        GROUP BY source_type"
    );
    let src_msgs = storage.query(&src_sql).await?;
    let mut agent = 0u64;
    let mut workflow = 0u64;
    let mut task = 0u64;
    let mut analytics = 0u64;
    for r in rows(&src_msgs) {
        match get_str(r, "source_type").as_str() {
            "agent" => agent = get_u64(r, "cnt"),
            "workflow" => workflow = get_u64(r, "cnt"),
            "task" => task = get_u64(r, "cnt"),
            "analytics" => analytics = get_u64(r, "cnt"),
            _ => {}
        }
    }

    let ctx_sql = format!(
        "SELECT ct.value AS context_type, count(*) AS cnt
        FROM oxy_obs_metric_usage, json_each(context_types) ct
        WHERE created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'
        GROUP BY context_type"
    );
    let ctx_msgs = storage.query(&ctx_sql).await?;
    let mut sql_count = 0u64;
    let mut semantic_query = 0u64;
    let mut question = 0u64;
    let mut response = 0u64;
    for r in rows(&ctx_msgs) {
        let ct = get_str(r, "context_type");
        let ct = ct.trim_matches('"');
        match ct {
            "SQL" | "sql" => sql_count = get_u64(r, "cnt"),
            "SemanticQuery" | "semantic_query" => semantic_query = get_u64(r, "cnt"),
            "Question" | "question" => question = get_u64(r, "cnt"),
            "Response" | "response" => response = get_u64(r, "cnt"),
            _ => {}
        }
    }

    Ok(MetricAnalyticsData {
        total_queries: total_queries as u64,
        unique_metrics: unique_metrics as u64,
        avg_per_metric,
        most_popular,
        most_popular_count,
        trend_vs_last_period: trend,
        by_source_type: SourceTypeBreakdownData { agent, workflow, task, analytics },
        by_context_type: ContextTypeBreakdownData {
            sql: sql_count,
            semantic_query,
            question,
            response,
        },
    })
}

pub async fn get_metrics_list(
    storage: &AirhouseObservabilityStorage,
    days: u32,
    limit: usize,
    offset: usize,
) -> Result<MetricsListData, OxyError> {
    let interval = format!("{days} DAY");

    let count_sql = format!(
        "SELECT count(DISTINCT metric_name) AS n
        FROM oxy_obs_metric_usage
        WHERE created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'"
    );
    let count_msgs = storage.query(&count_sql).await?;
    let total = rows(&count_msgs)
        .next()
        .map(|r| get_u64(r, "n"))
        .unwrap_or(0);

    let list_sql = format!(
        "SELECT
            metric_name,
            count(*) AS cnt,
            strftime(max(created_at)::TIMESTAMP, '%Y-%m-%d') AS last_used
        FROM oxy_obs_metric_usage
        WHERE created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'
        GROUP BY metric_name
        ORDER BY cnt DESC
        LIMIT {limit} OFFSET {offset}"
    );
    let list_msgs = storage.query(&list_sql).await?;
    let metrics = rows(&list_msgs)
        .map(|r| MetricListItem {
            name: get_str(r, "metric_name"),
            count: get_u64(r, "cnt"),
            last_used: get_str(r, "last_used"),
        })
        .collect();

    Ok(MetricsListData { metrics, total, limit, offset })
}

pub async fn get_metric_detail(
    storage: &AirhouseObservabilityStorage,
    metric_name: &str,
    days: u32,
) -> Result<MetricDetailData, OxyError> {
    let interval = format!("{days} DAY");
    let double_interval = format!("{} DAY", days * 2);
    let mname = esc(metric_name);

    let agg_sql = format!(
        "SELECT
            count_if(created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}') AS total,
            count_if(
                created_at >= current_timestamp::TIMESTAMP - INTERVAL '{double_interval}'
                AND created_at < current_timestamp::TIMESTAMP - INTERVAL '{interval}'
            ) AS prev,
            count_if(
                source_type = 'agent'
                AND created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'
            ) AS via_agent,
            count_if(
                source_type = 'workflow'
                AND created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'
            ) AS via_workflow
        FROM oxy_obs_metric_usage
        WHERE metric_name = '{mname}'"
    );
    let agg_msgs = storage.query(&agg_sql).await?;
    let (total_queries, prev_count, via_agent, via_workflow) = rows(&agg_msgs)
        .next()
        .map(|r| (get_i64(r, "total"), get_i64(r, "prev"), get_i64(r, "via_agent"), get_i64(r, "via_workflow")))
        .unwrap_or((0, 0, 0, 0));

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

    let trend_sql = format!(
        "SELECT strftime(created_at::TIMESTAMP, '%Y-%m-%d') AS date, count(*) AS cnt
        FROM oxy_obs_metric_usage
        WHERE metric_name = '{mname}'
          AND created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'
        GROUP BY date
        ORDER BY date ASC"
    );
    let trend_msgs = storage.query(&trend_sql).await?;
    let usage_trend: Vec<UsageTrendPointData> = rows(&trend_msgs)
        .map(|r| UsageTrendPointData { date: get_str(r, "date"), count: get_u64(r, "cnt") })
        .collect();

    let related_sql = format!(
        "SELECT m2.metric_name, count(*) AS co_count
        FROM oxy_obs_metric_usage m1
        INNER JOIN oxy_obs_metric_usage m2
            ON m1.trace_id = m2.trace_id AND m1.metric_name != m2.metric_name
        WHERE m1.metric_name = '{mname}'
          AND m1.created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'
        GROUP BY m2.metric_name
        ORDER BY co_count DESC
        LIMIT 10"
    );
    let related_msgs = storage.query(&related_sql).await?;
    let related_metrics: Vec<RelatedMetricData> = rows(&related_msgs)
        .map(|r| RelatedMetricData {
            name: get_str(r, "metric_name"),
            co_occurrence_count: get_u64(r, "co_count"),
        })
        .collect();

    let recent_sql = format!(
        "SELECT
            source_type, source_ref, context_types, trace_id,
            strftime(created_at::TIMESTAMP, '%Y-%m-%d %H:%M:%S') AS created_at,
            context
        FROM oxy_obs_metric_usage
        WHERE metric_name = '{mname}'
          AND created_at >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'
        ORDER BY created_at DESC
        LIMIT 20"
    );
    let recent_msgs = storage.query(&recent_sql).await?;
    let recent_usage: Vec<RecentUsageData> = rows(&recent_msgs)
        .map(|r| RecentUsageData {
            source_type: get_str(r, "source_type"),
            source_ref: get_str(r, "source_ref"),
            context_types: get_str(r, "context_types"),
            trace_id: get_str(r, "trace_id"),
            created_at: get_str(r, "created_at"),
            context: get_str(r, "context"),
        })
        .collect();

    Ok(MetricDetailData {
        name: metric_name.to_string(),
        total_queries: total_queries as u64,
        trend_vs_last_period: trend,
        via_agent: via_agent as u64,
        via_workflow: via_workflow as u64,
        usage_trend,
        related_metrics,
        recent_usage,
    })
}
