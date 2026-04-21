//! Execution analytics queries against ClickHouse.

use clickhouse::Row;
use oxy_shared::errors::OxyError;
use serde::Deserialize;

use super::ClickHouseObservabilityStorage;
use crate::types::{
    AgentExecutionStatsData, ExecutionDetailData, ExecutionListData, ExecutionSummaryData,
    ExecutionTimeBucketData,
};

const CH_EXECUTION_BASE_FROM: &str = "\
    FROM observability_spans AS tool \
    INNER JOIN observability_spans AS agent \
        ON tool.trace_id = agent.trace_id \
        AND agent.span_name IN ('agent.run_agent', 'analytics.run')";

const CH_EXECUTION_BASE_WHERE: &str = "\
    JSONExtractString(tool.span_attributes, 'oxy.span_type') = 'tool_call' \
    AND JSONExtractString(tool.span_attributes, 'oxy.execution_type') IN \
        ('semantic_query', 'omni_query', 'sql_generated', 'workflow', 'agent_tool') \
    AND JSONExtractString(agent.span_attributes, 'oxy.agent.ref') != ''";

fn error_message_expr() -> String {
    // Finds the error.message attribute of the first tool_call.output event
    // whose status is 'error'. Expressed as a subquery so it can be dropped
    // straight into larger SQL.
    "(SELECT JSONExtractString(ev, 'attributes', 'error.message')
      FROM (SELECT arrayJoin(JSONExtractArrayRaw(tool.event_data)) AS ev)
      WHERE JSONExtractString(ev, 'name') = 'tool_call.output'
        AND JSONExtractString(ev, 'attributes', 'status') = 'error'
      LIMIT 1)"
        .to_string()
}

fn input_expr() -> String {
    "(SELECT JSONExtractString(ev, 'attributes', 'input')
      FROM (SELECT arrayJoin(JSONExtractArrayRaw(tool.event_data)) AS ev)
      WHERE JSONExtractString(ev, 'name') = 'tool_call.input'
      LIMIT 1)"
        .to_string()
}

fn output_expr() -> String {
    "(SELECT JSONExtractString(ev, 'attributes', 'output')
      FROM (SELECT arrayJoin(JSONExtractArrayRaw(tool.event_data)) AS ev)
      WHERE JSONExtractString(ev, 'name') = 'tool_call.output'
        AND JSONExtractString(ev, 'attributes', 'status') = 'success'
      LIMIT 1)"
        .to_string()
}

fn escape_sql_literal(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

#[derive(Debug, Deserialize, Row)]
struct ExecutionSummaryDbRow {
    total_executions: u64,
    verified_count: u64,
    generated_count: u64,
    success_count_verified: u64,
    success_count_generated: u64,
    semantic_query_count: u64,
    omni_query_count: u64,
    sql_generated_count: u64,
    workflow_count: u64,
    agent_tool_count: u64,
}

#[derive(Debug, Deserialize, Row)]
struct ExecutionTimeBucketDbRow {
    date: String,
    verified_count: u64,
    generated_count: u64,
    semantic_query_count: u64,
    omni_query_count: u64,
    sql_generated_count: u64,
    workflow_count: u64,
    agent_tool_count: u64,
}

#[derive(Debug, Deserialize, Row)]
struct AgentStatsDbRow {
    agent_ref: String,
    total_executions: u64,
    verified_count: u64,
    generated_count: u64,
    success_count: u64,
    semantic_query_count: u64,
    omni_query_count: u64,
    sql_generated_count: u64,
    workflow_count: u64,
    agent_tool_count: u64,
}

#[derive(Debug, Deserialize, Row)]
struct ExecutionDetailDbRow {
    trace_id: String,
    span_id: String,
    timestamp: String,
    execution_type: String,
    is_verified: String,
    source_type: String,
    source_ref: String,
    status: String,
    duration_ns: i64,
    database: String,
    topic: String,
    semantic_query_params: String,
    generated_sql: String,
    integration: String,
    endpoint: String,
    sql: String,
    sql_ref: String,
    user_question: String,
    workflow_ref: String,
    agent_ref: String,
    tool_input: String,
    input: String,
    output: String,
    error: String,
}

#[derive(Debug, Deserialize, Row)]
struct CountOnly {
    count: u64,
}

pub(super) async fn get_execution_summary(
    storage: &ClickHouseObservabilityStorage,
    days: u32,
) -> Result<ExecutionSummaryData, OxyError> {
    let error_expr = error_message_expr();

    let sql = format!(
        "SELECT
            count() AS total_executions,
            countIf(JSONExtractString(tool.span_attributes, 'oxy.is_verified') = 'true') AS verified_count,
            countIf(JSONExtractString(tool.span_attributes, 'oxy.is_verified') != 'true') AS generated_count,
            countIf(
                JSONExtractString(tool.span_attributes, 'oxy.is_verified') = 'true'
                AND ({error_expr} IS NULL OR {error_expr} = '')
            ) AS success_count_verified,
            countIf(
                JSONExtractString(tool.span_attributes, 'oxy.is_verified') != 'true'
                AND ({error_expr} IS NULL OR {error_expr} = '')
            ) AS success_count_generated,
            countIf(JSONExtractString(tool.span_attributes, 'oxy.execution_type') = 'semantic_query') AS semantic_query_count,
            countIf(JSONExtractString(tool.span_attributes, 'oxy.execution_type') = 'omni_query') AS omni_query_count,
            countIf(JSONExtractString(tool.span_attributes, 'oxy.execution_type') = 'sql_generated') AS sql_generated_count,
            countIf(JSONExtractString(tool.span_attributes, 'oxy.execution_type') = 'workflow') AS workflow_count,
            countIf(JSONExtractString(tool.span_attributes, 'oxy.execution_type') = 'agent_tool') AS agent_tool_count
        {CH_EXECUTION_BASE_FROM}
        WHERE {CH_EXECUTION_BASE_WHERE}
          AND tool.timestamp >= now() - INTERVAL {days} DAY"
    );

    let row = storage
        .client()
        .query(&sql)
        .fetch_optional::<ExecutionSummaryDbRow>()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Execution summary query failed: {e}")))?
        .unwrap_or(ExecutionSummaryDbRow {
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

    Ok(ExecutionSummaryData {
        total_executions: row.total_executions,
        verified_count: row.verified_count,
        generated_count: row.generated_count,
        success_count_verified: row.success_count_verified,
        success_count_generated: row.success_count_generated,
        semantic_query_count: row.semantic_query_count,
        omni_query_count: row.omni_query_count,
        sql_generated_count: row.sql_generated_count,
        workflow_count: row.workflow_count,
        agent_tool_count: row.agent_tool_count,
    })
}

pub(super) async fn get_execution_time_series(
    storage: &ClickHouseObservabilityStorage,
    days: u32,
) -> Result<Vec<ExecutionTimeBucketData>, OxyError> {
    let sql = format!(
        "SELECT
            formatDateTime(toDate(tool.timestamp), '%Y-%m-%d') AS date,
            countIf(JSONExtractString(tool.span_attributes, 'oxy.is_verified') = 'true') AS verified_count,
            countIf(JSONExtractString(tool.span_attributes, 'oxy.is_verified') != 'true') AS generated_count,
            countIf(JSONExtractString(tool.span_attributes, 'oxy.execution_type') = 'semantic_query') AS semantic_query_count,
            countIf(JSONExtractString(tool.span_attributes, 'oxy.execution_type') = 'omni_query') AS omni_query_count,
            countIf(JSONExtractString(tool.span_attributes, 'oxy.execution_type') = 'sql_generated') AS sql_generated_count,
            countIf(JSONExtractString(tool.span_attributes, 'oxy.execution_type') = 'workflow') AS workflow_count,
            countIf(JSONExtractString(tool.span_attributes, 'oxy.execution_type') = 'agent_tool') AS agent_tool_count
        {CH_EXECUTION_BASE_FROM}
        WHERE {CH_EXECUTION_BASE_WHERE}
          AND tool.timestamp >= now() - INTERVAL {days} DAY
        GROUP BY date
        ORDER BY date ASC"
    );

    let rows: Vec<ExecutionTimeBucketDbRow> = storage
        .client()
        .query(&sql)
        .fetch_all()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Time series query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| ExecutionTimeBucketData {
            date: r.date,
            verified_count: r.verified_count,
            generated_count: r.generated_count,
            semantic_query_count: r.semantic_query_count,
            omni_query_count: r.omni_query_count,
            sql_generated_count: r.sql_generated_count,
            workflow_count: r.workflow_count,
            agent_tool_count: r.agent_tool_count,
        })
        .collect())
}

pub(super) async fn get_execution_agent_stats(
    storage: &ClickHouseObservabilityStorage,
    days: u32,
    limit: usize,
) -> Result<Vec<AgentExecutionStatsData>, OxyError> {
    let error_expr = error_message_expr();

    let sql = format!(
        "SELECT
            JSONExtractString(agent.span_attributes, 'oxy.agent.ref') AS agent_ref,
            count() AS total_executions,
            countIf(JSONExtractString(tool.span_attributes, 'oxy.is_verified') = 'true') AS verified_count,
            countIf(JSONExtractString(tool.span_attributes, 'oxy.is_verified') != 'true') AS generated_count,
            countIf({error_expr} IS NULL OR {error_expr} = '') AS success_count,
            countIf(JSONExtractString(tool.span_attributes, 'oxy.execution_type') = 'semantic_query') AS semantic_query_count,
            countIf(JSONExtractString(tool.span_attributes, 'oxy.execution_type') = 'omni_query') AS omni_query_count,
            countIf(JSONExtractString(tool.span_attributes, 'oxy.execution_type') = 'sql_generated') AS sql_generated_count,
            countIf(JSONExtractString(tool.span_attributes, 'oxy.execution_type') = 'workflow') AS workflow_count,
            countIf(JSONExtractString(tool.span_attributes, 'oxy.execution_type') = 'agent_tool') AS agent_tool_count
        {CH_EXECUTION_BASE_FROM}
        WHERE {CH_EXECUTION_BASE_WHERE}
          AND tool.timestamp >= now() - INTERVAL {days} DAY
        GROUP BY agent_ref
        ORDER BY total_executions DESC
        LIMIT {limit}"
    );

    let rows: Vec<AgentStatsDbRow> = storage
        .client()
        .query(&sql)
        .fetch_all()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Agent stats query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| AgentExecutionStatsData {
            agent_ref: r.agent_ref,
            total_executions: r.total_executions,
            verified_count: r.verified_count,
            generated_count: r.generated_count,
            success_count: r.success_count,
            semantic_query_count: r.semantic_query_count,
            omni_query_count: r.omni_query_count,
            sql_generated_count: r.sql_generated_count,
            workflow_count: r.workflow_count,
            agent_tool_count: r.agent_tool_count,
        })
        .collect())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn get_execution_list(
    storage: &ClickHouseObservabilityStorage,
    days: u32,
    limit: usize,
    offset: usize,
    execution_type: Option<&str>,
    is_verified: Option<bool>,
    source_ref: Option<&str>,
    status: Option<&str>,
) -> Result<ExecutionListData, OxyError> {
    let error_expr = error_message_expr();
    let input_expr = input_expr();
    let output_expr = output_expr();

    let mut extra_conditions: Vec<String> = Vec::new();

    if let Some(et) = execution_type {
        extra_conditions.push(format!(
            "JSONExtractString(tool.span_attributes, 'oxy.execution_type') = '{}'",
            escape_sql_literal(et)
        ));
    }

    if let Some(verified) = is_verified {
        if verified {
            extra_conditions
                .push("JSONExtractString(tool.span_attributes, 'oxy.is_verified') = 'true'".into());
        } else {
            extra_conditions.push(
                "JSONExtractString(tool.span_attributes, 'oxy.is_verified') != 'true'".into(),
            );
        }
    }

    if let Some(sr) = source_ref {
        extra_conditions.push(format!(
            "JSONExtractString(agent.span_attributes, 'oxy.agent.ref') = '{}'",
            escape_sql_literal(sr)
        ));
    }

    if let Some(st) = status {
        match st {
            "error" => {
                extra_conditions.push(format!("{error_expr} IS NOT NULL AND {error_expr} != ''"));
            }
            "success" => {
                extra_conditions.push(format!("({error_expr} IS NULL OR {error_expr} = '')"));
            }
            _ => {}
        }
    }

    let extra_where = if extra_conditions.is_empty() {
        String::new()
    } else {
        format!(" AND {}", extra_conditions.join(" AND "))
    };

    let count_sql = format!(
        "SELECT count() AS count
        {CH_EXECUTION_BASE_FROM}
        WHERE {CH_EXECUTION_BASE_WHERE}
          AND tool.timestamp >= now() - INTERVAL {days} DAY
          {extra_where}"
    );

    let total = storage
        .client()
        .query(&count_sql)
        .fetch_one::<CountOnly>()
        .await
        .map(|r| r.count)
        .map_err(|e| OxyError::RuntimeError(format!("Count query failed: {e}")))?;

    let data_sql = format!(
        "SELECT
            tool.trace_id AS trace_id,
            tool.span_id AS span_id,
            formatDateTime(tool.timestamp, '%Y-%m-%d %H:%M:%S.%f') AS timestamp,
            coalesce(JSONExtractString(tool.span_attributes, 'oxy.execution_type'), '') AS execution_type,
            coalesce(JSONExtractString(tool.span_attributes, 'oxy.is_verified'), 'false') AS is_verified,
            coalesce(JSONExtractString(tool.span_attributes, 'oxy.source_type'), '') AS source_type,
            coalesce(JSONExtractString(agent.span_attributes, 'oxy.agent.ref'), '') AS source_ref,
            if({error_expr} = '' OR {error_expr} IS NULL, 'success', 'error') AS status,
            tool.duration_ns AS duration_ns,
            coalesce(JSONExtractString(tool.span_attributes, 'oxy.database'), '') AS database,
            coalesce(JSONExtractString(tool.span_attributes, 'oxy.topic'), '') AS topic,
            coalesce(JSONExtractString(tool.span_attributes, 'oxy.semantic_query_params'), '') AS semantic_query_params,
            coalesce(JSONExtractString(tool.span_attributes, 'oxy.generated_sql'), '') AS generated_sql,
            coalesce(JSONExtractString(tool.span_attributes, 'oxy.integration'), '') AS integration,
            coalesce(JSONExtractString(tool.span_attributes, 'oxy.endpoint'), '') AS endpoint,
            coalesce(JSONExtractString(tool.span_attributes, 'oxy.sql'), '') AS sql,
            coalesce(JSONExtractString(tool.span_attributes, 'oxy.sql_ref'), '') AS sql_ref,
            coalesce(JSONExtractString(agent.span_attributes, 'agent.prompt'), '') AS user_question,
            coalesce(JSONExtractString(tool.span_attributes, 'oxy.workflow_ref'), '') AS workflow_ref,
            coalesce(JSONExtractString(agent.span_attributes, 'oxy.agent.ref'), '') AS agent_ref,
            coalesce({input_expr}, '') AS tool_input,
            coalesce({input_expr}, '') AS input,
            coalesce({output_expr}, '') AS output,
            coalesce({error_expr}, '') AS error
        {CH_EXECUTION_BASE_FROM}
        WHERE {CH_EXECUTION_BASE_WHERE}
          AND tool.timestamp >= now() - INTERVAL {days} DAY
          {extra_where}
        ORDER BY tool.timestamp DESC
        LIMIT {limit} OFFSET {offset}"
    );

    let rows: Vec<ExecutionDetailDbRow> = storage
        .client()
        .query(&data_sql)
        .fetch_all()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Execution list query failed: {e}")))?;

    let executions = rows
        .into_iter()
        .map(|r| ExecutionDetailData {
            trace_id: r.trace_id,
            span_id: r.span_id,
            timestamp: r.timestamp,
            execution_type: r.execution_type,
            is_verified: r.is_verified,
            source_type: r.source_type,
            source_ref: r.source_ref,
            status: r.status,
            duration_ns: r.duration_ns,
            database: r.database,
            topic: r.topic,
            semantic_query_params: r.semantic_query_params,
            generated_sql: r.generated_sql,
            integration: r.integration,
            endpoint: r.endpoint,
            sql: r.sql,
            sql_ref: r.sql_ref,
            user_question: r.user_question,
            workflow_ref: r.workflow_ref,
            agent_ref: r.agent_ref,
            tool_input: r.tool_input,
            input: r.input,
            output: r.output,
            error: r.error,
        })
        .collect();

    Ok(ExecutionListData {
        executions,
        total,
        limit,
        offset,
    })
}
