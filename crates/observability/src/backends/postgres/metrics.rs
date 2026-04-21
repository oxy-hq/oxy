//! Metric usage queries against Postgres observability tables.

use oxy_shared::errors::OxyError;
use sea_orm::{ConnectionTrait, FromQueryResult, Statement};

use super::{PostgresObservabilityStorage, pg};
use crate::types::{
    ContextTypeBreakdownData, MetricAnalyticsData, MetricDetailData, MetricListItem,
    MetricUsageRecord, MetricsListData, RecentUsageData, RelatedMetricData,
    SourceTypeBreakdownData, UsageTrendPointData,
};

#[derive(Debug, FromQueryResult)]
struct CountRow {
    count: i64,
}

#[derive(Debug, FromQueryResult)]
struct AggregateRow {
    total: i64,
    uniq: i64,
    avg: f64,
}

#[derive(Debug, FromQueryResult)]
struct PopularRow {
    metric_name: String,
    cnt: i64,
}

#[derive(Debug, FromQueryResult)]
struct SourceTypeRow {
    source_type: String,
    cnt: i64,
}

#[derive(Debug, FromQueryResult)]
struct ContextTypeRow {
    context_type: String,
    cnt: i64,
}

#[derive(Debug, FromQueryResult)]
struct MetricListQueryRow {
    metric_name: String,
    cnt: i64,
    last_used: String,
}

#[derive(Debug, FromQueryResult)]
struct TrendPointRow {
    date: String,
    cnt: i64,
}

#[derive(Debug, FromQueryResult)]
struct RelatedMetricRow {
    metric_name: String,
    co_count: i64,
}

#[derive(Debug, FromQueryResult)]
struct RecentUsageRow {
    source_type: String,
    source_ref: String,
    context_types: String,
    trace_id: String,
    created_at: String,
    context: String,
}

pub(super) async fn store_metric_usages(
    storage: &PostgresObservabilityStorage,
    metrics: Vec<MetricUsageRecord>,
) -> Result<(), OxyError> {
    if metrics.is_empty() {
        return Ok(());
    }

    let db = storage.db();

    for m in &metrics {
        let sql = "INSERT INTO observability_metric_usage
             (metric_name, source_type, source_ref, context, context_types, trace_id)
             VALUES ($1, $2, $3, $4, $5::JSONB, $6)";

        db.execute(Statement::from_sql_and_values(
            pg(),
            sql,
            vec![
                m.metric_name.clone().into(),
                m.source_type.clone().into(),
                m.source_ref.clone().into(),
                m.context.clone().into(),
                m.context_types.clone().into(),
                m.trace_id.clone().into(),
            ],
        ))
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Insert metric usage failed: {e}")))?;
    }

    Ok(())
}

pub(super) async fn get_metrics_analytics(
    storage: &PostgresObservabilityStorage,
    days: u32,
) -> Result<MetricAnalyticsData, OxyError> {
    let db = storage.db();
    let interval = format!("{days} days");

    let agg_sql = format!(
        "SELECT
            count(*)::BIGINT AS total,
            count(DISTINCT metric_name)::BIGINT AS uniq,
            CASE WHEN count(DISTINCT metric_name) > 0
                 THEN count(*)::DOUBLE PRECISION / count(DISTINCT metric_name)
                 ELSE 0.0 END AS avg
        FROM observability_metric_usage
        WHERE created_at >= now() - INTERVAL '{interval}'"
    );

    let agg =
        AggregateRow::find_by_statement(Statement::from_sql_and_values(pg(), &agg_sql, vec![]))
            .one(db)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Aggregate query failed: {e}")))?
            .unwrap_or(AggregateRow {
                total: 0,
                uniq: 0,
                avg: 0.0,
            });

    let popular_sql = format!(
        "SELECT metric_name, count(*)::BIGINT AS cnt
        FROM observability_metric_usage
        WHERE created_at >= now() - INTERVAL '{interval}'
        GROUP BY metric_name
        ORDER BY cnt DESC
        LIMIT 1"
    );

    let popular =
        PopularRow::find_by_statement(Statement::from_sql_and_values(pg(), &popular_sql, vec![]))
            .one(db)
            .await
            .ok()
            .flatten();

    let (most_popular, most_popular_count) = match popular {
        Some(p) => (Some(p.metric_name), Some(p.cnt as u64)),
        None => (None, None),
    };

    let prev_sql = format!(
        "SELECT count(*)::BIGINT AS count
        FROM observability_metric_usage
        WHERE created_at >= now() - INTERVAL '{} days'
          AND created_at < now() - INTERVAL '{interval}'",
        days * 2
    );

    let prev_count =
        CountRow::find_by_statement(Statement::from_sql_and_values(pg(), &prev_sql, vec![]))
            .one(db)
            .await
            .ok()
            .flatten()
            .map(|r| r.count)
            .unwrap_or(0);

    let trend = if prev_count > 0 {
        let pct = ((agg.total - prev_count) as f64 / prev_count as f64 * 100.0).round();
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
        "SELECT source_type, count(*)::BIGINT AS cnt
        FROM observability_metric_usage
        WHERE created_at >= now() - INTERVAL '{interval}'
        GROUP BY source_type"
    );

    let source_rows =
        SourceTypeRow::find_by_statement(Statement::from_sql_and_values(pg(), &source_sql, vec![]))
            .all(db)
            .await
            .unwrap_or_default();

    let mut agent = 0u64;
    let mut workflow = 0u64;
    let mut task = 0u64;
    let mut analytics = 0u64;
    for r in &source_rows {
        match r.source_type.as_str() {
            "agent" => agent = r.cnt as u64,
            "workflow" => workflow = r.cnt as u64,
            "task" => task = r.cnt as u64,
            "analytics" => analytics = r.cnt as u64,
            _ => {}
        }
    }

    let ctx_sql = format!(
        "SELECT ct AS context_type, count(*)::BIGINT AS cnt
        FROM observability_metric_usage, jsonb_array_elements_text(context_types) ct
        WHERE created_at >= now() - INTERVAL '{interval}'
        GROUP BY context_type"
    );

    let ctx_rows =
        ContextTypeRow::find_by_statement(Statement::from_sql_and_values(pg(), &ctx_sql, vec![]))
            .all(db)
            .await
            .unwrap_or_default();

    let mut sql_count = 0u64;
    let mut semantic_query = 0u64;
    let mut question = 0u64;
    let mut response = 0u64;
    for r in &ctx_rows {
        let ct = r.context_type.trim_matches('"');
        match ct {
            "SQL" | "sql" => sql_count = r.cnt as u64,
            "SemanticQuery" | "semantic_query" => semantic_query = r.cnt as u64,
            "Question" | "question" => question = r.cnt as u64,
            "Response" | "response" => response = r.cnt as u64,
            _ => {}
        }
    }

    Ok(MetricAnalyticsData {
        total_queries: agg.total as u64,
        unique_metrics: agg.uniq as u64,
        avg_per_metric: agg.avg,
        most_popular,
        most_popular_count,
        trend_vs_last_period: trend,
        by_source_type: SourceTypeBreakdownData {
            agent,
            workflow,
            task,
            analytics,
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
    storage: &PostgresObservabilityStorage,
    days: u32,
    limit: usize,
    offset: usize,
) -> Result<MetricsListData, OxyError> {
    let db = storage.db();
    let interval = format!("{days} days");

    let count_sql = format!(
        "SELECT count(DISTINCT metric_name)::BIGINT AS count
        FROM observability_metric_usage
        WHERE created_at >= now() - INTERVAL '{interval}'"
    );

    let total =
        CountRow::find_by_statement(Statement::from_sql_and_values(pg(), &count_sql, vec![]))
            .one(db)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Count query failed: {e}")))?
            .map(|r| r.count)
            .unwrap_or(0);

    let list_sql = format!(
        "SELECT
            metric_name,
            count(*)::BIGINT AS cnt,
            to_char(max(created_at), 'YYYY-MM-DD') AS last_used
        FROM observability_metric_usage
        WHERE created_at >= now() - INTERVAL '{interval}'
        GROUP BY metric_name
        ORDER BY cnt DESC
        LIMIT $1 OFFSET $2"
    );

    let rows = MetricListQueryRow::find_by_statement(Statement::from_sql_and_values(
        pg(),
        &list_sql,
        vec![(limit as i64).into(), (offset as i64).into()],
    ))
    .all(db)
    .await
    .map_err(|e| OxyError::RuntimeError(format!("Metrics list query failed: {e}")))?;

    let metrics = rows
        .into_iter()
        .map(|r| MetricListItem {
            name: r.metric_name,
            count: r.cnt as u64,
            last_used: r.last_used,
        })
        .collect();

    Ok(MetricsListData {
        metrics,
        total: total as u64,
        limit,
        offset,
    })
}

pub(super) async fn get_metric_detail(
    storage: &PostgresObservabilityStorage,
    metric_name: &str,
    days: u32,
) -> Result<MetricDetailData, OxyError> {
    let db = storage.db();
    let interval = format!("{days} days");

    let total_sql = format!(
        "SELECT count(*)::BIGINT AS count
        FROM observability_metric_usage
        WHERE metric_name = $1
          AND created_at >= now() - INTERVAL '{interval}'"
    );

    let total_queries = CountRow::find_by_statement(Statement::from_sql_and_values(
        pg(),
        &total_sql,
        vec![metric_name.into()],
    ))
    .one(db)
    .await
    .map_err(|e| OxyError::RuntimeError(format!("Total query failed: {e}")))?
    .map(|r| r.count)
    .unwrap_or(0);

    let prev_sql = format!(
        "SELECT count(*)::BIGINT AS count
        FROM observability_metric_usage
        WHERE metric_name = $1
          AND created_at >= now() - INTERVAL '{} days'
          AND created_at < now() - INTERVAL '{interval}'",
        days * 2
    );

    let prev_count = CountRow::find_by_statement(Statement::from_sql_and_values(
        pg(),
        &prev_sql,
        vec![metric_name.into()],
    ))
    .one(db)
    .await
    .ok()
    .flatten()
    .map(|r| r.count)
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

    let via_agent_sql = format!(
        "SELECT count(*)::BIGINT AS count
        FROM observability_metric_usage
        WHERE metric_name = $1 AND source_type = 'agent'
          AND created_at >= now() - INTERVAL '{interval}'"
    );
    let via_agent = CountRow::find_by_statement(Statement::from_sql_and_values(
        pg(),
        &via_agent_sql,
        vec![metric_name.into()],
    ))
    .one(db)
    .await
    .ok()
    .flatten()
    .map(|r| r.count)
    .unwrap_or(0);

    let via_workflow_sql = format!(
        "SELECT count(*)::BIGINT AS count
        FROM observability_metric_usage
        WHERE metric_name = $1 AND source_type = 'workflow'
          AND created_at >= now() - INTERVAL '{interval}'"
    );
    let via_workflow = CountRow::find_by_statement(Statement::from_sql_and_values(
        pg(),
        &via_workflow_sql,
        vec![metric_name.into()],
    ))
    .one(db)
    .await
    .ok()
    .flatten()
    .map(|r| r.count)
    .unwrap_or(0);

    let trend_sql = format!(
        "SELECT
            to_char(created_at, 'YYYY-MM-DD') AS date,
            count(*)::BIGINT AS cnt
        FROM observability_metric_usage
        WHERE metric_name = $1
          AND created_at >= now() - INTERVAL '{interval}'
        GROUP BY date
        ORDER BY date ASC"
    );

    let usage_trend = TrendPointRow::find_by_statement(Statement::from_sql_and_values(
        pg(),
        &trend_sql,
        vec![metric_name.into()],
    ))
    .all(db)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|r| UsageTrendPointData {
        date: r.date,
        count: r.cnt as u64,
    })
    .collect();

    let related_sql = format!(
        "SELECT m2.metric_name, count(*)::BIGINT AS co_count
        FROM observability_metric_usage m1
        INNER JOIN observability_metric_usage m2
            ON m1.trace_id = m2.trace_id AND m1.metric_name != m2.metric_name
        WHERE m1.metric_name = $1
          AND m1.created_at >= now() - INTERVAL '{interval}'
        GROUP BY m2.metric_name
        ORDER BY co_count DESC
        LIMIT 10"
    );

    let related_metrics = RelatedMetricRow::find_by_statement(Statement::from_sql_and_values(
        pg(),
        &related_sql,
        vec![metric_name.into()],
    ))
    .all(db)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|r| RelatedMetricData {
        name: r.metric_name,
        co_occurrence_count: r.co_count as u64,
    })
    .collect();

    let recent_sql = format!(
        "SELECT
            source_type,
            source_ref,
            context_types::TEXT AS context_types,
            trace_id,
            to_char(created_at, 'YYYY-MM-DD HH24:MI:SS') AS created_at,
            context
        FROM observability_metric_usage
        WHERE metric_name = $1
          AND created_at >= now() - INTERVAL '{interval}'
        ORDER BY created_at DESC
        LIMIT 20"
    );

    let recent_usage = RecentUsageRow::find_by_statement(Statement::from_sql_and_values(
        pg(),
        &recent_sql,
        vec![metric_name.into()],
    ))
    .all(db)
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
        total_queries: total_queries as u64,
        trend_vs_last_period: trend,
        via_agent: via_agent as u64,
        via_workflow: via_workflow as u64,
        usage_trend,
        related_metrics,
        recent_usage,
    })
}
