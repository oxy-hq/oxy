//! Tool executor implementation for agent-to-agent calls
//!
//! This module provides a ToolExecutor implementation that can be registered
//! with the core tool registry to handle Agent tool execution.

use async_trait::async_trait;
use std::sync::Arc;

use oxy::{
    config::model::ToolType,
    execute::{Executable, ExecutionContext, types::OutputContainer},
    tools::{ToolExecutor, types::ToolRawInput},
};
use oxy_shared::errors::OxyError;

use crate::{agent_launcher::AgentLauncherExecutable, types::AgentInput};

/// Executor for Agent tools
///
/// Handles agent-to-agent calls through the AgentLauncherExecutable.
pub struct AgentToolExecutor;

#[async_trait]
impl ToolExecutor for AgentToolExecutor {
    async fn execute(
        &self,
        execution_context: &ExecutionContext,
        tool_type: &ToolType,
        input: &ToolRawInput,
    ) -> Result<OutputContainer, OxyError> {
        match tool_type {
            ToolType::Agent(agent_config) => {
                // Create agent input from the tool configuration and parameters
                let agent_input = AgentInput {
                    agent_ref: agent_config.agent_ref.clone(),
                    prompt: input.param.clone(),
                    memory: vec![],
                    variables: agent_config.variables.as_ref().map(|v| v.into()),
                    a2a_task_id: None,
                    a2a_thread_id: None,
                    a2a_context_id: None,
                    sandbox_info: None,
                };

                AgentLauncherExecutable
                    .execute(execution_context, agent_input)
                    .await
            }
            _ => Err(OxyError::RuntimeError(
                "AgentToolExecutor can only handle Agent tools".to_string(),
            )),
        }
    }

    fn can_handle(&self, tool_type: &ToolType) -> bool {
        matches!(tool_type, ToolType::Agent(_))
    }

    fn name(&self) -> &'static str {
        "AgentToolExecutor"
    }
}

/// Register the agent tool executor
///
/// Call this function during application initialization to register
/// the agent executor with the global registry.
///
/// # Errors
///
/// Returns an error if registration fails (currently infallible, but
/// allows for future validation logic).
pub async fn register_agent_executor() -> Result<(), oxy_shared::errors::OxyError> {
    oxy::tools::register_tool_executor(Arc::new(AgentToolExecutor)).await?;
    tracing::info!("Registered agent tool executor");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_executor_can_handle() {
        let executor = AgentToolExecutor;

        let agent_tool = ToolType::Agent(oxy::config::model::AgentTool {
            name: "test_agent".to_string(),
            description: "test".to_string(),
            agent_ref: "test_agent".to_string(),
            variables: None,
            is_verified: false,
        });

        assert!(executor.can_handle(&agent_tool));
    }
}
