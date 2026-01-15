// MCP Tool Executor
//
// This module provides a trait-based system for executing different tool types
// in a polymorphic way, eliminating the need for large match statements.

use async_trait::async_trait;
use rmcp::model::CallToolResult;
use serde_json::{Map, Value};

use oxy::adapters::project::manager::ProjectManager;

use super::context::ToolExecutionContext;

/// Trait for executing MCP tools.
///
/// This trait provides a unified interface for executing different tool types,
/// allowing polymorphic behavior and eliminating the need for match statements
/// based on tool type.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Executes the tool with the given arguments and context.
    ///
    /// # Arguments
    ///
    /// * `project_manager` - Project manager for accessing configuration and resources
    /// * `tool_name` - Name of the tool (without prefix)
    /// * `arguments` - Tool-specific arguments from the MCP request
    /// * `context` - Execution context containing filters, overrides, and variables
    ///
    /// # Returns
    ///
    /// * `Ok(CallToolResult)` - Successful tool execution result
    /// * `Err(rmcp::ErrorData)` - Tool execution error
    async fn execute(
        &self,
        project_manager: &ProjectManager,
        tool_name: String,
        arguments: Option<Map<String, Value>>,
        context: ToolExecutionContext,
    ) -> Result<CallToolResult, rmcp::ErrorData>;
}

/// Agent tool executor
pub struct AgentExecutor;

#[async_trait]
impl ToolExecutor for AgentExecutor {
    async fn execute(
        &self,
        project_manager: &ProjectManager,
        tool_name: String,
        arguments: Option<Map<String, Value>>,
        context: ToolExecutionContext,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        super::tools::run_agent_tool(
            project_manager,
            tool_name,
            arguments,
            context.session_filters,
            context.connection_overrides,
            context.meta_variables,
        )
        .await
    }
}

/// Workflow tool executor
pub struct WorkflowExecutor;

#[async_trait]
impl ToolExecutor for WorkflowExecutor {
    async fn execute(
        &self,
        project_manager: &ProjectManager,
        tool_name: String,
        arguments: Option<Map<String, Value>>,
        context: ToolExecutionContext,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        super::tools::run_workflow_tool(
            project_manager,
            tool_name,
            arguments,
            context.session_filters,
            context.connection_overrides,
            context.meta_variables,
        )
        .await
    }
}

/// Semantic topic tool executor
pub struct SemanticExecutor;

#[async_trait]
impl ToolExecutor for SemanticExecutor {
    async fn execute(
        &self,
        project_manager: &ProjectManager,
        tool_name: String,
        arguments: Option<Map<String, Value>>,
        context: ToolExecutionContext,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        super::tools::run_semantic_topic_tool(
            project_manager,
            tool_name,
            arguments,
            context.session_filters,
            context.connection_overrides,
            context.meta_variables,
        )
        .await
    }
}

/// SQL file tool executor
pub struct SqlExecutor;

#[async_trait]
impl ToolExecutor for SqlExecutor {
    async fn execute(
        &self,
        project_manager: &ProjectManager,
        tool_name: String,
        arguments: Option<Map<String, Value>>,
        context: ToolExecutionContext,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // Note: SQL file tools don't currently support meta_variables
        super::tools::run_sql_file_tool(
            project_manager,
            tool_name,
            arguments,
            context.session_filters,
            context.connection_overrides,
        )
        .await
    }
}
