//! ClickHouse storage for execution analytics
//!
//! Queries the otel_traces table to extract execution analytics data
//! based on spans with oxy.span_type = 'tool_call' and oxy.execution_type set.

use clickhouse::Row;
use serde::Deserialize;

use crate::storage::clickhouse::ClickHouseStorage;

use super::types::{
    AgentExecutionStats, ExecutionDetail, ExecutionListResponse, ExecutionSummary,
    ExecutionTimeBucket,
};

/// Storage implementation for execution analytics
pub struct ExecutionAnalyticsStorage {
    storage: ClickHouseStorage,
}

impl ExecutionAnalyticsStorage {
    pub fn new(storage: ClickHouseStorage) -> Self {
        Self { storage }
    }

    pub fn from_env() -> Self {
        Self::new(ClickHouseStorage::from_env())
    }

    /// Get summary statistics for execution analytics
    pub async fn get_summary(
        &self,
        days: u32,
    ) -> Result<ExecutionSummary, clickhouse::error::Error> {
        #[derive(Debug, Row, Deserialize)]
        struct SummaryRow {
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

        let query = format!(
            r#"
            SELECT
                count() as total_executions,
                countIf(tool.SpanAttributes['oxy.is_verified'] = 'true') as verified_count,
                countIf(tool.SpanAttributes['oxy.is_verified'] = 'false') as generated_count,
                countIf(
                    arrayFirst(attrs -> attrs['name'] = 'tool_call.output' AND attrs['status'] = 'error', tool.Events.Attributes)['error.message'] = ''
                    AND tool.SpanAttributes['oxy.is_verified'] = 'true'
                ) as success_count_verified,
                countIf(
                    arrayFirst(attrs -> attrs['name'] = 'tool_call.output' AND attrs['status'] = 'error', tool.Events.Attributes)['error.message'] = ''
                    AND tool.SpanAttributes['oxy.is_verified'] = 'false'
                ) as success_count_generated,
                countIf(tool.SpanAttributes['oxy.execution_type'] = 'semantic_query') as semantic_query_count,
                countIf(tool.SpanAttributes['oxy.execution_type'] = 'omni_query') as omni_query_count,
                countIf(tool.SpanAttributes['oxy.execution_type'] = 'sql_generated') as sql_generated_count,
                countIf(tool.SpanAttributes['oxy.execution_type'] = 'workflow') as workflow_count,
                countIf(tool.SpanAttributes['oxy.execution_type'] = 'agent_tool') as agent_tool_count
            FROM otel.otel_traces AS tool
            INNER JOIN otel.otel_traces AS agent
                ON tool.TraceId = agent.TraceId
                AND agent.SpanName = 'agent.run_agent'
            WHERE tool.SpanAttributes['oxy.span_type'] = 'tool_call'
              AND tool.SpanAttributes['oxy.execution_type'] IN ('semantic_query', 'omni_query', 'sql_generated', 'workflow', 'agent_tool')
              AND agent.SpanAttributes['oxy.agent.ref'] != ''
              AND tool.Timestamp >= now() - INTERVAL {} DAY
            "#,
            days
        );

        let row = self
            .storage
            .client()
            .query(&query)
            .fetch_one::<SummaryRow>()
            .await?;

        let total = row.total_executions.max(1) as f64;
        let verified_percent = (row.verified_count as f64 / total) * 100.0;
        let generated_percent = (row.generated_count as f64 / total) * 100.0;

        let success_rate_verified = if row.verified_count > 0 {
            (row.success_count_verified as f64 / row.verified_count as f64) * 100.0
        } else {
            0.0
        };

        let success_rate_generated = if row.generated_count > 0 {
            (row.success_count_generated as f64 / row.generated_count as f64) * 100.0
        } else {
            0.0
        };

        // Determine the most executed type
        let type_counts = [
            (row.semantic_query_count, "semantic_query"),
            (row.omni_query_count, "omni_query"),
            (row.sql_generated_count, "sql_generated"),
            (row.workflow_count, "workflow"),
            (row.agent_tool_count, "agent_tool"),
        ];
        let most_executed_type = type_counts
            .iter()
            .max_by_key(|(count, _)| *count)
            .map(|(_, name)| *name)
            .unwrap_or("none")
            .to_string();

        Ok(ExecutionSummary {
            total_executions: row.total_executions,
            verified_count: row.verified_count,
            generated_count: row.generated_count,
            verified_percent,
            generated_percent,
            success_rate_verified,
            success_rate_generated,
            most_executed_type,
            semantic_query_count: row.semantic_query_count,
            omni_query_count: row.omni_query_count,
            sql_generated_count: row.sql_generated_count,
            workflow_count: row.workflow_count,
            agent_tool_count: row.agent_tool_count,
        })
    }

    /// Get time series data for execution analytics
    pub async fn get_time_series(
        &self,
        days: u32,
    ) -> Result<Vec<ExecutionTimeBucket>, clickhouse::error::Error> {
        #[derive(Debug, Row, Deserialize)]
        struct TimeSeriesRow {
            date: String,
            verified_count: u64,
            generated_count: u64,
            semantic_query_count: u64,
            omni_query_count: u64,
            sql_generated_count: u64,
            workflow_count: u64,
            agent_tool_count: u64,
        }

        let query = format!(
            r#"
            SELECT
                toString(toDate(tool.Timestamp)) as date,
                countIf(tool.SpanAttributes['oxy.is_verified'] = 'true') as verified_count,
                countIf(tool.SpanAttributes['oxy.is_verified'] = 'false') as generated_count,
                countIf(tool.SpanAttributes['oxy.execution_type'] = 'semantic_query') as semantic_query_count,
                countIf(tool.SpanAttributes['oxy.execution_type'] = 'omni_query') as omni_query_count,
                countIf(tool.SpanAttributes['oxy.execution_type'] = 'sql_generated') as sql_generated_count,
                countIf(tool.SpanAttributes['oxy.execution_type'] = 'workflow') as workflow_count,
                countIf(tool.SpanAttributes['oxy.execution_type'] = 'agent_tool') as agent_tool_count
            FROM otel.otel_traces AS tool
            INNER JOIN otel.otel_traces AS agent
                ON tool.TraceId = agent.TraceId
                AND agent.SpanName = 'agent.run_agent'
            WHERE tool.SpanAttributes['oxy.span_type'] = 'tool_call'
              AND tool.SpanAttributes['oxy.execution_type'] IN ('semantic_query', 'omni_query', 'sql_generated', 'workflow', 'agent_tool')
              AND agent.SpanAttributes['oxy.agent.ref'] != ''
              AND tool.Timestamp >= now() - INTERVAL {} DAY
            GROUP BY date
            ORDER BY date ASC
            "#,
            days
        );

        let rows = self
            .storage
            .client()
            .query(&query)
            .fetch_all::<TimeSeriesRow>()
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| ExecutionTimeBucket {
                timestamp: row.date,
                verified_count: row.verified_count,
                generated_count: row.generated_count,
                semantic_query_count: Some(row.semantic_query_count),
                omni_query_count: Some(row.omni_query_count),
                sql_generated_count: Some(row.sql_generated_count),
                workflow_count: Some(row.workflow_count),
                agent_tool_count: Some(row.agent_tool_count),
            })
            .collect())
    }

    /// Get per-agent execution statistics
    pub async fn get_agent_stats(
        &self,
        days: u32,
        limit: usize,
    ) -> Result<Vec<AgentExecutionStats>, clickhouse::error::Error> {
        #[derive(Debug, Row, Deserialize)]
        struct AgentStatsRow {
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

        let query = format!(
            r#"
            SELECT
                agent.SpanAttributes['oxy.agent.ref'] as agent_ref,
                count() as total_executions,
                countIf(tool.SpanAttributes['oxy.is_verified'] = 'true') as verified_count,
                countIf(tool.SpanAttributes['oxy.is_verified'] = 'false') as generated_count,
                countIf(
                    arrayFirst(attrs -> attrs['name'] = 'tool_call.output' AND attrs['status'] = 'error', tool.Events.Attributes)['error.message'] = ''
                ) as success_count,
                countIf(tool.SpanAttributes['oxy.execution_type'] = 'semantic_query') as semantic_query_count,
                countIf(tool.SpanAttributes['oxy.execution_type'] = 'omni_query') as omni_query_count,
                countIf(tool.SpanAttributes['oxy.execution_type'] = 'sql_generated') as sql_generated_count,
                countIf(tool.SpanAttributes['oxy.execution_type'] = 'workflow') as workflow_count,
                countIf(tool.SpanAttributes['oxy.execution_type'] = 'agent_tool') as agent_tool_count
            FROM otel.otel_traces AS tool
            INNER JOIN otel.otel_traces AS agent
                ON tool.TraceId = agent.TraceId
                AND agent.SpanName = 'agent.run_agent'
            WHERE tool.SpanAttributes['oxy.span_type'] = 'tool_call'
              AND tool.SpanAttributes['oxy.execution_type'] IN ('semantic_query', 'omni_query', 'sql_generated', 'workflow', 'agent_tool')
              AND agent.SpanAttributes['oxy.agent.ref'] != ''
              AND tool.Timestamp >= now() - INTERVAL {} DAY
            GROUP BY agent_ref
            ORDER BY total_executions DESC
            LIMIT {}
            "#,
            days, limit
        );

        let rows = self
            .storage
            .client()
            .query(&query)
            .fetch_all::<AgentStatsRow>()
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let total = row.total_executions.max(1) as f64;
                let type_counts = [
                    (row.semantic_query_count, "semantic_query"),
                    (row.omni_query_count, "omni_query"),
                    (row.sql_generated_count, "sql_generated"),
                    (row.workflow_count, "workflow"),
                    (row.agent_tool_count, "agent_tool"),
                ];
                let most_executed_type = type_counts
                    .iter()
                    .max_by_key(|(count, _)| *count)
                    .map(|(_, name)| *name)
                    .unwrap_or("none")
                    .to_string();
                AgentExecutionStats {
                    agent_ref: row.agent_ref,
                    total_executions: row.total_executions,
                    verified_count: row.verified_count,
                    generated_count: row.generated_count,
                    verified_percent: (row.verified_count as f64 / total) * 100.0,
                    most_executed_type,
                    success_rate: (row.success_count as f64 / total) * 100.0,
                }
            })
            .collect())
    }

    /// Get paginated execution details
    pub async fn get_executions(
        &self,
        days: u32,
        limit: usize,
        offset: usize,
        execution_type: Option<&str>,
        is_verified: Option<bool>,
        source_ref: Option<&str>,
        status: Option<&str>,
    ) -> Result<ExecutionListResponse, clickhouse::error::Error> {
        #[derive(Debug, Row, Deserialize)]
        struct ExecutionRow {
            trace_id: String,
            span_id: String,
            timestamp: String,
            execution_type: String,
            is_verified: String,
            source_type: String,
            source_ref: String,
            status: String,
            duration_ns: i64,
            status_message: String,
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

        // Build WHERE conditions
        let mut conditions = vec![
            "tool.SpanAttributes['oxy.span_type'] = 'tool_call'".to_string(),
            "tool.SpanAttributes['oxy.execution_type'] IN ('semantic_query', 'omni_query', 'sql_generated', 'workflow', 'agent_tool')".to_string(),
            "agent.SpanAttributes['oxy.agent.ref'] != ''".to_string(),
            format!("tool.Timestamp >= now() - INTERVAL {} DAY", days),
        ];

        if let Some(et) = execution_type {
            conditions.push(format!(
                "tool.SpanAttributes['oxy.execution_type'] = '{}'",
                et.replace('\'', "''")
            ));
        }

        if let Some(verified) = is_verified {
            conditions.push(format!(
                "tool.SpanAttributes['oxy.is_verified'] = '{}'",
                if verified { "true" } else { "false" }
            ));
        }

        if let Some(ref source) = source_ref {
            conditions.push(format!(
                "agent.SpanAttributes['oxy.agent.ref'] = '{}'",
                source.replace('\'', "''")
            ));
        }

        if let Some(status_filter) = status {
            if status_filter == "success" {
                conditions.push(
                    "arrayFirst(attrs -> attrs['name'] = 'tool_call.output' AND attrs['status'] = 'error', tool.Events.Attributes)['error.message'] = ''".to_string()
                );
            } else if status_filter == "error" {
                conditions.push(
                    "arrayFirst(attrs -> attrs['name'] = 'tool_call.output' AND attrs['status'] = 'error', tool.Events.Attributes)['error.message'] != ''".to_string()
                );
            }
        }

        let where_clause = conditions.join(" AND ");

        // Count query
        let count_query = format!(
            r#"SELECT count() as cnt FROM otel.otel_traces AS tool
            INNER JOIN otel.otel_traces AS agent
                ON tool.TraceId = agent.TraceId
                AND agent.SpanName = 'agent.run_agent'
            WHERE {}"#,
            where_clause
        );

        #[derive(Debug, Row, Deserialize)]
        struct CountRow {
            cnt: u64,
        }

        let count_result = self
            .storage
            .client()
            .query(&count_query)
            .fetch_one::<CountRow>()
            .await?;

        // Data query - extract output/error from events, join with agent trace for source_ref
        let data_query = format!(
            r#"
            SELECT
                tool.TraceId as trace_id,
                tool.SpanId as span_id,
                toString(tool.Timestamp) as timestamp,
                tool.SpanAttributes['oxy.execution_type'] as execution_type,
                tool.SpanAttributes['oxy.is_verified'] as is_verified,
                'agent' as source_type,
                agent.SpanAttributes['oxy.agent.ref'] as source_ref,
                if(
                    arrayFirst(attrs -> attrs['name'] = 'tool_call.output' AND attrs['status'] = 'error', tool.Events.Attributes)['error.message'] = '',
                    'success',
                    'error'
                ) as status,
                tool.Duration as duration_ns,
                tool.StatusMessage as status_message,
                tool.SpanAttributes['oxy.database'] as database,
                tool.SpanAttributes['oxy.topic'] as topic,
                tool.SpanAttributes['oxy.semantic_query_params'] as semantic_query_params,
                tool.SpanAttributes['oxy.generated_sql'] as generated_sql,
                tool.SpanAttributes['oxy.integration'] as integration,
                tool.SpanAttributes['oxy.endpoint'] as endpoint,
                tool.SpanAttributes['oxy.sql'] as sql,
                tool.SpanAttributes['oxy.sql_ref'] as sql_ref,
                tool.SpanAttributes['oxy.user_question'] as user_question,
                tool.SpanAttributes['oxy.workflow_ref'] as workflow_ref,
                tool.SpanAttributes['oxy.agent_ref'] as agent_ref,
                tool.SpanAttributes['oxy.tool_input'] as tool_input,
                arrayFirst(
                    attrs -> attrs['name'] = 'tool_call.input',
                    tool.Events.Attributes
                )['input'] as input,
                arrayFirst(
                    attrs -> attrs['name'] = 'tool_call.output' AND attrs['status'] = 'success',
                    tool.Events.Attributes
                )['output'] as output,
                arrayFirst(
                    attrs -> attrs['name'] = 'tool_call.output' AND attrs['status'] = 'error',
                    tool.Events.Attributes
                )['error.message'] as error
            FROM otel.otel_traces AS tool
            INNER JOIN otel.otel_traces AS agent
                ON tool.TraceId = agent.TraceId
                AND agent.SpanName = 'agent.run_agent'
            WHERE {}
            ORDER BY tool.Timestamp DESC
            LIMIT {} OFFSET {}
            "#,
            where_clause, limit, offset
        );

        let rows = self
            .storage
            .client()
            .query(&data_query)
            .fetch_all::<ExecutionRow>()
            .await?;

        let executions = rows
            .into_iter()
            .map(|row| {
                // Use error from events, fallback to status_message
                let error = non_empty(row.error).or_else(|| non_empty(row.status_message.clone()));

                ExecutionDetail {
                    trace_id: row.trace_id,
                    span_id: row.span_id,
                    timestamp: row.timestamp,
                    execution_type: row.execution_type,
                    is_verified: row.is_verified == "true",
                    source_type: row.source_type,
                    source_ref: row.source_ref,
                    status: row.status,
                    duration_ms: row.duration_ns as f64 / 1_000_000.0,
                    database: non_empty(row.database),
                    output: non_empty(row.output),
                    error,
                    topic: non_empty(row.topic),
                    semantic_query_params: non_empty(row.semantic_query_params),
                    generated_sql: non_empty(row.generated_sql),
                    integration: non_empty(row.integration),
                    endpoint: non_empty(row.endpoint),
                    sql: non_empty(row.sql),
                    sql_ref: non_empty(row.sql_ref),
                    user_question: non_empty(row.user_question),
                    workflow_ref: non_empty(row.workflow_ref),
                    agent_ref: non_empty(row.agent_ref),
                    tool_input: non_empty(row.input).or_else(|| non_empty(row.tool_input)),
                }
            })
            .collect();

        Ok(ExecutionListResponse {
            executions,
            total: count_result.cnt,
            limit,
            offset,
        })
    }
}

fn non_empty(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}
