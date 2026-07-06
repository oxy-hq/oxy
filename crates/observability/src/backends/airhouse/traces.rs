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

/// Build the `(count_sql, data_sql)` pair for [`list_traces`]. Pure so the
/// SQL shape is unit-testable.
pub(crate) fn build_list_traces_sql(
    limit: i64,
    offset: i64,
    agent_ref: Option<&str>,
    status: Option<&str>,
    duration_filter: Option<&str>,
) -> (String, String) {
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

    // token_agg scans child spans, and trace_id alone gives DuckLake nothing
    // to prune parquet on — unbounded, this was a full-table scan + json_each
    // over every event payload (incident 2026-07-06). Child spans live inside
    // their root's window; the extra day absorbs any trace shorter than a day
    // regardless of whether timestamps record span start or close.
    let token_agg_bound = crate::duration::duckdb_interval(duration_filter)
        .map(|interval| {
            format!(
                "\n              AND s2.timestamp >= current_timestamp::TIMESTAMP - INTERVAL '{interval}' - INTERVAL '1 DAY'"
            )
        })
        .unwrap_or_default();

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
              AND json_extract_string(ev.value, '$.name') = 'llm.usage'{token_agg_bound}
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
    (count_sql, data_sql)
}

pub async fn list_traces(
    storage: &AirhouseObservabilityStorage,
    limit: i64,
    offset: i64,
    agent_ref: Option<&str>,
    status: Option<&str>,
    duration_filter: Option<&str>,
) -> Result<(Vec<TraceRow>, i64), OxyError> {
    let (count_sql, data_sql) =
        build_list_traces_sql(limit, offset, agent_ref, status, duration_filter);

    let count_msgs = storage.query(&count_sql).await?;
    let total = rows(&count_msgs)
        .next()
        .and_then(|r| r.get("n").and_then(|s| s.parse::<i64>().ok()))
        .unwrap_or(0);

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

#[cfg(test)]
mod tests {
    use super::*;

    // Incident 2026-07-06: the token_agg CTE lateral-joined json_each over
    // every span in the table (filtered only by trace_id, which DuckLake
    // cannot prune on). On S3-backed DuckLake this produced 25s full scans
    // that OOMed the data plane and tripped a DuckDB internal error that
    // poisoned the tenant database. The list query MUST carry the duration
    // window into token_agg.

    #[test]
    fn token_agg_is_time_bounded_when_window_given() {
        let (_count, data) = build_list_traces_sql(10, 0, None, None, Some("30d"));
        assert!(
            data.contains(
                "s2.timestamp >= current_timestamp::TIMESTAMP - INTERVAL '30 DAY' - INTERVAL '1 DAY'"
            ),
            "token_agg must carry the duration window (plus margin) so DuckLake can prune parquet, got:\n{data}"
        );
    }

    #[test]
    fn token_agg_unbounded_without_window() {
        let (_count, data) = build_list_traces_sql(10, 0, None, None, None);
        assert!(
            !data.contains("s2.timestamp >="),
            "no window requested ('all') must not bound token_agg:\n{data}"
        );
    }

    #[test]
    fn root_filter_carries_window_and_paging() {
        let (count, data) = build_list_traces_sql(10, 20, None, None, Some("7d"));
        assert!(count.contains("s.timestamp >= current_timestamp::TIMESTAMP - INTERVAL '7 DAY'"));
        assert!(data.contains("LIMIT 10 OFFSET 20"));
    }

    #[test]
    fn agent_and_status_filters_are_escaped() {
        let (count, _data) = build_list_traces_sql(5, 0, Some("it's"), Some("ERROR' OR 1=1"), None);
        assert!(count.contains("it''s"));
        assert!(count.contains("ERROR'' OR 1=1"));
    }
}
