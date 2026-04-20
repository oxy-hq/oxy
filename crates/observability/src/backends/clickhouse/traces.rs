//! Trace queries against ClickHouse observability tables.

use clickhouse::Row;
use oxy_shared::errors::OxyError;
use serde::Deserialize;

use super::ClickHouseObservabilityStorage;
use crate::types::{
    ClusterInfoRow, ClusterMapDataRow, SpanRecord, TraceDetailRow, TraceEnrichmentRow, TraceRow,
};

#[derive(Debug, Deserialize, Row)]
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

#[derive(Debug, Deserialize, Row)]
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

#[derive(Debug, Deserialize, Row)]
struct ClusterMapQueryRow {
    trace_id: String,
    question: String,
    embedding: Vec<f32>,
    cluster_id: i32,
    intent_name: String,
    confidence: f32,
    classified_at: String,
    source: String,
}

#[derive(Debug, Deserialize, Row)]
struct ClusterInfoQueryRow {
    cluster_id: i32,
    intent_name: String,
    intent_description: String,
    sample_questions: String,
}

#[derive(Debug, Deserialize, Row)]
struct TraceEnrichmentQueryRow {
    trace_id: String,
    status_code: String,
    duration_ns: i64,
}

#[derive(Debug, Deserialize, Row)]
struct CountOnly {
    count: u64,
}

/// ClickHouse row mirror for inserts into `observability_spans`.
#[derive(Debug, serde::Serialize, Row)]
struct SpanInsertRow {
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
    /// Unix nanoseconds (DateTime64(9) stored as Int64 on the wire).
    timestamp: i64,
}

fn duration_interval(dur: Option<&str>) -> Option<&'static str> {
    crate::duration::clickhouse_interval(dur)
}

/// Escape a string for inclusion as a ClickHouse SQL string literal.
///
/// Uses ANSI-style single-quote doubling (`'` → `''`). This is the only
/// escape ClickHouse accepts unconditionally — backslash escapes depend on
/// the `allow_backslash_escaping_in_strings` setting, which defaults to `off`
/// in ClickHouse ≥ 22.4 and would silently produce malformed literals.
fn escape_sql_literal(s: &str) -> String {
    s.replace('\'', "''")
}

pub(super) async fn list_traces(
    storage: &ClickHouseObservabilityStorage,
    limit: i64,
    offset: i64,
    agent_ref: Option<&str>,
    status: Option<&str>,
    duration_filter: Option<&str>,
) -> Result<(Vec<TraceRow>, i64), OxyError> {
    let mut conditions = vec![
        "s.span_name IN ('workflow.run_workflow', 'agent.run_agent', 'analytics.run')".to_string(),
        "s.parent_span_id = ''".to_string(),
    ];

    if let Some(agent) = agent_ref {
        conditions.push(format!(
            "JSONExtractString(s.span_attributes, 'oxy.agent.ref') = '{}'",
            escape_sql_literal(agent)
        ));
    }

    if let Some(st) = status {
        conditions.push(format!("s.status_code = '{}'", escape_sql_literal(st)));
    }

    if let Some(interval) = duration_interval(duration_filter) {
        conditions.push(format!("s.timestamp >= now() - {interval}"));
    }

    let where_clause = conditions.join(" AND ");

    let count_sql =
        format!("SELECT count() AS count FROM observability_spans s WHERE {where_clause}");
    let total: u64 = storage
        .client()
        .query(&count_sql)
        .fetch_one::<CountOnly>()
        .await
        .map(|r| r.count)
        .map_err(|e| OxyError::RuntimeError(format!("Count query failed: {e}")))?;

    let data_sql = format!(
        "WITH root_traces AS (
            SELECT trace_id, span_id, timestamp, span_name, service_name,
                   duration_ns, status_code, status_message,
                   span_attributes, event_data
            FROM observability_spans s
            WHERE {where_clause}
            ORDER BY s.timestamp DESC
            LIMIT {limit} OFFSET {offset}
        ),
        token_agg AS (
            SELECT
                s2.trace_id,
                sum(toInt64OrZero(JSONExtractString(ev, 'attributes', 'prompt_tokens'))) AS prompt_tokens,
                sum(toInt64OrZero(JSONExtractString(ev, 'attributes', 'completion_tokens'))) AS completion_tokens,
                sum(toInt64OrZero(JSONExtractString(ev, 'attributes', 'total_tokens'))) AS total_tokens
            FROM observability_spans AS s2
            ARRAY JOIN JSONExtractArrayRaw(s2.event_data) AS ev
            WHERE s2.trace_id IN (SELECT trace_id FROM root_traces)
              AND JSONExtractString(ev, 'name') = 'llm.usage'
            GROUP BY s2.trace_id
        )
        SELECT
            r.trace_id AS trace_id,
            r.span_id AS span_id,
            formatDateTime(r.timestamp, '%Y-%m-%d %H:%M:%S.%f') AS timestamp,
            r.span_name AS span_name,
            r.service_name AS service_name,
            r.duration_ns AS duration_ns,
            r.status_code AS status_code,
            r.status_message AS status_message,
            r.span_attributes AS span_attributes,
            r.event_data AS event_data,
            coalesce(t.prompt_tokens, 0) AS prompt_tokens,
            coalesce(t.completion_tokens, 0) AS completion_tokens,
            coalesce(t.total_tokens, 0) AS total_tokens
        FROM root_traces r
        LEFT JOIN token_agg t ON r.trace_id = t.trace_id
        ORDER BY r.timestamp DESC"
    );

    let rows: Vec<TraceQueryRow> = storage
        .client()
        .query(&data_sql)
        .fetch_all()
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

    Ok((traces, total as i64))
}

pub(super) async fn get_trace_detail(
    storage: &ClickHouseObservabilityStorage,
    trace_id: &str,
) -> Result<Vec<TraceDetailRow>, OxyError> {
    let sql = format!(
        "SELECT
            formatDateTime(timestamp, '%Y-%m-%d %H:%M:%S.%f') AS timestamp,
            trace_id,
            span_id,
            parent_span_id,
            span_name,
            service_name,
            span_attributes,
            duration_ns,
            status_code,
            status_message,
            event_data
        FROM observability_spans
        WHERE trace_id = '{}'
        ORDER BY timestamp ASC",
        escape_sql_literal(trace_id)
    );

    let rows: Vec<TraceDetailQueryRow> = storage
        .client()
        .query(&sql)
        .fetch_all()
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
    storage: &ClickHouseObservabilityStorage,
    days: u32,
    limit: usize,
    source: Option<&str>,
) -> Result<Vec<ClusterMapDataRow>, OxyError> {
    let mut conditions = vec![format!("classified_at >= now() - INTERVAL {days} DAY")];
    if let Some(src) = source {
        conditions.push(format!("source = '{}'", escape_sql_literal(src)));
    }

    let where_clause = conditions.join(" AND ");

    let sql = format!(
        "SELECT
            trace_id,
            question,
            embedding,
            cluster_id,
            intent_name,
            confidence,
            formatDateTime(classified_at, '%Y-%m-%d %H:%M:%S.%f') AS classified_at,
            source
        FROM observability_intent_classifications FINAL
        WHERE {where_clause}
        ORDER BY classified_at DESC
        LIMIT {limit}"
    );

    let rows: Vec<ClusterMapQueryRow> = storage
        .client()
        .query(&sql)
        .fetch_all()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Cluster map query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| ClusterMapDataRow {
            trace_id: r.trace_id,
            question: r.question,
            embedding: r.embedding,
            cluster_id: r.cluster_id,
            intent_name: r.intent_name,
            confidence: r.confidence,
            classified_at: r.classified_at,
            source: r.source,
        })
        .collect())
}

pub(super) async fn get_cluster_infos(
    storage: &ClickHouseObservabilityStorage,
) -> Result<Vec<ClusterInfoRow>, OxyError> {
    let sql = "SELECT cluster_id, intent_name, intent_description, sample_questions
        FROM observability_intent_clusters FINAL
        ORDER BY cluster_id";

    let rows: Vec<ClusterInfoQueryRow> = storage
        .client()
        .query(sql)
        .fetch_all()
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
    storage: &ClickHouseObservabilityStorage,
    trace_ids: &[String],
) -> Result<Vec<TraceEnrichmentRow>, OxyError> {
    if trace_ids.is_empty() {
        return Ok(Vec::new());
    }

    let list = trace_ids
        .iter()
        .map(|id| format!("'{}'", escape_sql_literal(id)))
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "SELECT trace_id, status_code, duration_ns
        FROM observability_spans
        WHERE parent_span_id = ''
          AND trace_id IN ({list})"
    );

    let rows: Vec<TraceEnrichmentQueryRow> = storage
        .client()
        .query(&sql)
        .fetch_all()
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

pub(super) async fn insert_spans(
    storage: &ClickHouseObservabilityStorage,
    spans: Vec<SpanRecord>,
) -> Result<(), OxyError> {
    if spans.is_empty() {
        return Ok(());
    }

    let mut insert = storage
        .client()
        .insert::<SpanInsertRow>("observability_spans")
        .map_err(|e| OxyError::RuntimeError(format!("ClickHouse insert init failed: {e}")))?;

    for span in spans {
        let ts_ns = parse_timestamp_ns(&span.timestamp);
        let row = SpanInsertRow {
            trace_id: span.trace_id,
            span_id: span.span_id,
            parent_span_id: span.parent_span_id,
            span_name: span.span_name,
            service_name: span.service_name,
            span_attributes: span.span_attributes,
            duration_ns: span.duration_ns,
            status_code: span.status_code,
            status_message: span.status_message,
            event_data: span.event_data,
            timestamp: ts_ns,
        };

        insert
            .write(&row)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("ClickHouse span write failed: {e}")))?;
    }

    insert
        .end()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("ClickHouse span insert end failed: {e}")))?;

    Ok(())
}

/// Parse an RFC3339 timestamp into nanoseconds since Unix epoch.
/// On parse failure, falls back to the current wall clock.
fn parse_timestamp_ns(ts: &str) -> i64 {
    match chrono::DateTime::parse_from_rfc3339(ts) {
        Ok(dt) => dt.timestamp_nanos_opt().unwrap_or(0),
        Err(_) => chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
    }
}
