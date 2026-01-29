//! Tool executor implementation for workflow-related tools
//!
//! This module provides ToolExecutor implementations that can be registered
//! with the core tool registry to handle Workflow and SemanticQuery execution.

use async_trait::async_trait;
use std::sync::Arc;

use oxy::{
    config::model::ToolType,
    execute::{Executable, ExecutionContext, types::OutputContainer},
    observability::events,
    tools::{ToolExecutor, types::ToolRawInput},
};
use oxy_shared::errors::OxyError;

use crate::{
    builders::{WorkflowInput, WorkflowLauncherExecutable},
    semantic_builder::build_semantic_query_executable,
};

/// Executor for Workflow tools
///
/// Handles execution of workflow files through the WorkflowLauncherExecutable.
pub struct WorkflowToolExecutor;

#[async_trait]
impl ToolExecutor for WorkflowToolExecutor {
    #[tracing::instrument(skip_all, err, fields(
        otel.name = events::tool::WORKFLOW_EXECUTE,
        oxy.span_type = events::tool::TOOL_CALL_TYPE,
        oxy.execution_type = events::tool::EXECUTION_TYPE_WORKFLOW,
        oxy.is_verified = tracing::field::Empty,
        oxy.workflow_ref = tracing::field::Empty,
        oxy.tool_input = tracing::field::Empty,
    ))]
    async fn execute(
        &self,
        execution_context: &ExecutionContext,
        tool_type: &ToolType,
        input: &ToolRawInput,
    ) -> Result<OutputContainer, OxyError> {
        events::tool::tool_call_input(input);
        match tool_type {
            ToolType::Workflow(workflow_config) => {
                // Record execution analytics fields
                let span = tracing::Span::current();
                span.record("oxy.is_verified", workflow_config.is_verified);
                span.record("oxy.workflow_ref", &workflow_config.workflow_ref);
                span.record("oxy.tool_input", &input.param);

                let workflow_input = WorkflowInput {
                    workflow_ref: workflow_config.name.clone(),
                    retry: oxy::checkpoint::types::RetryStrategy::NoRetry { variables: None },
                };

                let result = WorkflowLauncherExecutable
                    .execute(execution_context, workflow_input)
                    .await;

                match &result {
                    Ok(output) => events::tool::tool_call_output(output),
                    Err(e) => events::tool::tool_call_error(&e.to_string()),
                }

                result
            }
            _ => Err(OxyError::RuntimeError(
                "WorkflowToolExecutor can only handle Workflow tools".to_string(),
            )),
        }
    }

    fn can_handle(&self, tool_type: &ToolType) -> bool {
        matches!(tool_type, ToolType::Workflow(_))
    }

    fn name(&self) -> &'static str {
        "WorkflowToolExecutor"
    }
}

/// Executor for SemanticQuery tools
///
/// Handles semantic query execution through the semantic query builder.
pub struct SemanticQueryToolExecutor;

#[async_trait]
impl ToolExecutor for SemanticQueryToolExecutor {
    #[tracing::instrument(skip_all, err, fields(
        otel.name = events::tool::SEMANTIC_QUERY_EXECUTE,
        oxy.span_type = events::tool::TOOL_CALL_TYPE,
        oxy.execution_type = events::tool::EXECUTION_TYPE_SEMANTIC_QUERY,
        oxy.is_verified = true,
        oxy.topic = tracing::field::Empty,
        oxy.semantic_query_params = tracing::field::Empty,
        oxy.generated_sql = tracing::field::Empty,
    ))]
    async fn execute(
        &self,
        execution_context: &ExecutionContext,
        tool_type: &ToolType,
        input: &ToolRawInput,
    ) -> Result<OutputContainer, OxyError> {
        events::tool::tool_call_input(input);
        match tool_type {
            ToolType::SemanticQuery(semantic_config) => {
                // Parse the params from the input as SemanticQueryParams
                let mut query: oxy::types::SemanticQueryParams = serde_json::from_str(&input.param)
                    .map_err(|e| {
                        OxyError::ArgumentError(format!("Invalid semantic query params: {}", e))
                    })?;

                // Apply topic and variables from the tool config
                if let Some(topic) = &semantic_config.topic {
                    query.topic = Some(topic.clone());
                }
                if let Some(vars) = &semantic_config.variables {
                    query.variables = Some(vars.clone());
                }

                // Record execution analytics fields
                let span = tracing::Span::current();
                span.record("oxy.topic", query.topic.as_deref().unwrap_or("unknown"));
                span.record(
                    "oxy.semantic_query_params",
                    serde_json::to_string(&query).unwrap_or_default(),
                );

                events::tool::record_semantic_tool_call_metric(
                    execution_context,
                    query.topic.as_deref().unwrap_or("unknown"),
                    &query,
                );

                let task = oxy::config::model::SemanticQueryTask {
                    query,
                    export: None,
                    variables: semantic_config.variables.clone(),
                };

                let result = build_semantic_query_executable()
                    .execute(execution_context, task)
                    .await;

                match &result {
                    Ok(output) => {
                        // Record generated SQL for tracing
                        if let oxy::execute::types::Output::Table(table) = output
                            && let Some(ref reference) = table.reference
                        {
                            println!("generated sql: {}", reference.sql);
                            span.record("oxy.generated_sql", &reference.sql);
                        }
                        events::tool::tool_call_output(output);
                    }
                    Err(e) => events::tool::tool_call_error(&e.to_string()),
                }

                result.map(OutputContainer::Single)
            }
            _ => Err(OxyError::RuntimeError(
                "SemanticQueryToolExecutor can only handle SemanticQuery tools".to_string(),
            )),
        }
    }

    fn can_handle(&self, tool_type: &ToolType) -> bool {
        matches!(tool_type, ToolType::SemanticQuery(_))
    }

    fn name(&self) -> &'static str {
        "SemanticQueryToolExecutor"
    }
}

/// Register all workflow-related tool executors
///
/// Call this function during application initialization to register
/// workflow and semantic query executors with the global registry.
///
/// # Errors
///
/// Returns an error if registration fails (currently infallible, but
/// allows for future validation logic).
pub async fn register_workflow_executors() -> Result<(), oxy_shared::errors::OxyError> {
    oxy::tools::register_tool_executor(Arc::new(WorkflowToolExecutor)).await?;
    oxy::tools::register_tool_executor(Arc::new(SemanticQueryToolExecutor)).await?;
    tracing::info!("Registered workflow tool executors");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_executor_can_handle() {
        let executor = WorkflowToolExecutor;

        let workflow_tool = ToolType::Workflow(oxy::config::model::WorkflowTool {
            name: "test".to_string(),
            description: "test".to_string(),
            workflow_ref: "test.yml".to_string(),
            variables: None,
            output_task_ref: None,
            is_verified: false,
        });

        assert!(executor.can_handle(&workflow_tool));
    }

    #[test]
    fn test_semantic_query_executor_can_handle() {
        let executor = SemanticQueryToolExecutor;

        let semantic_tool = ToolType::SemanticQuery(oxy::config::model::SemanticQueryTool {
            name: "test".to_string(),
            description: "test description".to_string(),
            topic: Some("test_topic".to_string()),
            dry_run_limit: None,
            variables: None,
        });

        assert!(executor.can_handle(&semantic_tool));
    }
}
