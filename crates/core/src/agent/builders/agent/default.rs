use crate::adapters::openai::AsyncFunctionObject;
use crate::agent::OpenAIExecutableResponse;
use crate::agent::builders::openai::OpenAIExecutable;
use crate::agent::builders::tool::OpenAITool;
use crate::agent::contexts::Contexts;
use crate::agent::databases::DatabasesContext;
use crate::config::constants::AGENT_SOURCE_PROMPT;
use crate::config::model::{AgentContext, AgentToolsConfig, DefaultAgent};
use crate::execute::builders::map::ParamMapper;
use crate::execute::renderer::Renderer;
use crate::tools::ToolsContext;
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
use minijinja::{Value, context};

#[derive(Debug, Clone)]
pub(super) struct DefaultAgentExecutable;

#[derive(Debug, Clone)]
pub struct DefaultAgentInput {
    pub agent_name: String,
    pub model: String,
    pub default_agent: DefaultAgent,
    pub contexts: Option<Vec<AgentContext>>,
    pub prompt: String,
}

#[async_trait::async_trait]
impl Executable<DefaultAgentInput> for DefaultAgentExecutable {
    type Response = Output;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: DefaultAgentInput,
    ) -> Result<Self::Response, OxyError> {
        let DefaultAgentInput {
            agent_name,
            model,
            prompt,
            contexts: _,
            default_agent:
                DefaultAgent {
                    system_instructions,
                    tools_config:
                        AgentToolsConfig {
                            tools,
                            max_tool_calls,
                            max_tool_concurrency,
                        },
                },
        } = input;
        let model_config = execution_context.config.resolve_model(&model)?;
        let system_instructions = execution_context
            .renderer
            .render_async(&system_instructions)
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
            agent_name,
            tools,
            max_tool_concurrency,
            client,
            model_config.model_name().to_string(),
            max_tool_calls,
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
    let tools: Vec<ChatCompletionTool> =
        futures::future::join_all(tool_configs.iter().map(ChatCompletionTool::from_tool_async))
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

#[derive(Clone)]
pub struct DefaultAgentMapper;

#[async_trait::async_trait]
impl ParamMapper<DefaultAgentInput, DefaultAgentInput> for DefaultAgentMapper {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: DefaultAgentInput,
    ) -> Result<(DefaultAgentInput, Option<ExecutionContext>), OxyError> {
        let default_agent = &input.default_agent;
        let global_context = build_global_context(
            &execution_context,
            &input.agent_name,
            default_agent,
            input.contexts.clone().unwrap_or_default(),
            &input.prompt,
        )?;
        let renderer = Renderer::from_template(
            global_context,
            &input.default_agent.system_instructions.as_str(),
        )?;
        let execution_context = execution_context.wrap_renderer(renderer);
        Ok((input, Some(execution_context)))
    }
}

fn build_global_context(
    execution_context: &ExecutionContext,
    agent_name: &str,
    default_agent: &DefaultAgent,
    contexts: Vec<AgentContext>,
    prompt: &str,
) -> Result<Value, OxyError> {
    let contexts = Contexts::new(contexts, execution_context.config.clone());
    let databases = DatabasesContext::new(execution_context.config.clone());
    let tools = ToolsContext::from_execution_context(
        execution_context,
        agent_name.to_string(),
        default_agent.tools_config.tools.clone(),
        prompt.to_string(),
    );
    Ok(context! {
        context => Value::from_object(contexts),
        databases => Value::from_object(databases),
        tools => Value::from_object(tools)
    })
}

pub(super) fn build_default_agent_executable()
-> impl Executable<DefaultAgentInput, Response = Output> {
    ExecutableBuilder::new()
        .map(DefaultAgentMapper)
        .executable(DefaultAgentExecutable)
}
