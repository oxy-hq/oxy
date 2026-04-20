//! Trace-related queries against Postgres observability tables.

use oxy_shared::errors::OxyError;
use sea_orm::{FromQueryResult, Statement};

use super::{PostgresObservabilityStorage, parse_pg_float_array, pg};
use crate::types::{
    ClusterInfoRow, ClusterMapDataRow, TraceDetailRow, TraceEnrichmentRow, TraceRow,
};

#[derive(Debug, FromQueryResult)]
struct CountRow {
    count: i64,
}

#[derive(Debug, FromQueryResult)]
struct TraceQueryRow {
    trace_id: String,
    span_id: String,
    timestamp: String,
    span_name: String,
    service_name: String,
    duration_ns: i64,
    status_code: String,
    status_message: String,
    span_attributes: String,
    event_data: String,
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
}

#[derive(Debug, FromQueryResult)]
struct TraceDetailQueryRow {
    timestamp: String,
    trace_id: String,
    span_id: String,
    parent_span_id: String,
    span_name: String,
    service_name: String,
    span_attributes: String,
    duration_ns: i64,
    status_code: String,
    status_message: String,
    event_data: String,
}

#[derive(Debug, FromQueryResult)]
struct ClusterMapQueryRow {
    trace_id: String,
    question: String,
    embedding: String,
    cluster_id: i32,
    intent_name: String,
    confidence: f32,
    classified_at: String,
    source: String,
}

#[derive(Debug, FromQueryResult)]
struct ClusterInfoQueryRow {
    cluster_id: i32,
    intent_name: String,
    intent_description: String,
    sample_questions: String,
}

#[derive(Debug, FromQueryResult)]
struct TraceEnrichmentQueryRow {
    trace_id: String,
    status_code: String,
    duration_ns: i64,
}

pub(super) async fn list_traces(
    storage: &PostgresObservabilityStorage,
    limit: i64,
    offset: i64,
    agent_ref: Option<&str>,
    status: Option<&str>,
    duration_filter: Option<&str>,
) -> Result<(Vec<TraceRow>, i64), OxyError> {
    let db = storage.db();

    let mut conditions = vec![
        "s.span_name IN ('workflow.run_workflow', 'agent.run_agent', 'analytics.run')".to_string(),
        "s.parent_span_id = ''".to_string(),
    ];
    let mut param_idx = 1u32;
    let mut values: Vec<sea_orm::Value> = Vec::new();

    if let Some(agent) = agent_ref {
        conditions.push(format!(
            "s.span_attributes->>'oxy.agent.ref' = ${param_idx}"
        ));
        values.push(agent.into());
        param_idx += 1;
    }

    if let Some(st) = status {
        conditions.push(format!("s.status_code = ${param_idx}"));
        values.push(st.into());
        param_idx += 1;
    }

    if let Some(interval) = crate::duration::postgres_interval(duration_filter) {
        conditions.push(format!("s.timestamp >= now() - INTERVAL '{interval}'"));
    }

    let where_clause = conditions.join(" AND ");

    let count_sql =
        format!("SELECT count(*)::BIGINT AS count FROM observability_spans s WHERE {where_clause}");
    let count_result = CountRow::find_by_statement(Statement::from_sql_and_values(
        pg(),
        &count_sql,
        values.clone(),
    ))
    .one(db)
    .await
    .map_err(|e| OxyError::RuntimeError(format!("Count query failed: {e}")))?;
    let total = count_result.map(|r| r.count).unwrap_or(0);

    let limit_param = format!("${param_idx}");
    let offset_param = format!("${}", param_idx + 1);
    values.push(limit.into());
    values.push(offset.into());

    let data_sql = format!(
        "WITH root_traces AS (
            SELECT trace_id, span_id, timestamp, span_name, service_name,
                   duration_ns, status_code, status_message,
                   span_attributes, event_data
            FROM observability_spans s
            WHERE {where_clause}
            ORDER BY s.timestamp DESC
            LIMIT {limit_param} OFFSET {offset_param}
        ),
        token_agg AS (
            SELECT
                s2.trace_id,
                SUM((ev.value->'attributes'->>'prompt_tokens')::BIGINT) AS prompt_tokens,
                SUM((ev.value->'attributes'->>'completion_tokens')::BIGINT) AS completion_tokens,
                SUM((ev.value->'attributes'->>'total_tokens')::BIGINT) AS total_tokens
            FROM observability_spans s2,
                 jsonb_array_elements(s2.event_data) ev(value)
            WHERE s2.trace_id IN (SELECT trace_id FROM root_traces)
              AND ev.value->>'name' = 'llm.usage'
            GROUP BY s2.trace_id
        )
        SELECT
            r.trace_id, r.span_id,
            to_char(r.timestamp, 'YYYY-MM-DD HH24:MI:SS.US') AS timestamp,
            r.span_name, r.service_name, r.duration_ns,
            r.status_code, r.status_message,
            r.span_attributes::TEXT AS span_attributes,
            r.event_data::TEXT AS event_data,
            COALESCE(t.prompt_tokens, 0)::BIGINT AS prompt_tokens,
            COALESCE(t.completion_tokens, 0)::BIGINT AS completion_tokens,
            COALESCE(t.total_tokens, 0)::BIGINT AS total_tokens
        FROM root_traces r
        LEFT JOIN token_agg t ON r.trace_id = t.trace_id
        ORDER BY r.timestamp DESC"
    );

    let rows =
        TraceQueryRow::find_by_statement(Statement::from_sql_and_values(pg(), &data_sql, values))
            .all(db)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Traces query failed: {e}")))?;

    let traces = rows
        .into_iter()
        .map(|r| TraceRow {
            trace_id: r.trace_id,
            span_id: r.span_id,
            timestamp: r.timestamp,
            span_name: r.span_name,
            service_name: r.service_name,
            duration_ns: r.duration_ns,
            status_code: r.status_code,
            status_message: r.status_message,
            span_attributes: r.span_attributes,
            event_data: r.event_data,
            prompt_tokens: r.prompt_tokens,
            completion_tokens: r.completion_tokens,
            total_tokens: r.total_tokens,
        })
        .collect();

    Ok((traces, total))
}

pub(super) async fn get_trace_detail(
    storage: &PostgresObservabilityStorage,
    trace_id: &str,
) -> Result<Vec<TraceDetailRow>, OxyError> {
    let sql = "SELECT
        to_char(timestamp, 'YYYY-MM-DD HH24:MI:SS.US') AS timestamp,
        trace_id,
        span_id,
        parent_span_id,
        span_name,
        service_name,
        span_attributes::TEXT AS span_attributes,
        duration_ns,
        status_code,
        status_message,
        event_data::TEXT AS event_data
    FROM observability_spans
    WHERE trace_id = $1
    ORDER BY timestamp ASC";

    let rows = TraceDetailQueryRow::find_by_statement(Statement::from_sql_and_values(
        pg(),
        sql,
        vec![trace_id.into()],
    ))
    .all(storage.db())
    .await
    .map_err(|e| OxyError::RuntimeError(format!("Trace detail query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| TraceDetailRow {
            timestamp: r.timestamp,
            trace_id: r.trace_id,
            span_id: r.span_id,
            parent_span_id: r.parent_span_id,
            span_name: r.span_name,
            service_name: r.service_name,
            span_attributes: r.span_attributes,
            duration_ns: r.duration_ns,
            status_code: r.status_code,
            status_message: r.status_message,
            event_data: r.event_data,
        })
        .collect())
}

pub(super) async fn get_cluster_map_data(
    storage: &PostgresObservabilityStorage,
    days: u32,
    limit: usize,
    source: Option<&str>,
) -> Result<Vec<ClusterMapDataRow>, OxyError> {
    let mut conditions = vec![format!("classified_at >= now() - INTERVAL '{days} days'")];
    let mut values: Vec<sea_orm::Value> = Vec::new();
    let mut param_idx = 1u32;

    if let Some(src) = source {
        conditions.push(format!("source = ${param_idx}"));
        values.push(src.into());
        param_idx += 1;
    }

    let where_clause = conditions.join(" AND ");
    let limit_param = format!("${param_idx}");
    values.push((limit as i64).into());

    let sql = format!(
        "SELECT
            trace_id,
            question,
            embedding::TEXT AS embedding,
            cluster_id,
            intent_name,
            confidence,
            to_char(classified_at, 'YYYY-MM-DD HH24:MI:SS.US') AS classified_at,
            source
        FROM observability_intent_classifications
        WHERE {where_clause}
        ORDER BY classified_at DESC
        LIMIT {limit_param}"
    );

    let rows =
        ClusterMapQueryRow::find_by_statement(Statement::from_sql_and_values(pg(), &sql, values))
            .all(storage.db())
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Cluster map query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| ClusterMapDataRow {
            trace_id: r.trace_id,
            question: r.question,
            embedding: parse_pg_float_array(&r.embedding),
            cluster_id: r.cluster_id,
            intent_name: r.intent_name,
            confidence: r.confidence,
            classified_at: r.classified_at,
            source: r.source,
        })
        .collect())
}

pub(super) async fn get_cluster_infos(
    storage: &PostgresObservabilityStorage,
) -> Result<Vec<ClusterInfoRow>, OxyError> {
    let sql = "SELECT cluster_id, intent_name, intent_description,
        sample_questions::TEXT AS sample_questions
        FROM observability_intent_clusters
        ORDER BY cluster_id";

    let rows =
        ClusterInfoQueryRow::find_by_statement(Statement::from_sql_and_values(pg(), sql, vec![]))
            .all(storage.db())
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Cluster info query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| ClusterInfoRow {
            cluster_id: r.cluster_id,
            intent_name: r.intent_name,
            intent_description: r.intent_description,
            sample_questions: r.sample_questions,
        })
        .collect())
}

pub(super) async fn get_trace_enrichments(
    storage: &PostgresObservabilityStorage,
    trace_ids: &[String],
) -> Result<Vec<TraceEnrichmentRow>, OxyError> {
    if trace_ids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders: Vec<String> = trace_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("${}", i + 1))
        .collect();
    let placeholders_str = placeholders.join(", ");

    let sql = format!(
        "SELECT trace_id, status_code, duration_ns
        FROM observability_spans
        WHERE parent_span_id = ''
          AND trace_id IN ({placeholders_str})"
    );

    let values: Vec<sea_orm::Value> = trace_ids.iter().map(|id| id.as_str().into()).collect();

    let rows = TraceEnrichmentQueryRow::find_by_statement(Statement::from_sql_and_values(
        pg(),
        &sql,
        values,
    ))
    .all(storage.db())
    .await
    .map_err(|e| OxyError::RuntimeError(format!("Trace enrichment query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| TraceEnrichmentRow {
            trace_id: r.trace_id,
            status_code: r.status_code,
            duration_ns: r.duration_ns,
        })
        .collect())
}
