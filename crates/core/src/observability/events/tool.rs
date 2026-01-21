use tracing::{Level, event};

use crate::execute::ExecutionContext;

pub static NAME: &str = "tool.execute";
pub static TYPE: &str = "tool";
pub static INPUT: &str = "tool.input";
pub static OUTPUT: &str = "tool.output";

// Span names and types for tracing::instrument
pub const TOOL_EXECUTE: &str = "tool.execute";
pub const TOOL_TYPE: &str = "tool";

// Tool call span names and types (for individual tool calls)
pub const TOOL_CALL_EXECUTE: &str = "tool_call.execute";
pub const TOOL_CALL_TYPE: &str = "tool_call";
pub const TOOL_CALL_INPUT: &str = "tool_call.input";
pub const TOOL_CALL_OUTPUT: &str = "tool_call.output";

// Individual tool execution span names
pub const SQL_EXECUTE: &str = "sql.execute";
pub const VALIDATE_SQL_EXECUTE: &str = "validate_sql.execute";
pub const WORKFLOW_EXECUTE: &str = "workflow.execute";
pub const RETRIEVAL_EXECUTE: &str = "retrieval.execute";
pub const OMNI_QUERY_EXECUTE: &str = "omni_query.execute";
pub const VISUALIZE_EXECUTE: &str = "visualize.execute";
pub const CREATE_DATA_APP_EXECUTE: &str = "create_data_app.execute";
pub const TOOL_LAUNCHER_EXECUTE: &str = "tool_launcher.execute";
pub const SEMANTIC_QUERY_EXECUTE: &str = "semantic_query.execute";
pub const AGENT_EXECUTE: &str = "agent.execute";

// Semantic query compile span names and types
pub const SEMANTIC_QUERY_COMPILE: &str = "semantic_query.compile";
pub const SEMANTIC_QUERY_COMPILE_TYPE: &str = "semantic_query";
pub const SEMANTIC_QUERY_COMPILE_INPUT: &str = "semantic_query.compile.input";
pub const SEMANTIC_QUERY_COMPILE_OUTPUT: &str = "semantic_query.compile.output";

/// Log tool execution input
pub fn input<T: serde::Serialize>(input: &T) {
    event!(
        Level::INFO,
        name = INPUT,
        is_visible = true,
        input = %serde_json::to_string(&input).unwrap_or_default()
    );
}

/// Log tool execution output
pub fn output<T: serde::Serialize>(output: &T) {
    event!(
        Level::INFO,
        name = OUTPUT,
        is_visible = true,
        status = "success",
        output = %serde_json::to_string(&output).unwrap_or_default()
    );
}

/// Log individual tool call input
pub fn tool_call_input<T: serde::Serialize>(input: &T) {
    event!(
        Level::INFO,
        name = TOOL_CALL_INPUT,
        is_visible = true,
        input = %serde_json::to_string(&input).unwrap_or_default()
    );
}

pub fn record_semantic_tool_call_metric(
    execution_context: &ExecutionContext,
    topic: &str,
    query: &crate::service::types::SemanticQueryParams,
) {
    execution_context.record_explicit_metrics(&query.measures, &query.dimensions, Some(topic));
}

// Record explicit metrics via ExecutionContext

/// Log whether tool call is verified
pub fn tool_call_is_verified(is_verified: bool) {
    event!(
        Level::INFO,
        name = "tool_call.is_verified",
        is_visible = true,
        is_verified = is_verified
    );
}

/// Log individual tool call output
pub fn tool_call_output<T: serde::Serialize>(output: &T) {
    event!(
        Level::INFO,
        name = TOOL_CALL_OUTPUT,
        is_visible = true,
        status = "success",
        output = %serde_json::to_string(&output).unwrap_or_default()
    );
}

/// Log individual tool call error
pub fn tool_call_error(error: &str) {
    event!(
        Level::ERROR,
        name = TOOL_CALL_OUTPUT,
        is_visible = true,
        status = "error",
        { "error.message" } = %error
    );
}

/// Record SQL execution for tracing and metrics collection
pub fn add_sql(execution_context: &ExecutionContext, sql: &str) {
    // Record SQL in metric context via ExecutionContext
    execution_context.record_sql(sql);

    // Emit tracing event
    event!(
        Level::INFO,
        name = "tool.sql",
        is_visible = true,
        sql = %sql
    );
}

/// Log semantic query compile input and record explicit metrics
pub fn semantic_query_compile_input(
    topic: &str,
    query: &crate::service::types::SemanticQueryParams,
) {
    // Emit tracing event
    event!(
        Level::INFO,
        name = SEMANTIC_QUERY_COMPILE_INPUT,
        is_visible = true,
        topic = %topic,
        query = %serde_json::to_string(query).unwrap_or_default()
    );
}

pub fn semantic_query_compile_output(sql: &str) {
    event!(
        Level::INFO,
        name = SEMANTIC_QUERY_COMPILE_OUTPUT,
        is_visible = true,
        status = "success",
        sql = %sql
    );
}

// Execution analytics - span field values for oxy.execution_type
pub const EXECUTION_TYPE_SEMANTIC_QUERY: &str = "semantic_query";
pub const EXECUTION_TYPE_OMNI_QUERY: &str = "omni_query";
pub const EXECUTION_TYPE_SQL_GENERATED: &str = "sql_generated";
pub const EXECUTION_TYPE_WORKFLOW: &str = "workflow";
pub const EXECUTION_TYPE_AGENT_TOOL: &str = "agent_tool";
