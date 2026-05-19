//! Execution analytics query functions for the Airhouse observability backend.

use oxy_shared::errors::OxyError;
use tokio_postgres::SimpleQueryMessage;

use super::{AirhouseObservabilityStorage, esc, get_i64, get_str, get_u64};
use crate::types::{
    AgentExecutionStatsData, ExecutionDetailData, ExecutionListData, ExecutionSummaryData,
    ExecutionTimeBucketData,
};

// ── Constants ─────────────────────────────────────────────────────────────────

const EXEC_BASE_FROM: &str = "\
    FROM oxy_obs_spans AS tool \
    INNER JOIN oxy_obs_spans AS agent \
        ON tool.trace_id = agent.trace_id \
        AND agent.span_name IN ('agent.run_agent', 'analytics.run')";

const EXEC_BASE_WHERE: &str = "\
    json_extract_string(tool.span_attributes, '$.\"oxy.span_type\"') = 'tool_call' \
    AND json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') IN \
        ('semantic_query', 'omni_query', 'sql_generated', 'workflow', 'agent_tool') \
    AND json_extract_string(agent.span_attributes, '$.\"oxy.agent.ref\"') != ''";

fn rows(messages: &[SimpleQueryMessage]) -> impl Iterator<Item = &tokio_postgres::SimpleQueryRow> {
    messages.iter().filter_map(|m| match m {
        SimpleQueryMessage::Row(r) => Some(r),
        _ => None,
    })
}

// ── Queries ───────────────────────────────────────────────────────────────────

pub async fn get_execution_summary(
    storage: &AirhouseObservabilityStorage,
    days: u32,
) -> Result<ExecutionSummaryData, OxyError> {
    let sql = format!(
        "SELECT
            count(*) AS total_executions,
            count_if(json_extract_string(tool.span_attributes, '$.\"oxy.is_verified\"') = 'true') AS verified_count,
            count_if(json_extract_string(tool.span_attributes, '$.\"oxy.is_verified\"') != 'true') AS generated_count,
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
            ) AS success_count_verified,
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
            ) AS success_count_generated,
            count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'semantic_query') AS semantic_query_count,
            count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'omni_query') AS omni_query_count,
            count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'sql_generated') AS sql_generated_count,
            count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'workflow') AS workflow_count,
            count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'agent_tool') AS agent_tool_count
        {EXEC_BASE_FROM}
        WHERE {EXEC_BASE_WHERE}
          AND tool.timestamp >= current_timestamp::TIMESTAMP - INTERVAL '{days} DAY'"
    );
    let msgs = storage.query(&sql).await?;
    let data = rows(&msgs)
        .next()
        .map(|r| ExecutionSummaryData {
            total_executions: get_u64(r, "total_executions"),
            verified_count: get_u64(r, "verified_count"),
            generated_count: get_u64(r, "generated_count"),
            success_count_verified: get_u64(r, "success_count_verified"),
            success_count_generated: get_u64(r, "success_count_generated"),
            semantic_query_count: get_u64(r, "semantic_query_count"),
            omni_query_count: get_u64(r, "omni_query_count"),
            sql_generated_count: get_u64(r, "sql_generated_count"),
            workflow_count: get_u64(r, "workflow_count"),
            agent_tool_count: get_u64(r, "agent_tool_count"),
        })
        .unwrap_or(ExecutionSummaryData {
            total_executions: 0,
            verified_count: 0,
            generated_count: 0,
            success_count_verified: 0,
            success_count_generated: 0,
            semantic_query_count: 0,
            omni_query_count: 0,
            sql_generated_count: 0,
            workflow_count: 0,
            agent_tool_count: 0,
        });
    Ok(data)
}

pub async fn get_execution_time_series(
    storage: &AirhouseObservabilityStorage,
    days: u32,
) -> Result<Vec<ExecutionTimeBucketData>, OxyError> {
    let sql = format!(
        "SELECT
            CAST(CAST(tool.timestamp AS TIMESTAMP) AS DATE)::VARCHAR AS date,
            count_if(json_extract_string(tool.span_attributes, '$.\"oxy.is_verified\"') = 'true') AS verified_count,
            count_if(json_extract_string(tool.span_attributes, '$.\"oxy.is_verified\"') != 'true') AS generated_count,
            count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'semantic_query') AS semantic_query_count,
            count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'omni_query') AS omni_query_count,
            count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'sql_generated') AS sql_generated_count,
            count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'workflow') AS workflow_count,
            count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'agent_tool') AS agent_tool_count
        {EXEC_BASE_FROM}
        WHERE {EXEC_BASE_WHERE}
          AND tool.timestamp >= current_timestamp::TIMESTAMP - INTERVAL '{days} DAY'
        GROUP BY date
        ORDER BY date ASC"
    );
    let msgs = storage.query(&sql).await?;
    let result = rows(&msgs)
        .map(|r| ExecutionTimeBucketData {
            date: get_str(r, "date"),
            verified_count: get_u64(r, "verified_count"),
            generated_count: get_u64(r, "generated_count"),
            semantic_query_count: get_u64(r, "semantic_query_count"),
            omni_query_count: get_u64(r, "omni_query_count"),
            sql_generated_count: get_u64(r, "sql_generated_count"),
            workflow_count: get_u64(r, "workflow_count"),
            agent_tool_count: get_u64(r, "agent_tool_count"),
        })
        .collect();
    Ok(result)
}

pub async fn get_execution_agent_stats(
    storage: &AirhouseObservabilityStorage,
    days: u32,
    limit: usize,
) -> Result<Vec<AgentExecutionStatsData>, OxyError> {
    let sql = format!(
        "SELECT
            json_extract_string(agent.span_attributes, '$.\"oxy.agent.ref\"') AS agent_ref,
            count(*) AS total_executions,
            count_if(json_extract_string(tool.span_attributes, '$.\"oxy.is_verified\"') = 'true') AS verified_count,
            count_if(json_extract_string(tool.span_attributes, '$.\"oxy.is_verified\"') != 'true') AS generated_count,
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
            ) AS success_count,
            count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'semantic_query') AS semantic_query_count,
            count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'omni_query') AS omni_query_count,
            count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'sql_generated') AS sql_generated_count,
            count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'workflow') AS workflow_count,
            count_if(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = 'agent_tool') AS agent_tool_count
        {EXEC_BASE_FROM}
        WHERE {EXEC_BASE_WHERE}
          AND tool.timestamp >= current_timestamp::TIMESTAMP - INTERVAL '{days} DAY'
        GROUP BY agent_ref
        ORDER BY total_executions DESC
        LIMIT {limit}"
    );
    let msgs = storage.query(&sql).await?;
    let result = rows(&msgs)
        .map(|r| AgentExecutionStatsData {
            agent_ref: get_str(r, "agent_ref"),
            total_executions: get_u64(r, "total_executions"),
            verified_count: get_u64(r, "verified_count"),
            generated_count: get_u64(r, "generated_count"),
            success_count: get_u64(r, "success_count"),
            semantic_query_count: get_u64(r, "semantic_query_count"),
            omni_query_count: get_u64(r, "omni_query_count"),
            sql_generated_count: get_u64(r, "sql_generated_count"),
            workflow_count: get_u64(r, "workflow_count"),
            agent_tool_count: get_u64(r, "agent_tool_count"),
        })
        .collect();
    Ok(result)
}

pub async fn get_execution_list(
    storage: &AirhouseObservabilityStorage,
    days: u32,
    limit: usize,
    offset: usize,
    execution_type: Option<&str>,
    is_verified: Option<bool>,
    source_ref: Option<&str>,
    status: Option<&str>,
) -> Result<ExecutionListData, OxyError> {
    let mut extra: Vec<String> = Vec::new();

    if let Some(et) = execution_type {
        extra.push(format!(
            "json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"') = '{}'",
            esc(et)
        ));
    }
    match is_verified {
        Some(true) => extra.push(
            "json_extract_string(tool.span_attributes, '$.\"oxy.is_verified\"') = 'true'"
                .to_string(),
        ),
        Some(false) => extra.push(
            "json_extract_string(tool.span_attributes, '$.\"oxy.is_verified\"') != 'true'"
                .to_string(),
        ),
        None => {}
    }
    if let Some(sr) = source_ref {
        extra.push(format!(
            "json_extract_string(agent.span_attributes, '$.\"oxy.agent.ref\"') = '{}'",
            esc(sr)
        ));
    }
    if let Some(st) = status {
        match st {
            "error" => extra.push(
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
            ),
            "success" => extra.push(
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
            ),
            _ => {}
        }
    }

    let extra_where = if extra.is_empty() {
        String::new()
    } else {
        format!(" AND {}", extra.join(" AND "))
    };

    let count_sql = format!(
        "SELECT count(*) AS n
        {EXEC_BASE_FROM}
        WHERE {EXEC_BASE_WHERE}
          AND tool.timestamp >= current_timestamp::TIMESTAMP - INTERVAL '{days} DAY'
          {extra_where}"
    );
    let count_msgs = storage.query(&count_sql).await?;
    let total = rows(&count_msgs)
        .next()
        .map(|r| get_u64(r, "n"))
        .unwrap_or(0);

    let data_sql = format!(
        "SELECT
            tool.trace_id,
            tool.span_id,
            CAST(tool.timestamp AS VARCHAR) AS timestamp,
            COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.execution_type\"'), '') AS execution_type,
            COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.is_verified\"'), 'false') AS is_verified,
            COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.source_type\"'), '') AS source_type,
            COALESCE(json_extract_string(agent.span_attributes, '$.\"oxy.agent.ref\"'), '') AS source_ref,
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
            ) IS NULL THEN 'success' ELSE 'error' END AS status,
            tool.duration_ns,
            COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.database\"'), '') AS database,
            COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.topic\"'), '') AS topic,
            COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.semantic_query_params\"'), '') AS semantic_query_params,
            COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.generated_sql\"'), '') AS generated_sql,
            COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.integration\"'), '') AS integration,
            COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.endpoint\"'), '') AS endpoint,
            COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.sql\"'), '') AS sql,
            COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.sql_ref\"'), '') AS sql_ref,
            COALESCE(json_extract_string(agent.span_attributes, '$.\"agent.prompt\"'), '') AS user_question,
            COALESCE(json_extract_string(tool.span_attributes, '$.\"oxy.workflow_ref\"'), '') AS workflow_ref,
            COALESCE(json_extract_string(agent.span_attributes, '$.\"oxy.agent.ref\"'), '') AS agent_ref,
            COALESCE((
                SELECT json_extract_string(ev.value, '$.attributes.input')
                FROM json_each(tool.event_data) ev
                WHERE json_extract_string(ev.value, '$.name') = 'tool_call.input'
                LIMIT 1
            ), '') AS tool_input,
            COALESCE((
                SELECT json_extract_string(ev.value, '$.attributes.input')
                FROM json_each(tool.event_data) ev
                WHERE json_extract_string(ev.value, '$.name') = 'tool_call.input'
                LIMIT 1
            ), '') AS input,
            COALESCE((
                SELECT json_extract_string(ev.value, '$.attributes.output')
                FROM json_each(tool.event_data) ev
                WHERE json_extract_string(ev.value, '$.name') = 'tool_call.output'
                  AND json_extract_string(ev.value, '$.attributes.status') = 'success'
                LIMIT 1
            ), '') AS output,
            COALESCE((
                SELECT json_extract_string(ev.value, '$.attributes.error.message')
                FROM json_each(tool.event_data) ev
                WHERE json_extract_string(ev.value, '$.name') = 'tool_call.output'
                  AND json_extract_string(ev.value, '$.attributes.status') = 'error'
                LIMIT 1
            ), '') AS error
        {EXEC_BASE_FROM}
        WHERE {EXEC_BASE_WHERE}
          AND tool.timestamp >= current_timestamp::TIMESTAMP - INTERVAL '{days} DAY'
          {extra_where}
        ORDER BY tool.timestamp DESC
        LIMIT {limit} OFFSET {offset}"
    );
    let data_msgs = storage.query(&data_sql).await?;
    let executions = rows(&data_msgs)
        .map(|r| ExecutionDetailData {
            trace_id: get_str(r, "trace_id"),
            span_id: get_str(r, "span_id"),
            timestamp: get_str(r, "timestamp"),
            execution_type: get_str(r, "execution_type"),
            is_verified: get_str(r, "is_verified"),
            source_type: get_str(r, "source_type"),
            source_ref: get_str(r, "source_ref"),
            status: get_str(r, "status"),
            duration_ns: get_i64(r, "duration_ns"),
            database: get_str(r, "database"),
            topic: get_str(r, "topic"),
            semantic_query_params: get_str(r, "semantic_query_params"),
            generated_sql: get_str(r, "generated_sql"),
            integration: get_str(r, "integration"),
            endpoint: get_str(r, "endpoint"),
            sql: get_str(r, "sql"),
            sql_ref: get_str(r, "sql_ref"),
            user_question: get_str(r, "user_question"),
            workflow_ref: get_str(r, "workflow_ref"),
            agent_ref: get_str(r, "agent_ref"),
            tool_input: get_str(r, "tool_input"),
            input: get_str(r, "input"),
            output: get_str(r, "output"),
            error: get_str(r, "error"),
        })
        .collect();

    Ok(ExecutionListData { executions, total, limit, offset })
}
