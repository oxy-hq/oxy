//! Trace query implementations for DuckDB storage.
//!
//! DuckDB queries against the local `spans`, `intent_classifications`,
//! and `intent_clusters` tables.

use std::sync::Arc;

use oxy_shared::errors::OxyError;

use super::DuckDBStorage;
use crate::types::{
    ClusterInfoRow, ClusterMapDataRow, TraceDetailRow, TraceEnrichmentRow, TraceRow,
};

// ── Queries ────────────────────────────────────────────────────────────────

impl DuckDBStorage {
    /// List traces with pagination and filtering.
    /// Returns (traces, total_count).
    pub async fn list_traces(
        &self,
        limit: i64,
        offset: i64,
        agent_ref: Option<&str>,
        status: Option<&str>,
        duration_filter: Option<&str>,
    ) -> Result<(Vec<TraceRow>, i64), OxyError> {
        let conn = Arc::clone(self.conn());
        let agent_ref = agent_ref.map(String::from);
        let status = status.map(String::from);
        let duration_filter = duration_filter.map(String::from);

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            // Build WHERE clause dynamically.
            let mut conditions = vec![
                "s.span_name IN ('workflow.run_workflow', 'agent.run_agent', 'analytics.run')"
                    .to_string(),
                "s.parent_span_id = ''".to_string(),
            ];
            let mut params: Vec<Box<dyn duckdb::ToSql>> = Vec::new();

            if let Some(ref agent) = agent_ref {
                conditions
                    .push("json_extract_string(s.span_attributes, '$.\"oxy.agent.ref\"') = ?".to_string());
                params.push(Box::new(agent.clone()));
            }

            if let Some(ref st) = status {
                conditions.push("s.status_code = ?".to_string());
                params.push(Box::new(st.clone()));
            }

            if let Some(interval) = crate::duration::duckdb_interval(duration_filter.as_deref()) {
                conditions.push(format!(
                    "s.timestamp >= current_timestamp::TIMESTAMP - INTERVAL '{interval}'"
                ));
            }

            let where_clause = conditions.join(" AND ");

            // Count query.
            let count_sql = format!("SELECT count(*) FROM spans s WHERE {where_clause}");
            let param_refs: Vec<&dyn duckdb::ToSql> = params.iter().map(|p| p.as_ref()).collect();
            let total: i64 = conn
                .query_row(&count_sql, param_refs.as_slice(), |row| row.get(0))
                .map_err(|e| OxyError::RuntimeError(format!("Count query failed: {e}")))?;

            // Data query with token aggregation via subselects on event_data.
            let data_sql = format!(
                "WITH root_traces AS (
                    SELECT trace_id, span_id, timestamp, span_name, service_name,
                           duration_ns, status_code, status_message,
                           span_attributes, event_data
                    FROM spans s
                    WHERE {where_clause}
                    ORDER BY s.timestamp DESC
                    LIMIT ? OFFSET ?
                ),
                token_agg AS (
                    SELECT
                        s2.trace_id,
                        SUM(CAST(json_extract_string(ev.value, '$.attributes.prompt_tokens') AS BIGINT)) AS prompt_tokens,
                        SUM(CAST(json_extract_string(ev.value, '$.attributes.completion_tokens') AS BIGINT)) AS completion_tokens,
                        SUM(CAST(json_extract_string(ev.value, '$.attributes.total_tokens') AS BIGINT)) AS total_tokens
                    FROM spans s2, json_each(s2.event_data) ev
                    WHERE s2.trace_id IN (SELECT trace_id FROM root_traces)
                      AND json_extract_string(ev.value, '$.name') = 'llm.usage'
                    GROUP BY s2.trace_id
                )
                SELECT
                    r.trace_id, r.span_id,
                    CAST(r.timestamp AS VARCHAR) AS timestamp,
                    r.span_name, r.service_name, r.duration_ns,
                    r.status_code, r.status_message,
                    r.span_attributes, r.event_data,
                    COALESCE(t.prompt_tokens, 0) AS prompt_tokens,
                    COALESCE(t.completion_tokens, 0) AS completion_tokens,
                    COALESCE(t.total_tokens, 0) AS total_tokens
                FROM root_traces r
                LEFT JOIN token_agg t ON r.trace_id = t.trace_id
                ORDER BY r.timestamp DESC"
            );

            let mut data_params: Vec<&dyn duckdb::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();
            data_params.push(&limit);
            data_params.push(&offset);

            let mut stmt = conn
                .prepare(&data_sql)
                .map_err(|e| OxyError::RuntimeError(format!("Prepare failed: {e}")))?;

            let rows = stmt
                .query_map(data_params.as_slice(), |row| {
                    Ok(TraceRow {
                        trace_id: row.get(0)?,
                        span_id: row.get(1)?,
                        timestamp: row.get(2)?,
                        span_name: row.get(3)?,
                        service_name: row.get(4)?,
                        duration_ns: row.get(5)?,
                        status_code: row.get(6)?,
                        status_message: row.get(7)?,
                        span_attributes: row.get(8)?,
                        event_data: row.get(9)?,
                        prompt_tokens: row.get(10)?,
                        completion_tokens: row.get(11)?,
                        total_tokens: row.get(12)?,
                    })
                })
                .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

            let traces: Vec<TraceRow> = rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| OxyError::RuntimeError(format!("Row read failed: {e}")))?;

            Ok((traces, total))
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }

    /// Get all spans for a given trace ID.
    pub async fn get_trace_detail(&self, trace_id: &str) -> Result<Vec<TraceDetailRow>, OxyError> {
        let conn = Arc::clone(self.conn());
        let trace_id = trace_id.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            let mut stmt = conn
                .prepare(
                    "SELECT
                        CAST(timestamp AS VARCHAR) AS timestamp,
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
                    FROM spans
                    WHERE trace_id = ?
                    ORDER BY timestamp ASC",
                )
                .map_err(|e| OxyError::RuntimeError(format!("Prepare failed: {e}")))?;

            let rows = stmt
                .query_map([&trace_id], |row| {
                    Ok(TraceDetailRow {
                        timestamp: row.get(0)?,
                        trace_id: row.get(1)?,
                        span_id: row.get(2)?,
                        parent_span_id: row.get(3)?,
                        span_name: row.get(4)?,
                        service_name: row.get(5)?,
                        span_attributes: row.get(6)?,
                        duration_ns: row.get(7)?,
                        status_code: row.get(8)?,
                        status_message: row.get(9)?,
                        event_data: row.get(10)?,
                    })
                })
                .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| OxyError::RuntimeError(format!("Row read failed: {e}")))
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }

    /// Get embeddings with classification data for cluster map visualization.
    pub async fn get_cluster_map_data(
        &self,
        days: u32,
        limit: usize,
        source: Option<&str>,
    ) -> Result<Vec<ClusterMapDataRow>, OxyError> {
        let conn = Arc::clone(self.conn());
        let source = source.map(String::from);

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            let mut conditions = vec![format!(
                "classified_at >= current_timestamp::TIMESTAMP - INTERVAL '{days} DAY'"
            )];

            let mut params: Vec<Box<dyn duckdb::ToSql>> = Vec::new();

            if let Some(ref src) = source {
                conditions.push("source = ?".to_string());
                params.push(Box::new(src.clone()));
            }

            let where_clause = conditions.join(" AND ");
            let limit_val = limit as i64;

            let sql = format!(
                "SELECT
                    trace_id,
                    question,
                    CAST(embedding AS VARCHAR) AS embedding,
                    cluster_id,
                    intent_name,
                    confidence,
                    CAST(classified_at AS VARCHAR) AS classified_at,
                    source
                FROM intent_classifications
                WHERE {where_clause}
                ORDER BY classified_at DESC
                LIMIT ?"
            );

            let mut param_refs: Vec<&dyn duckdb::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();
            param_refs.push(&limit_val);

            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| OxyError::RuntimeError(format!("Prepare failed: {e}")))?;

            let rows = stmt
                .query_map(param_refs.as_slice(), |row| {
                    // DuckDB FLOAT[] can be read as a string and parsed, or directly as Vec<f32>.
                    let embedding_str: String = row.get(2)?;
                    let embedding = parse_float_array(&embedding_str);

                    Ok(ClusterMapDataRow {
                        trace_id: row.get(0)?,
                        question: row.get(1)?,
                        embedding,
                        cluster_id: row.get(3)?,
                        intent_name: row.get(4)?,
                        confidence: row.get(5)?,
                        classified_at: row.get(6)?,
                        source: row.get(7)?,
                    })
                })
                .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| OxyError::RuntimeError(format!("Row read failed: {e}")))
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }

    /// Get cluster info for visualization.
    pub async fn get_cluster_infos(&self) -> Result<Vec<ClusterInfoRow>, OxyError> {
        let conn = Arc::clone(self.conn());

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            let mut stmt = conn
                .prepare(
                    "SELECT cluster_id, intent_name, intent_description, sample_questions
                    FROM intent_clusters
                    ORDER BY cluster_id",
                )
                .map_err(|e| OxyError::RuntimeError(format!("Prepare failed: {e}")))?;

            let rows = stmt
                .query_map([], |row| {
                    Ok(ClusterInfoRow {
                        cluster_id: row.get(0)?,
                        intent_name: row.get(1)?,
                        intent_description: row.get(2)?,
                        sample_questions: row.get(3)?,
                    })
                })
                .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| OxyError::RuntimeError(format!("Row read failed: {e}")))
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }

    /// Get trace enrichment data (status, duration) for a set of trace IDs.
    pub async fn get_trace_enrichments(
        &self,
        trace_ids: &[String],
    ) -> Result<Vec<TraceEnrichmentRow>, OxyError> {
        if trace_ids.is_empty() {
            return Ok(Vec::new());
        }

        let conn = Arc::clone(self.conn());
        let trace_ids = trace_ids.to_vec();

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            let placeholders = trace_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");

            let sql = format!(
                "SELECT trace_id, status_code, duration_ns
                FROM spans
                WHERE parent_span_id = ''
                  AND trace_id IN ({placeholders})"
            );

            let params: Vec<&dyn duckdb::ToSql> = trace_ids
                .iter()
                .map(|id| id as &dyn duckdb::ToSql)
                .collect();

            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| OxyError::RuntimeError(format!("Prepare failed: {e}")))?;

            let rows = stmt
                .query_map(params.as_slice(), |row| {
                    Ok(TraceEnrichmentRow {
                        trace_id: row.get(0)?,
                        status_code: row.get(1)?,
                        duration_ns: row.get(2)?,
                    })
                })
                .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| OxyError::RuntimeError(format!("Row read failed: {e}")))
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Parse a DuckDB FLOAT[] string representation (e.g. "[1.0, 2.0, 3.0]") into a Vec<f32>.
pub(super) fn parse_float_array(s: &str) -> Vec<f32> {
    let trimmed = s.trim().trim_start_matches('[').trim_end_matches(']');
    if trimmed.is_empty() {
        return Vec::new();
    }
    trimmed
        .split(',')
        .filter_map(|v| v.trim().parse::<f32>().ok())
        .collect()
}
