mod default;
mod routing;

use std::{collections::HashMap, sync::Arc};

use default::{DefaultAgentInput, build_default_agent_executable};
use routing::{RoutingAgentExecutable, RoutingAgentInput};

use crate::{
    agent::{AgentReferencesHandler, types::AgentInput},
    config::{
        constants::{AGENT_SOURCE, AGENT_SOURCE_PROMPT, AGENT_SOURCE_TYPE},
        model::AgentType,
    },
    errors::OxyError,
    execute::{
        Executable, ExecutionContext, execute_with_handler,
        types::{Metadata, OutputContainer},
    },
};

#[derive(Debug, Clone)]
pub struct AgentExecutable;

#[async_trait::async_trait]
impl Executable<AgentInput> for AgentExecutable {
    type Response = OutputContainer;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: AgentInput,
    ) -> Result<Self::Response, OxyError> {
        let AgentInput {
            agent_ref,
            prompt,
            memory,
            variables: runtime_variables,
            a2a_task_id: _,
            a2a_thread_id: _,
            a2a_context_id: _,
        } = input;
        let config_manager = &execution_context.project.config_manager;
        let agent_config = config_manager
            .resolve_agent(&agent_ref)
            .await
            .map_err(|e| {
                OxyError::ConfigurationError(format!("Failed to resolve agent config: {e}"))
            })?;

        // Resolve variables by merging runtime params with defaults
        let resolved_variables = if let Some(variables_config) = &agent_config.variables {
            variables_config.resolve_params(runtime_variables)?
        } else if runtime_variables.is_some() {
            tracing::warn!(
                "Runtime variables provided but agent '{}' has no variables schema",
                agent_ref
            );
            HashMap::new()
        } else {
            HashMap::new()
        };

        // Build template context with variables
        // Variables are added to the renderer context alongside globals
        let routing_context = if !resolved_variables.is_empty() {
            let variables_value = minijinja::Value::from_serialize(&resolved_variables);
            tracing::info!(
                "Agent '{}' resolved variables: {:?}",
                agent_ref,
                resolved_variables
            );
            execution_context.wrap_render_context(&variables_value)
        } else {
            execution_context.clone()
        };

        let source_id = short_uuid::short!();
        let handler = AgentReferencesHandler::new(
            execution_context.writer.clone(),
            Some(source_id.to_string()),
        );
        let references = handler.references.clone();
        let metadata = HashMap::from_iter([
            (
                AGENT_SOURCE_TYPE.to_string(),
                agent_config.r#type.to_string(),
            ),
            (AGENT_SOURCE_PROMPT.to_string(), prompt.to_string()),
        ]);
        let routing_context =
            routing_context.with_child_source(source_id.to_string(), AGENT_SOURCE.to_string());
        let output_container = match agent_config.r#type {
            AgentType::Default(default_agent) => {
                tracing::debug!("Executing default agent: {:?}", &default_agent);
                let default_agent_executable = build_default_agent_executable();
                execute_with_handler(
                    default_agent_executable,
                    &routing_context,
                    DefaultAgentInput {
                        agent_name: agent_config.name,
                        model: agent_config.model,
                        default_agent,
                        contexts: agent_config.context,
                        prompt,
                        memory,
                        reasoning_config: agent_config.reasoning,
                    },
                    handler,
                )
                .await
            }
            AgentType::Routing(routing_agent) => {
                tracing::debug!("Executing routing agent: {:?}", &routing_agent);
                execute_with_handler(
                    RoutingAgentExecutable,
                    &routing_context,
                    RoutingAgentInput {
                        agent_name: agent_config.name,
                        model: agent_config.model,
                        routing_agent,
                        prompt,
                        memory,
                        reasoning_config: agent_config.reasoning,
                    },
                    handler,
                )
                .await
            }
        }?;

        let references = Arc::try_unwrap(references)
            .map_err(|_| OxyError::RuntimeError("Failed to unwrap agent references".to_string()))?
            .into_inner()?;
        Ok(OutputContainer::Metadata {
            value: Metadata {
                output: Box::new(output_container),
                references,
                metadata,
            },
        })
    }
}
