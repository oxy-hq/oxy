//! Trace query functions for the Airhouse observability backend.

use oxy_shared::errors::OxyError;
use tokio_postgres::SimpleQueryMessage;

use super::{AirhouseObservabilityStorage, esc, get_i64, get_str, parse_float_array};
use crate::types::{
    ClusterInfoRow, ClusterMapDataRow, TraceDetailRow, TraceEnrichmentRow, TraceRow,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn rows(messages: &[SimpleQueryMessage]) -> impl Iterator<Item = &tokio_postgres::SimpleQueryRow> {
    messages.iter().filter_map(|m| match m {
        SimpleQueryMessage::Row(r) => Some(r),
        _ => None,
    })
}

// ── Queries ───────────────────────────────────────────────────────────────────

pub async fn list_traces(
    storage: &AirhouseObservabilityStorage,
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

    if let Some(a) = agent_ref {
        conditions.push(format!(
            "json_extract_string(s.span_attributes, '$.\"oxy.agent.ref\"') = '{}'",
            esc(a)
        ));
    }
    if let Some(s) = status {
        conditions.push(format!("s.status_code = '{}'", esc(s)));
    }
    if let Some(interval) = crate::duration::duckdb_interval(duration_filter) {
        conditions.push(format!(
            "s.timestamp >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'"
        ));
    }

    let where_clause = conditions.join(" AND ");

    let count_sql = format!("SELECT count(*) AS n FROM oxy_obs_spans s WHERE {where_clause}");
    let count_msgs = storage.query(&count_sql).await?;
    let total = rows(&count_msgs)
        .next()
        .and_then(|r| r.get("n").and_then(|s| s.parse::<i64>().ok()))
        .unwrap_or(0);

    let data_sql = format!(
        "WITH root_traces AS (
            SELECT trace_id, span_id, timestamp, span_name, service_name,
                   duration_ns, status_code, status_message, span_attributes, event_data
            FROM oxy_obs_spans s
            WHERE {where_clause}
            ORDER BY s.timestamp DESC
            LIMIT {limit} OFFSET {offset}
        ),
        token_agg AS (
            SELECT
                s2.trace_id,
                SUM(CAST(json_extract_string(ev.value, '$.attributes.prompt_tokens') AS BIGINT)) AS prompt_tokens,
                SUM(CAST(json_extract_string(ev.value, '$.attributes.completion_tokens') AS BIGINT)) AS completion_tokens,
                SUM(CAST(json_extract_string(ev.value, '$.attributes.total_tokens') AS BIGINT)) AS total_tokens
            FROM oxy_obs_spans s2, json_each(s2.event_data) ev
            WHERE s2.trace_id IN (SELECT trace_id FROM root_traces)
              AND json_extract_string(ev.value, '$.name') = 'llm.usage'
            GROUP BY s2.trace_id
        )
        SELECT
            r.trace_id, r.span_id,
            CAST(r.timestamp AS VARCHAR) AS timestamp,
            r.span_name, r.service_name, r.duration_ns,
            r.status_code, r.status_message, r.span_attributes, r.event_data,
            COALESCE(t.prompt_tokens, 0) AS prompt_tokens,
            COALESCE(t.completion_tokens, 0) AS completion_tokens,
            COALESCE(t.total_tokens, 0) AS total_tokens
        FROM root_traces r
        LEFT JOIN token_agg t ON r.trace_id = t.trace_id
        ORDER BY r.timestamp DESC"
    );
    let data_msgs = storage.query(&data_sql).await?;
    let traces = rows(&data_msgs)
        .map(|r| TraceRow {
            trace_id: get_str(r, "trace_id"),
            span_id: get_str(r, "span_id"),
            timestamp: get_str(r, "timestamp"),
            span_name: get_str(r, "span_name"),
            service_name: get_str(r, "service_name"),
            duration_ns: get_i64(r, "duration_ns"),
            status_code: get_str(r, "status_code"),
            status_message: get_str(r, "status_message"),
            span_attributes: get_str(r, "span_attributes"),
            event_data: get_str(r, "event_data"),
            prompt_tokens: get_i64(r, "prompt_tokens"),
            completion_tokens: get_i64(r, "completion_tokens"),
            total_tokens: get_i64(r, "total_tokens"),
        })
        .collect();

    Ok((traces, total))
}

pub async fn get_trace_detail(
    storage: &AirhouseObservabilityStorage,
    trace_id: &str,
) -> Result<Vec<TraceDetailRow>, OxyError> {
    let sql = format!(
        "SELECT
            CAST(timestamp AS VARCHAR) AS timestamp,
            trace_id, span_id, parent_span_id, span_name, service_name,
            span_attributes, duration_ns, status_code, status_message, event_data
        FROM oxy_obs_spans
        WHERE trace_id = '{}'
        ORDER BY timestamp ASC",
        esc(trace_id)
    );
    let msgs = storage.query(&sql).await?;
    let rows = rows(&msgs)
        .map(|r| TraceDetailRow {
            timestamp: get_str(r, "timestamp"),
            trace_id: get_str(r, "trace_id"),
            span_id: get_str(r, "span_id"),
            parent_span_id: get_str(r, "parent_span_id"),
            span_name: get_str(r, "span_name"),
            service_name: get_str(r, "service_name"),
            span_attributes: get_str(r, "span_attributes"),
            duration_ns: get_i64(r, "duration_ns"),
            status_code: get_str(r, "status_code"),
            status_message: get_str(r, "status_message"),
            event_data: get_str(r, "event_data"),
        })
        .collect();
    Ok(rows)
}

pub async fn get_cluster_map_data(
    storage: &AirhouseObservabilityStorage,
    days: u32,
    limit: usize,
    source: Option<&str>,
) -> Result<Vec<ClusterMapDataRow>, OxyError> {
    let mut conditions = vec![format!(
        "classified_at >= current_timestamp::TIMESTAMP - INTERVAL '{days} DAY'"
    )];
    if let Some(src) = source {
        conditions.push(format!("source = '{}'", esc(src)));
    }
    let where_clause = conditions.join(" AND ");

    let sql = format!(
        "SELECT
            trace_id, question,
            CAST(embedding AS VARCHAR) AS embedding,
            cluster_id, intent_name, confidence,
            CAST(classified_at AS VARCHAR) AS classified_at,
            source
        FROM oxy_obs_intent_classifications
        WHERE {where_clause}
        ORDER BY classified_at DESC
        LIMIT {limit}"
    );
    let msgs = storage.query(&sql).await?;
    let rows = rows(&msgs)
        .map(|r| ClusterMapDataRow {
            trace_id: get_str(r, "trace_id"),
            question: get_str(r, "question"),
            embedding: parse_float_array(&get_str(r, "embedding")),
            cluster_id: get_i64(r, "cluster_id") as i32,
            intent_name: get_str(r, "intent_name"),
            confidence: r
                .get("confidence")
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(0.0),
            classified_at: get_str(r, "classified_at"),
            source: get_str(r, "source"),
        })
        .collect();
    Ok(rows)
}

pub async fn get_cluster_infos(
    storage: &AirhouseObservabilityStorage,
) -> Result<Vec<ClusterInfoRow>, OxyError> {
    let sql = "SELECT cluster_id, intent_name, intent_description, sample_questions
               FROM oxy_obs_intent_clusters
               ORDER BY cluster_id";
    let msgs = storage.query(sql).await?;
    let rows = rows(&msgs)
        .map(|r| ClusterInfoRow {
            cluster_id: get_i64(r, "cluster_id") as i32,
            intent_name: get_str(r, "intent_name"),
            intent_description: get_str(r, "intent_description"),
            sample_questions: get_str(r, "sample_questions"),
        })
        .collect();
    Ok(rows)
}

pub async fn get_trace_enrichments(
    storage: &AirhouseObservabilityStorage,
    trace_ids: &[String],
) -> Result<Vec<TraceEnrichmentRow>, OxyError> {
    if trace_ids.is_empty() {
        return Ok(Vec::new());
    }
    let list = trace_ids
        .iter()
        .map(|id| format!("'{}'", esc(id)))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT trace_id, status_code, duration_ns
        FROM oxy_obs_spans
        WHERE parent_span_id = ''
          AND trace_id IN ({list})"
    );
    let msgs = storage.query(&sql).await?;
    let rows = rows(&msgs)
        .map(|r| TraceEnrichmentRow {
            trace_id: get_str(r, "trace_id"),
            status_code: get_str(r, "status_code"),
            duration_ns: get_i64(r, "duration_ns"),
        })
        .collect();
    Ok(rows)
}
