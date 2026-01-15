use async_openai::types::chat::{
    ChatCompletionMessageToolCall, ChatCompletionMessageToolCalls,
    ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
    ChatCompletionRequestToolMessageArgs,
};

use super::openai::OpenAIExecutableResponse;

use oxy::{
    config::model::ToolType,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, map::ParamMapper},
        types::{EventKind, OutputContainer},
    },
    observability::events,
    tools::{ToolInput, ToolLauncherExecutable},
};
use oxy_shared::errors::OxyError;

#[derive(Clone, Debug)]
pub struct OpenAITool {
    agent_name: String,
    max_concurrency: usize,
    tool_configs: Vec<ToolType>,
}

impl OpenAITool {
    pub fn new(
        agent_name: String,
        tool_configs: impl IntoIterator<Item = ToolType>,
        max_concurrency: usize,
    ) -> Self {
        Self {
            agent_name,
            max_concurrency,
            tool_configs: tool_configs.into_iter().collect(),
        }
    }
}

#[async_trait::async_trait]
impl Executable<OpenAIExecutableResponse> for OpenAITool {
    type Response = Option<Vec<ChatCompletionRequestMessage>>;

    #[tracing::instrument(skip_all, err, fields(
        otel.name = events::tool::TOOL_EXECUTE,
        oxy.span_type = events::tool::TOOL_TYPE,
        oxy.agent.name = %self.agent_name,
    ))]
    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: OpenAIExecutableResponse,
    ) -> Result<Self::Response, OxyError> {
        if input.tool_calls.is_empty() {
            tracing::debug!("No tool calls to execute");
            return Ok(None);
        }

        events::tool::input(&input);

        let response = build_tool_executable(
            self.agent_name.to_string(),
            self.tool_configs.clone(),
            self.max_concurrency,
        )
        .execute(execution_context, input.tool_calls.clone())
        .await?;

        let success_count = response.iter().filter(|r| r.is_ok()).count();
        let error_count = response.iter().filter(|r| r.is_err()).count();

        tracing::info!(
            tool.success_count = success_count,
            tool.error_count = error_count,
            "Tool execution completed"
        );

        for tool_ret in response.iter() {
            if let Err(e) = tool_ret {
                tracing::error!(tool.error = %e, "Tool execution failed");
                execution_context
                    .write_kind(EventKind::Error {
                        message: e.to_string(),
                    })
                    .await?;
            }
        }

        let tool_rets: Result<Vec<_>, OxyError> = input
            .tool_calls
            .iter()
            .zip(response)
            .map(|(c, r)| -> Result<ChatCompletionRequestMessage, OxyError> {
                match r {
                    Ok(o) => Ok(ChatCompletionRequestToolMessageArgs::default()
                        .tool_call_id(c.id.clone())
                        .content(o.to_string())
                        .build()
                        .map_err(|e| {
                            OxyError::RuntimeError(format!(
                                "Failed to build tool message for success: {e}"
                            ))
                        })?
                        .into()),
                    Err(e) => Ok(ChatCompletionRequestToolMessageArgs::default()
                        .tool_call_id(c.id.clone())
                        .content(e.to_string())
                        .build()
                        .map_err(|e| {
                            OxyError::RuntimeError(format!(
                                "Failed to build tool message for error: {e}"
                            ))
                        })?
                        .into()),
                }
            })
            .collect();
        let tool_rets = tool_rets?;
        let agent_message = ChatCompletionRequestAssistantMessageArgs::default()
            .tool_calls(
                input
                    .tool_calls
                    .into_iter()
                    .map(ChatCompletionMessageToolCalls::Function)
                    .collect::<Vec<ChatCompletionMessageToolCalls>>(),
            )
            .build()?;
        let mut result = vec![agent_message.into()];
        result.extend_from_slice(&tool_rets);

        events::tool::output(&result);

        Ok(Some(result))
    }
}

#[derive(Clone, Debug)]
pub struct ToolMapper;

#[async_trait::async_trait]
impl ParamMapper<((String, Vec<ToolType>), ChatCompletionMessageToolCall), ToolInput>
    for ToolMapper
{
    async fn map(
        &self,
        _execution_context: &ExecutionContext,
        input: ((String, Vec<ToolType>), ChatCompletionMessageToolCall),
    ) -> Result<(ToolInput, Option<ExecutionContext>), OxyError> {
        let ((agent_name, tools), tool_call) = input;
        Ok((
            ToolInput {
                raw: tool_call.into(),
                agent_name,
                tools,
            },
            None,
        ))
    }
}

fn build_tool_executable(
    agent_name: String,
    tool_configs: Vec<ToolType>,
    max: usize,
) -> impl Executable<Vec<ChatCompletionMessageToolCall>, Response = Vec<Result<OutputContainer, OxyError>>>
{
    ExecutableBuilder::new()
        .concurrency::<ChatCompletionMessageToolCall>(max)
        .state((agent_name, tool_configs))
        .map(ToolMapper)
        .executable(ToolLauncherExecutable)
}
