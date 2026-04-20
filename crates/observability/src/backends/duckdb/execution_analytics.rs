//! Execution analytics query implementations for DuckDB storage.
//!
//! Provides methods for querying execution-level analytics, including
//! summaries, time series, per-agent stats, and detailed execution lists.
//! These queries use a self-join pattern on the `spans` table, joining
//! tool-call spans with their parent agent spans.

use std::sync::Arc;

use oxy_shared::errors::OxyError;

use super::DuckDBStorage;
use crate::types::{
    AgentExecutionStatsData, ExecutionDetailData, ExecutionListData, ExecutionSummaryData,
    ExecutionTimeBucketData,
};

// ── Constants ──────────────────────────────────────────────────────────────

/// Common WHERE clause for identifying tool-call execution spans.
const EXECUTION_BASE_WHERE: &str = "\
    json_extract_string(tool.span_attributes, '$.\"oxy.span_type\"') = 'tool_call' \
    AND json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') IN \
        ('semantic_query', 'omni_query', 'sql_generated', 'workflow', 'agent_tool') \
    AND json_extract_string(agent.span_attributes, '$.\"oxy.agent.ref\"') != ''";

/// Common FROM + JOIN clause.
const EXECUTION_BASE_FROM: &str = "\
    FROM spans AS tool \
    INNER JOIN spans AS agent \
        ON tool.trace_id = agent.trace_id \
        AND agent.span_name = 'agent.run_agent'";

// ── Queries ────────────────────────────────────────────────────────────────

impl DuckDBStorage {
    /// Get execution analytics summary.
    pub async fn get_execution_summary(&self, days: u32) -> Result<ExecutionSummaryData, OxyError> {
        let conn = Arc::clone(self.conn());

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            let sql = format!(
                "SELECT
                    count(*) as total_executions,
                    count_if(json_extract_string(tool.span_attributes, '$.\"oxy.is_verified\"') = 'true') as verified_count,
                    count_if(json_extract_string(tool.span_attributes, '$.\"oxy.is_verified\"') != 'true') as generated_count,
                    count_if(
                        json_extract_string(tool.span_attributes, '$.\"oxy.is_verified\"') = 'true'
                        AND (
                            (SELECT json_extract_string(ev.value, '$.attributes.error.message')
                             FROM json_each(tool.event_data) ev
                             WHERE json_extract_string(ev.value, '$.name') = 'tool_call.output'
                               AND json_extract_string(ev.value, '$.attributes.status') = 'error'
                             LIMIT 1) IS NULL
                            OR
                            (SELECT json_extract_string(ev.value, '$.attributes.error.message')
                             FROM json_each(tool.event_data) ev
                             WHERE json_extract_string(ev.value, '$.name') = 'tool_call.output'
                               AND json_extract_string(ev.value, '$.attributes.status') = 'error'
                             LIMIT 1) = ''
                        )
                    ) as success_count_verified,
                    count_if(
                        json_extract_string(tool.span_attributes, '$.\"oxy.is_verified\"') != 'true'
                        AND (
                            (SELECT json_extract_string(ev.value, '$.attributes.error.message')
                             FROM json_each(tool.event_data) ev
                             WHERE json_extract_string(ev.value, '$.name') = 'tool_call.output'
                               AND json_extract_string(ev.value, '$.attributes.status') = 'error'
                             LIMIT 1) IS NULL
                            OR
                            (SELECT json_extract_string(ev.value, '$.attributes.error.message')
                             FROM json_each(tool.event_data) ev
                             WHERE json_extract_string(ev.value, '$.name') = 'tool_call.output'
                               AND json_extract_string(ev.value, '$.attributes.status') = 'error'
                             LIMIT 1) = ''
                        )
                    ) as success_count_generated,
                    count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'semantic_query') as semantic_query_count,
                    count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'omni_query') as omni_query_count,
                    count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'sql_generated') as sql_generated_count,
                    count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'workflow') as workflow_count,
                    count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'agent_tool') as agent_tool_count
                {EXECUTION_BASE_FROM}
                WHERE {EXECUTION_BASE_WHERE}
                  AND tool.timestamp >= current_timestamp::TIMESTAMP - INTERVAL '{days} DAY'"
            );

            let row = conn
                .query_row(&sql, [], |row| {
                    Ok(ExecutionSummaryData {
                        total_executions: row.get::<_, Option<i64>>(0)?.unwrap_or(0) as u64,
                        verified_count: row.get::<_, Option<i64>>(1)?.unwrap_or(0) as u64,
                        generated_count: row.get::<_, Option<i64>>(2)?.unwrap_or(0) as u64,
                        success_count_verified: row.get::<_, Option<i64>>(3)?.unwrap_or(0) as u64,
                        success_count_generated: row.get::<_, Option<i64>>(4)?.unwrap_or(0) as u64,
                        semantic_query_count: row.get::<_, Option<i64>>(5)?.unwrap_or(0) as u64,
                        omni_query_count: row.get::<_, Option<i64>>(6)?.unwrap_or(0) as u64,
                        sql_generated_count: row.get::<_, Option<i64>>(7)?.unwrap_or(0) as u64,
                        workflow_count: row.get::<_, Option<i64>>(8)?.unwrap_or(0) as u64,
                        agent_tool_count: row.get::<_, Option<i64>>(9)?.unwrap_or(0) as u64,
                    })
                })
                .map_err(|e| OxyError::RuntimeError(format!("Summary query failed: {e}")))?;

            Ok(row)
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }

    /// Get execution time series (daily buckets).
    pub async fn get_execution_time_series(
        &self,
        days: u32,
    ) -> Result<Vec<ExecutionTimeBucketData>, OxyError> {
        let conn = Arc::clone(self.conn());

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            let sql = format!(
                "SELECT
                    CAST(CAST(tool.timestamp AS TIMESTAMP) AS DATE)::VARCHAR as date,
                    count_if(json_extract_string(tool.span_attributes, '$.\"oxy.is_verified\"') = 'true') as verified_count,
                    count_if(json_extract_string(tool.span_attributes, '$.\"oxy.is_verified\"') != 'true') as generated_count,
                    count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'semantic_query') as semantic_query_count,
                    count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'omni_query') as omni_query_count,
                    count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'sql_generated') as sql_generated_count,
                    count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'workflow') as workflow_count,
                    count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'agent_tool') as agent_tool_count
                {EXECUTION_BASE_FROM}
                WHERE {EXECUTION_BASE_WHERE}
                  AND tool.timestamp >= current_timestamp::TIMESTAMP - INTERVAL '{days} DAY'
                GROUP BY date
                ORDER BY date ASC"
            );

            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| OxyError::RuntimeError(format!("Prepare failed: {e}")))?;

            let rows = stmt
                .query_map([], |row| {
                    Ok(ExecutionTimeBucketData {
                        date: row.get(0)?,
                        verified_count: row.get::<_, i64>(1)? as u64,
                        generated_count: row.get::<_, i64>(2)? as u64,
                        semantic_query_count: row.get::<_, i64>(3)? as u64,
                        omni_query_count: row.get::<_, i64>(4)? as u64,
                        sql_generated_count: row.get::<_, i64>(5)? as u64,
                        workflow_count: row.get::<_, i64>(6)? as u64,
                        agent_tool_count: row.get::<_, i64>(7)? as u64,
                    })
                })
                .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| OxyError::RuntimeError(format!("Row read failed: {e}")))
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }

    /// Get per-agent execution stats.
    pub async fn get_execution_agent_stats(
        &self,
        days: u32,
        limit: usize,
    ) -> Result<Vec<AgentExecutionStatsData>, OxyError> {
        let conn = Arc::clone(self.conn());
        let limit_val = limit as i64;

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            let sql = format!(
                "SELECT
                    json_extract_string(agent.span_attributes, '$.\"oxy.agent.ref\"') as agent_ref,
                    count(*) as total_executions,
                    count_if(json_extract_string(tool.span_attributes, '$.\"oxy.is_verified\"') = 'true') as verified_count,
                    count_if(json_extract_string(tool.span_attributes, '$.\"oxy.is_verified\"') != 'true') as generated_count,
                    count_if(
                        (SELECT json_extract_string(ev.value, '$.attributes.error.message')
                         FROM json_each(tool.event_data) ev
                         WHERE json_extract_string(ev.value, '$.name') = 'tool_call.output'
                           AND json_extract_string(ev.value, '$.attributes.status') = 'error'
                         LIMIT 1) IS NULL
                        OR
                        (SELECT json_extract_string(ev.value, '$.attributes.error.message')
                         FROM json_each(tool.event_data) ev
                         WHERE json_extract_string(ev.value, '$.name') = 'tool_call.output'
                           AND json_extract_string(ev.value, '$.attributes.status') = 'error'
                         LIMIT 1) = ''
                    ) as success_count,
                    count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'semantic_query') as semantic_query_count,
                    count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'omni_query') as omni_query_count,
                    count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'sql_generated') as sql_generated_count,
                    count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'workflow') as workflow_count,
                    count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'agent_tool') as agent_tool_count
                {EXECUTION_BASE_FROM}
                WHERE {EXECUTION_BASE_WHERE}
                  AND tool.timestamp >= current_timestamp::TIMESTAMP - INTERVAL '{days} DAY'
                GROUP BY agent_ref
                ORDER BY total_executions DESC
                LIMIT ?"
            );

            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| OxyError::RuntimeError(format!("Prepare failed: {e}")))?;

            let rows = stmt
                .query_map([&limit_val], |row| {
                    Ok(AgentExecutionStatsData {
                        agent_ref: row.get(0)?,
                        total_executions: row.get::<_, i64>(1)? as u64,
                        verified_count: row.get::<_, i64>(2)? as u64,
                        generated_count: row.get::<_, i64>(3)? as u64,
                        success_count: row.get::<_, i64>(4)? as u64,
                        semantic_query_count: row.get::<_, i64>(5)? as u64,
                        omni_query_count: row.get::<_, i64>(6)? as u64,
                        sql_generated_count: row.get::<_, i64>(7)? as u64,
                        workflow_count: row.get::<_, i64>(8)? as u64,
                        agent_tool_count: row.get::<_, i64>(9)? as u64,
                    })
                })
                .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| OxyError::RuntimeError(format!("Row read failed: {e}")))
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }

    /// Get paginated execution details.
    pub async fn get_execution_list(
        &self,
        days: u32,
        limit: usize,
        offset: usize,
        execution_type: Option<&str>,
        is_verified: Option<bool>,
        source_ref: Option<&str>,
        status: Option<&str>,
    ) -> Result<ExecutionListData, OxyError> {
        let conn = Arc::clone(self.conn());
        let execution_type = execution_type.map(String::from);
        let source_ref = source_ref.map(String::from);
        let status = status.map(String::from);

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            // Build additional filter conditions.
            let mut extra_conditions: Vec<String> = Vec::new();
            let mut params: Vec<Box<dyn duckdb::ToSql>> = Vec::new();

            if let Some(ref et) = execution_type {
                extra_conditions.push(
                    "json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = ?"
                        .to_string(),
                );
                params.push(Box::new(et.clone()));
            }

            if let Some(verified) = is_verified {
                if verified {
                    extra_conditions.push(
                        "json_extract_string(tool.span_attributes, '$.\"oxy.is_verified\"') = 'true'"
                            .to_string(),
                    );
                } else {
                    extra_conditions.push(
                        "json_extract_string(tool.span_attributes, '$.\"oxy.is_verified\"') != 'true'"
                            .to_string(),
                    );
                }
            }

            if let Some(ref sr) = source_ref {
                extra_conditions.push(
                    "json_extract_string(agent.span_attributes, '$.\"oxy.agent.ref\"') = ?".to_string(),
                );
                params.push(Box::new(sr.clone()));
            }

            if let Some(ref st) = status {
                match st.as_str() {
                    "error" => {
                        extra_conditions.push(
                            "(SELECT json_extract_string(ev.value, '$.attributes.error.message')
                              FROM json_each(tool.event_data) ev
                              WHERE json_extract_string(ev.value, '$.name') = 'tool_call.output'
                                AND json_extract_string(ev.value, '$.attributes.status') = 'error'
                              LIMIT 1) IS NOT NULL
                             AND (SELECT json_extract_string(ev.value, '$.attributes.error.message')
                              FROM json_each(tool.event_data) ev
                              WHERE json_extract_string(ev.value, '$.name') = 'tool_call.output'
                                AND json_extract_string(ev.value, '$.attributes.status') = 'error'
                              LIMIT 1) != ''"
                                .to_string(),
                        );
                    }
                    "success" => {
                        extra_conditions.push(
                            "((SELECT json_extract_string(ev.value, '$.attributes.error.message')
                              FROM json_each(tool.event_data) ev
                              WHERE json_extract_string(ev.value, '$.name') = 'tool_call.output'
                                AND json_extract_string(ev.value, '$.attributes.status') = 'error'
                              LIMIT 1) IS NULL
                             OR (SELECT json_extract_string(ev.value, '$.attributes.error.message')
                              FROM json_each(tool.event_data) ev
                              WHERE json_extract_string(ev.value, '$.name') = 'tool_call.output'
                                AND json_extract_string(ev.value, '$.attributes.status') = 'error'
                              LIMIT 1) = '')"
                                .to_string(),
                        );
                    }
                    _ => {}
                }
            }

            let extra_where = if extra_conditions.is_empty() {
                String::new()
            } else {
                format!(" AND {}", extra_conditions.join(" AND "))
            };

            // Count query.
            let count_sql = format!(
                "SELECT count(*)
                {EXECUTION_BASE_FROM}
                WHERE {EXECUTION_BASE_WHERE}
                  AND tool.timestamp >= current_timestamp::TIMESTAMP - INTERVAL '{days} DAY'
                  {extra_where}"
            );

            let param_refs: Vec<&dyn duckdb::ToSql> = params.iter().map(|p| p.as_ref()).collect();

            let total: i64 = conn
                .query_row(&count_sql, param_refs.as_slice(), |row| {
                    row.get::<_, Option<i64>>(0).map(|v| v.unwrap_or(0))
                })
                .map_err(|e| OxyError::RuntimeError(format!("Count query failed: {e}")))?;

            // Data query.
            let limit_val = limit as i64;
            let offset_val = offset as i64;

            let data_sql = format!(
                "SELECT
                    tool.trace_id,
                    tool.span_id,
                    CAST(tool.timestamp AS VARCHAR) as timestamp,
                    COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"'), '') as execution_type,
                    COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.is_verified\"'), 'false') as is_verified,
                    COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.source_type\"'), '') as source_type,
                    COALESCE(json_extract_string(agent.span_attributes, '$.\"oxy.agent.ref\"'), '') as source_ref,
                    CASE WHEN (
                        SELECT json_extract_string(ev.value, '$.attributes.error.message')
                        FROM json_each(tool.event_data) ev
                        WHERE json_extract_string(ev.value, '$.name') = 'tool_call.output'
                          AND json_extract_string(ev.value, '$.attributes.status') = 'error'
                        LIMIT 1
                    ) = '' OR (
                        SELECT json_extract_string(ev.value, '$.attributes.error.message')
                        FROM json_each(tool.event_data) ev
                        WHERE json_extract_string(ev.value, '$.name') = 'tool_call.output'
                          AND json_extract_string(ev.value, '$.attributes.status') = 'error'
                        LIMIT 1
                    ) IS NULL THEN 'success' ELSE 'error' END as status,
                    tool.duration_ns,
                    COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.database\"'), '') as database,
                    COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.topic\"'), '') as topic,
                    COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.semantic_query_params\"'), '') as semantic_query_params,
                    COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.generated_sql\"'), '') as generated_sql,
                    COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.integration\"'), '') as integration,
                    COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.endpoint\"'), '') as endpoint,
                    COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.sql\"'), '') as sql,
                    COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.sql_ref\"'), '') as sql_ref,
                    COALESCE(json_extract_string(agent.span_attributes, '$.\"agent.prompt\"'), '') as user_question,
                    COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.workflow_ref\"'), '') as workflow_ref,
                    COALESCE(json_extract_string(agent.span_attributes, '$.\"oxy.agent.ref\"'), '') as agent_ref,
                    COALESCE((
                        SELECT json_extract_string(ev.value, '$.attributes.input')
                        FROM json_each(tool.event_data) ev
                        WHERE json_extract_string(ev.value, '$.name') = 'tool_call.input'
                        LIMIT 1
                    ), '') as tool_input,
                    COALESCE((
                        SELECT json_extract_string(ev.value, '$.attributes.input')
                        FROM json_each(tool.event_data) ev
                        WHERE json_extract_string(ev.value, '$.name') = 'tool_call.input'
                        LIMIT 1
                    ), '') as input,
                    COALESCE((
                        SELECT json_extract_string(ev.value, '$.attributes.output')
                        FROM json_each(tool.event_data) ev
                        WHERE json_extract_string(ev.value, '$.name') = 'tool_call.output'
                          AND json_extract_string(ev.value, '$.attributes.status') = 'success'
                        LIMIT 1
                    ), '') as output,
                    COALESCE((
                        SELECT json_extract_string(ev.value, '$.attributes.error.message')
                        FROM json_each(tool.event_data) ev
                        WHERE json_extract_string(ev.value, '$.name') = 'tool_call.output'
                          AND json_extract_string(ev.value, '$.attributes.status') = 'error'
                        LIMIT 1
                    ), '') as error
                {EXECUTION_BASE_FROM}
                WHERE {EXECUTION_BASE_WHERE}
                  AND tool.timestamp >= current_timestamp::TIMESTAMP - INTERVAL '{days} DAY'
                  {extra_where}
                ORDER BY tool.timestamp DESC
                LIMIT ? OFFSET ?"
            );

            let mut data_params: Vec<&dyn duckdb::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();
            data_params.push(&limit_val);
            data_params.push(&offset_val);

            let mut stmt = conn
                .prepare(&data_sql)
                .map_err(|e| OxyError::RuntimeError(format!("Prepare failed: {e}")))?;

            let rows = stmt
                .query_map(data_params.as_slice(), |row| {
                    Ok(ExecutionDetailData {
                        trace_id: row.get(0)?,
                        span_id: row.get(1)?,
                        timestamp: row.get(2)?,
                        execution_type: row.get(3)?,
                        is_verified: row.get(4)?,
                        source_type: row.get(5)?,
                        source_ref: row.get(6)?,
                        status: row.get(7)?,
                        duration_ns: row.get(8)?,
                        database: row.get(9)?,
                        topic: row.get(10)?,
                        semantic_query_params: row.get(11)?,
                        generated_sql: row.get(12)?,
                        integration: row.get(13)?,
                        endpoint: row.get(14)?,
                        sql: row.get(15)?,
                        sql_ref: row.get(16)?,
                        user_question: row.get(17)?,
                        workflow_ref: row.get(18)?,
                        agent_ref: row.get(19)?,
                        tool_input: row.get(20)?,
                        input: row.get(21)?,
                        output: row.get(22)?,
                        error: row.get(23)?,
                    })
                })
                .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

            let executions: Vec<ExecutionDetailData> = rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| OxyError::RuntimeError(format!("Row read failed: {e}")))?;

            Ok(ExecutionListData {
                executions,
                total: total as u64,
                limit,
                offset,
            })
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }
}
