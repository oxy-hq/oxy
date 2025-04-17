use super::openai::{OpenAIExecutable, OpenAIExecutableResponse};
use super::tool::OpenAITool;
use crate::adapters::openai::AsyncFunctionObject;
use crate::config::constants::AGENT_SOURCE_PROMPT;
use crate::config::model::AgentConfig;
use crate::{
    adapters::openai::OpenAIClient,
    config::model::ToolType,
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::ExecutableBuilder,
        types::{Chunk, Output, Prompt},
    },
};
use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestUserMessageArgs, ChatCompletionTool,
};
use futures;

#[derive(Debug, Clone)]
pub struct AgentExecutable;

#[async_trait::async_trait]
impl Executable<(AgentConfig, String)> for AgentExecutable {
    type Response = Output;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: (AgentConfig, String),
    ) -> Result<Self::Response, OxyError> {
        let (agent_config, prompt) = input;
        let model_config = execution_context
            .config
            .resolve_model(&agent_config.model)?;
        let system_instructions = execution_context
            .renderer
            .render_async(&agent_config.system_instructions)
            .await?;
        let client = OpenAIClient::with_config(model_config.try_into()?);
        let messages: Vec<ChatCompletionRequestMessage> = vec![
            ChatCompletionRequestSystemMessageArgs::default()
                .content(system_instructions)
                .build()?
                .into(),
            ChatCompletionRequestUserMessageArgs::default()
                .content(prompt.clone())
                .build()?
                .into(),
        ];
        execution_context
            .write_chunk(Chunk {
                key: Some(AGENT_SOURCE_PROMPT.to_string()),
                delta: Prompt::new(prompt.clone()).into(),
                finished: true,
            })
            .await?;
        let mut react_executable = build_react_loop(
            agent_config.name.clone(),
            agent_config.tools_config.tools.clone(),
            agent_config.tools_config.max_tool_concurrency,
            client,
            model_config.model_name().to_string(),
            agent_config.tools_config.max_tool_calls,
        )
        .await;
        let response = react_executable
            .execute(execution_context, messages)
            .await?;
        Ok(response.content)
    }
}

async fn build_react_loop(
    agent_name: String,
    tool_configs: Vec<ToolType>,
    max_concurrency: usize,
    client: OpenAIClient,
    model: String,
    max_iterations: usize,
) -> impl Executable<Vec<ChatCompletionRequestMessage>, Response = OpenAIExecutableResponse> {
    let tools: Vec<ChatCompletionTool> = futures::future::join_all(
        tool_configs
            .iter()
            .map(|tool| ChatCompletionTool::from_tool_async(tool)),
    )
    .await
    .into_iter()
    .collect();
    ExecutableBuilder::new()
        .react(
            OpenAITool::new(agent_name, tool_configs, max_concurrency),
            |response: &OpenAIExecutableResponse,
             new_response: Option<&OpenAIExecutableResponse>| {
                match new_response {
                    Some(new_response) => OpenAIExecutableResponse {
                        content: response.content.merge(&new_response.content),
                        tool_calls: response
                            .tool_calls
                            .iter()
                            .chain(new_response.tool_calls.iter())
                            .cloned()
                            .collect(),
                    },
                    None => OpenAIExecutableResponse {
                        content: response.content.clone(),
                        tool_calls: response.tool_calls.clone(),
                    },
                }
            },
            |input: &Vec<ChatCompletionRequestMessage>,
             new_input: &Vec<ChatCompletionRequestMessage>| {
                input.iter().chain(new_input.iter()).cloned().collect()
            },
            max_iterations,
        )
        .executable(OpenAIExecutable::new(client, model, tools))
}
