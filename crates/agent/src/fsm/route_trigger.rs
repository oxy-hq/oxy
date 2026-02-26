use async_openai::types::chat::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestSystemMessageContent, ChatCompletionTool,
};

use crate::{
    fsm::{
        config::{AgenticConfig, RoutingConfig},
        state::MachineContext,
    },
    routing::RouteResolver,
};
use oxy::{
    adapters::openai::{AsyncFunctionObject, OpenAIAdapter},
    execute::{
        Executable, ExecutionContext,
        builders::fsm::Trigger,
        types::{Chunk, Output},
    },
    tools::{ToolInput, ToolLauncherExecutable, types::ToolRawInput},
};
use oxy_shared::errors::OxyError;

pub struct RouteTrigger {
    adapter: OpenAIAdapter,
    routing_config: RoutingConfig,
    agentic_config: AgenticConfig,
}

impl RouteTrigger {
    pub fn new(
        adapter: OpenAIAdapter,
        routing_config: RoutingConfig,
        agentic_config: AgenticConfig,
    ) -> Self {
        Self {
            adapter,
            routing_config,
            agentic_config,
        }
    }

    async fn signal_fallback(
        &self,
        execution_context: &ExecutionContext,
        state: &mut MachineContext,
    ) -> Result<(), OxyError> {
        let ctx = execution_context
            .with_child_source(uuid::Uuid::new_v4().to_string(), "text".to_string());
        ctx.write_chunk(Chunk {
            key: None,
            delta: Output::Text("No route found. Proceeding to plan.".to_string()),
            finished: true,
        })
        .await?;
        state.set_route_fallback();
        Ok(())
    }
}

#[async_trait::async_trait]
impl Trigger for RouteTrigger {
    type State = MachineContext;

    async fn run(
        &self,
        execution_context: &ExecutionContext,
        state: &mut Self::State,
    ) -> Result<(), OxyError> {
        let agent_name = &self.agentic_config.name;

        // Step 1: Resolve routes via vector search
        let tool_configs = RouteResolver::resolve_routes(
            execution_context,
            agent_name,
            &self.routing_config.db_config,
            &self.routing_config.embedding_config,
            &self.routing_config.api_url,
            &self.routing_config.key_var,
            state.user_query(),
        )
        .await?;

        if tool_configs.is_empty() {
            tracing::info!("No routes found via vector search, falling through");
            return self.signal_fallback(execution_context, state).await;
        }

        // Step 2: Render tools into ChatCompletionTools (concurrently)
        let config_manager = &execution_context.project.config_manager;
        let rendered_tools = futures::future::try_join_all(
            tool_configs
                .iter()
                .map(|tool| tool.render(&execution_context.renderer)),
        )
        .await?;

        let tools: Vec<ChatCompletionTool> = futures::future::join_all(
            rendered_tools
                .iter()
                .map(|tool| ChatCompletionTool::from_tool_async(tool, config_manager)),
        )
        .await
        .into_iter()
        .collect();

        if tools.is_empty() {
            tracing::info!("No tools rendered, falling through");
            return self.signal_fallback(execution_context, state).await;
        }

        // Step 3: LLM tool selection with tool_choice: auto (LLM can decline)
        let messages: Vec<ChatCompletionRequestMessage> = vec![
            ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(
                    "You are a routing agent. Based on the user's query, select the most appropriate tool to handle it. If none of the available tools are a good match, do not call any tool.".to_string(),
                ),
                ..Default::default()
            }
            .into(),
            async_openai::types::chat::ChatCompletionRequestUserMessageArgs::default()
                .content(state.user_query().to_string())
                .build()
                .map_err(|e| OxyError::RuntimeError(format!("Failed to build user message: {e}")))?
                .into(),
        ];

        let (response, tool_calls) = self
            .adapter
            .request_tool_call_with_usage(
                execution_context,
                messages,
                tools,
                None, // tool_choice: auto (default)
                None,
            )
            .await?;

        if tool_calls.is_empty() {
            tracing::info!("LLM declined to route, falling through");
            // Add the LLM's reasoning to message history so the Plan step
            // knows that routing was attempted and why it was declined.
            if let Some(reasoning) = response {
                state.add_message(reasoning);
            }
            return self.signal_fallback(execution_context, state).await;
        }

        if tool_calls.len() > 1 {
            tracing::warn!(
                "LLM returned {} tool calls; only the first will be used",
                tool_calls.len()
            );
        }

        // Step 4: Execute the selected tool
        let tool_call = &tool_calls[0];
        let route_name = tool_call.function.name.clone();
        tracing::info!("Route matched: {}", route_name);

        // Emit route selection info in the reasoning trace
        let route_context = execution_context
            .with_child_source(uuid::Uuid::new_v4().to_string(), "text".to_string());
        route_context
            .write_chunk(Chunk {
                key: None,
                delta: Output::Text(format!("Selected route: **{}**", route_name)),
                finished: true,
            })
            .await?;

        let raw_input = ToolRawInput::from(tool_call);

        let tool_input = ToolInput {
            raw: raw_input,
            agent_name: agent_name.to_string(),
            tools: rendered_tools,
        };

        let output = ToolLauncherExecutable
            .execute(execution_context, tool_input)
            .await?;

        // Step 5: Add result to state
        state.add_message(output.to_string());

        // Step 6: Signal route completed — next_trigger will create a proper End step
        state.set_route_completed();

        Ok(())
    }
}
