//! Execution analytics queries against Postgres observability tables.

use oxy_shared::errors::OxyError;
use sea_orm::{FromQueryResult, Statement};

use super::{PostgresObservabilityStorage, pg};
use crate::types::{
    AgentExecutionStatsData, ExecutionDetailData, ExecutionListData, ExecutionSummaryData,
    ExecutionTimeBucketData,
};

const PG_EXECUTION_BASE_FROM: &str = "\
    FROM observability_spans AS tool \
    INNER JOIN observability_spans AS agent \
        ON tool.trace_id = agent.trace_id \
        AND agent.span_name = 'agent.run_agent'";

const PG_EXECUTION_BASE_WHERE: &str = "\
    tool.span_attributes->>'oxy.span_type' = 'tool_call' \
    AND tool.span_attributes->>'oxy.execution_type' IN \
        ('semantic_query', 'omni_query', 'sql_generated', 'workflow', 'agent_tool') \
    AND agent.span_attributes->>'oxy.agent.ref' != ''";

fn pg_error_message_expr(table_alias: &str) -> String {
    format!(
        "(SELECT (ev.value->'attributes'->>'error.message')
         FROM jsonb_array_elements({table_alias}.event_data) ev(value)
         WHERE ev.value->>'name' = 'tool_call.output'
           AND ev.value->'attributes'->>'status' = 'error'
         LIMIT 1)",
        table_alias = table_alias,
    )
}

#[derive(Debug, FromQueryResult)]
struct CountRow {
    count: i64,
}

#[derive(Debug, FromQueryResult)]
struct ExecutionSummaryRow {
    total_executions: i64,
    verified_count: i64,
    generated_count: i64,
    success_count_verified: i64,
    success_count_generated: i64,
    semantic_query_count: i64,
    omni_query_count: i64,
    sql_generated_count: i64,
    workflow_count: i64,
    agent_tool_count: i64,
}

#[derive(Debug, FromQueryResult)]
struct ExecutionTimeBucketRow {
    date: String,
    verified_count: i64,
    generated_count: i64,
    semantic_query_count: i64,
    omni_query_count: i64,
    sql_generated_count: i64,
    workflow_count: i64,
    agent_tool_count: i64,
}

#[derive(Debug, FromQueryResult)]
struct AgentStatsRow {
    agent_ref: String,
    total_executions: i64,
    verified_count: i64,
    generated_count: i64,
    success_count: i64,
    semantic_query_count: i64,
    omni_query_count: i64,
    sql_generated_count: i64,
    workflow_count: i64,
    agent_tool_count: i64,
}

#[derive(Debug, FromQueryResult)]
struct ExecutionDetailRow {
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

pub(super) async fn get_execution_summary(
    storage: &PostgresObservabilityStorage,
    days: u32,
) -> Result<ExecutionSummaryData, OxyError> {
    let error_expr = pg_error_message_expr("tool");

    let sql = format!(
        "SELECT
            count(*)::BIGINT AS total_executions,
            count(*) FILTER (WHERE tool.span_attributes->>'oxy.is_verified' = 'true')::BIGINT AS verified_count,
            count(*) FILTER (WHERE tool.span_attributes->>'oxy.is_verified' != 'true')::BIGINT AS generated_count,
            count(*) FILTER (WHERE
                tool.span_attributes->>'oxy.is_verified' = 'true'
                AND ({error_expr} IS NULL OR {error_expr} = '')
            )::BIGINT AS success_count_verified,
            count(*) FILTER (WHERE
                tool.span_attributes->>'oxy.is_verified' != 'true'
                AND ({error_expr} IS NULL OR {error_expr} = '')
            )::BIGINT AS success_count_generated,
            count(*) FILTER (WHERE tool.span_attributes->>'oxy.execution_type' = 'semantic_query')::BIGINT AS semantic_query_count,
            count(*) FILTER (WHERE tool.span_attributes->>'oxy.execution_type' = 'omni_query')::BIGINT AS omni_query_count,
            count(*) FILTER (WHERE tool.span_attributes->>'oxy.execution_type' = 'sql_generated')::BIGINT AS sql_generated_count,
            count(*) FILTER (WHERE tool.span_attributes->>'oxy.execution_type' = 'workflow')::BIGINT AS workflow_count,
            count(*) FILTER (WHERE tool.span_attributes->>'oxy.execution_type' = 'agent_tool')::BIGINT AS agent_tool_count
        {PG_EXECUTION_BASE_FROM}
        WHERE {PG_EXECUTION_BASE_WHERE}
          AND tool.timestamp >= now() - INTERVAL '{days} days'"
    );

    let row =
        ExecutionSummaryRow::find_by_statement(Statement::from_sql_and_values(pg(), &sql, vec![]))
            .one(storage.db())
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Execution summary query failed: {e}")))?
            .unwrap_or(ExecutionSummaryRow {
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
        total_executions: row.total_executions as u64,
        verified_count: row.verified_count as u64,
        generated_count: row.generated_count as u64,
        success_count_verified: row.success_count_verified as u64,
        success_count_generated: row.success_count_generated as u64,
        semantic_query_count: row.semantic_query_count as u64,
        omni_query_count: row.omni_query_count as u64,
        sql_generated_count: row.sql_generated_count as u64,
        workflow_count: row.workflow_count as u64,
        agent_tool_count: row.agent_tool_count as u64,
    })
}

pub(super) async fn get_execution_time_series(
    storage: &PostgresObservabilityStorage,
    days: u32,
) -> Result<Vec<ExecutionTimeBucketData>, OxyError> {
    let sql = format!(
        "SELECT
            to_char(tool.timestamp::DATE, 'YYYY-MM-DD') AS date,
            count(*) FILTER (WHERE tool.span_attributes->>'oxy.is_verified' = 'true')::BIGINT AS verified_count,
            count(*) FILTER (WHERE tool.span_attributes->>'oxy.is_verified' != 'true')::BIGINT AS generated_count,
            count(*) FILTER (WHERE tool.span_attributes->>'oxy.execution_type' = 'semantic_query')::BIGINT AS semantic_query_count,
            count(*) FILTER (WHERE tool.span_attributes->>'oxy.execution_type' = 'omni_query')::BIGINT AS omni_query_count,
            count(*) FILTER (WHERE tool.span_attributes->>'oxy.execution_type' = 'sql_generated')::BIGINT AS sql_generated_count,
            count(*) FILTER (WHERE tool.span_attributes->>'oxy.execution_type' = 'workflow')::BIGINT AS workflow_count,
            count(*) FILTER (WHERE tool.span_attributes->>'oxy.execution_type' = 'agent_tool')::BIGINT AS agent_tool_count
        {PG_EXECUTION_BASE_FROM}
        WHERE {PG_EXECUTION_BASE_WHERE}
          AND tool.timestamp >= now() - INTERVAL '{days} days'
        GROUP BY date
        ORDER BY date ASC"
    );

    let rows = ExecutionTimeBucketRow::find_by_statement(Statement::from_sql_and_values(
        pg(),
        &sql,
        vec![],
    ))
    .all(storage.db())
    .await
    .map_err(|e| OxyError::RuntimeError(format!("Time series query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| ExecutionTimeBucketData {
            date: r.date,
            verified_count: r.verified_count as u64,
            generated_count: r.generated_count as u64,
            semantic_query_count: r.semantic_query_count as u64,
            omni_query_count: r.omni_query_count as u64,
            sql_generated_count: r.sql_generated_count as u64,
            workflow_count: r.workflow_count as u64,
            agent_tool_count: r.agent_tool_count as u64,
        })
        .collect())
}

pub(super) async fn get_execution_agent_stats(
    storage: &PostgresObservabilityStorage,
    days: u32,
    limit: usize,
) -> Result<Vec<AgentExecutionStatsData>, OxyError> {
    let error_expr = pg_error_message_expr("tool");

    let sql = format!(
        "SELECT
            agent.span_attributes->>'oxy.agent.ref' AS agent_ref,
            count(*)::BIGINT AS total_executions,
            count(*) FILTER (WHERE tool.span_attributes->>'oxy.is_verified' = 'true')::BIGINT AS verified_count,
            count(*) FILTER (WHERE tool.span_attributes->>'oxy.is_verified' != 'true')::BIGINT AS generated_count,
            count(*) FILTER (WHERE
                {error_expr} IS NULL OR {error_expr} = ''
            )::BIGINT AS success_count,
            count(*) FILTER (WHERE tool.span_attributes->>'oxy.execution_type' = 'semantic_query')::BIGINT AS semantic_query_count,
            count(*) FILTER (WHERE tool.span_attributes->>'oxy.execution_type' = 'omni_query')::BIGINT AS omni_query_count,
            count(*) FILTER (WHERE tool.span_attributes->>'oxy.execution_type' = 'sql_generated')::BIGINT AS sql_generated_count,
            count(*) FILTER (WHERE tool.span_attributes->>'oxy.execution_type' = 'workflow')::BIGINT AS workflow_count,
            count(*) FILTER (WHERE tool.span_attributes->>'oxy.execution_type' = 'agent_tool')::BIGINT AS agent_tool_count
        {PG_EXECUTION_BASE_FROM}
        WHERE {PG_EXECUTION_BASE_WHERE}
          AND tool.timestamp >= now() - INTERVAL '{days} days'
        GROUP BY agent_ref
        ORDER BY total_executions DESC
        LIMIT $1"
    );

    let rows = AgentStatsRow::find_by_statement(Statement::from_sql_and_values(
        pg(),
        &sql,
        vec![(limit as i64).into()],
    ))
    .all(storage.db())
    .await
    .map_err(|e| OxyError::RuntimeError(format!("Agent stats query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| AgentExecutionStatsData {
            agent_ref: r.agent_ref,
            total_executions: r.total_executions as u64,
            verified_count: r.verified_count as u64,
            generated_count: r.generated_count as u64,
            success_count: r.success_count as u64,
            semantic_query_count: r.semantic_query_count as u64,
            omni_query_count: r.omni_query_count as u64,
            sql_generated_count: r.sql_generated_count as u64,
            workflow_count: r.workflow_count as u64,
            agent_tool_count: r.agent_tool_count as u64,
        })
        .collect())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn get_execution_list(
    storage: &PostgresObservabilityStorage,
    days: u32,
    limit: usize,
    offset: usize,
    execution_type: Option<&str>,
    is_verified: Option<bool>,
    source_ref: Option<&str>,
    status: Option<&str>,
) -> Result<ExecutionListData, OxyError> {
    let error_expr = pg_error_message_expr("tool");

    let mut extra_conditions: Vec<String> = Vec::new();
    let mut params: Vec<sea_orm::Value> = Vec::new();
    let mut param_idx = 1u32;

    if let Some(et) = execution_type {
        extra_conditions.push(format!(
            "tool.span_attributes->>'oxy.execution_type' = ${param_idx}"
        ));
        params.push(et.into());
        param_idx += 1;
    }

    if let Some(verified) = is_verified {
        if verified {
            extra_conditions.push("tool.span_attributes->>'oxy.is_verified' = 'true'".to_string());
        } else {
            extra_conditions.push("tool.span_attributes->>'oxy.is_verified' != 'true'".to_string());
        }
    }

    if let Some(sr) = source_ref {
        extra_conditions.push(format!(
            "agent.span_attributes->>'oxy.agent.ref' = ${param_idx}"
        ));
        params.push(sr.into());
        param_idx += 1;
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
        "SELECT count(*)::BIGINT AS count
        {PG_EXECUTION_BASE_FROM}
        WHERE {PG_EXECUTION_BASE_WHERE}
          AND tool.timestamp >= now() - INTERVAL '{days} days'
          {extra_where}"
    );

    let total = CountRow::find_by_statement(Statement::from_sql_and_values(
        pg(),
        &count_sql,
        params.clone(),
    ))
    .one(storage.db())
    .await
    .map_err(|e| OxyError::RuntimeError(format!("Count query failed: {e}")))?
    .map(|r| r.count)
    .unwrap_or(0);

    let limit_param = format!("${param_idx}");
    let offset_param = format!("${}", param_idx + 1);
    params.push((limit as i64).into());
    params.push((offset as i64).into());

    let input_expr = "(SELECT (ev.value->'attributes'->>'input')
             FROM jsonb_array_elements(tool.event_data) ev(value)
             WHERE ev.value->>'name' = 'tool_call.input'
             LIMIT 1)"
        .to_string();

    let output_expr = "(SELECT (ev.value->'attributes'->>'output')
             FROM jsonb_array_elements(tool.event_data) ev(value)
             WHERE ev.value->>'name' = 'tool_call.output'
               AND ev.value->'attributes'->>'status' = 'success'
             LIMIT 1)"
        .to_string();

    let data_sql = format!(
        "SELECT
            tool.trace_id,
            tool.span_id,
            to_char(tool.timestamp, 'YYYY-MM-DD HH24:MI:SS.US') AS timestamp,
            COALESCE(tool.span_attributes->>'oxy.execution_type', '') AS execution_type,
            COALESCE(tool.span_attributes->>'oxy.is_verified', 'false') AS is_verified,
            COALESCE(tool.span_attributes->>'oxy.source_type', '') AS source_type,
            COALESCE(agent.span_attributes->>'oxy.agent.ref', '') AS source_ref,
            CASE WHEN ({error_expr} = '' OR {error_expr} IS NULL) THEN 'success' ELSE 'error' END AS status,
            tool.duration_ns,
            COALESCE(tool.span_attributes->>'oxy.database', '') AS database,
            COALESCE(tool.span_attributes->>'oxy.topic', '') AS topic,
            COALESCE(tool.span_attributes->>'oxy.semantic_query_params', '') AS semantic_query_params,
            COALESCE(tool.span_attributes->>'oxy.generated_sql', '') AS generated_sql,
            COALESCE(tool.span_attributes->>'oxy.integration', '') AS integration,
            COALESCE(tool.span_attributes->>'oxy.endpoint', '') AS endpoint,
            COALESCE(tool.span_attributes->>'oxy.sql', '') AS sql,
            COALESCE(tool.span_attributes->>'oxy.sql_ref', '') AS sql_ref,
            COALESCE(agent.span_attributes->>'agent.prompt', '') AS user_question,
            COALESCE(tool.span_attributes->>'oxy.workflow_ref', '') AS workflow_ref,
            COALESCE(agent.span_attributes->>'oxy.agent.ref', '') AS agent_ref,
            COALESCE({input_expr}, '') AS tool_input,
            COALESCE({input_expr}, '') AS input,
            COALESCE({output_expr}, '') AS output,
            COALESCE({error_expr}, '') AS error
        {PG_EXECUTION_BASE_FROM}
        WHERE {PG_EXECUTION_BASE_WHERE}
          AND tool.timestamp >= now() - INTERVAL '{days} days'
          {extra_where}
        ORDER BY tool.timestamp DESC
        LIMIT {limit_param} OFFSET {offset_param}"
    );

    let rows = ExecutionDetailRow::find_by_statement(Statement::from_sql_and_values(
        pg(),
        &data_sql,
        params,
    ))
    .all(storage.db())
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
        total: total as u64,
        limit,
        offset,
    })
}
