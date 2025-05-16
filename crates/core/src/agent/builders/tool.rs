use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessageArgs,
    ChatCompletionRequestMessage, ChatCompletionRequestToolMessageArgs,
};

use crate::{
    config::model::ToolType,
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, map::ParamMapper},
        types::{EventKind, OutputContainer},
    },
    theme::StyledText,
    tools::{ToolInput, ToolLauncherExecutable},
};

use super::openai::OpenAIExecutableResponse;

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

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: OpenAIExecutableResponse,
    ) -> Result<Self::Response, OxyError> {
        if input.tool_calls.is_empty() {
            return Ok(None);
        }

        let assistant_message = ChatCompletionRequestAssistantMessageArgs::default()
            .tool_calls(input.tool_calls.clone())
            .build()?;
        let response = build_tool_executable(
            self.agent_name.to_string(),
            self.tool_configs.clone(),
            self.max_concurrency,
        )
        .execute(execution_context, input.tool_calls.clone())
        .await?;

        for tool_ret in response.iter() {
            if let Err(e) = tool_ret {
                execution_context
                    .write_kind(EventKind::Message {
                        message: e.to_string().error().to_string(),
                    })
                    .await?;
            }
        }

        let tool_rets = input
            .tool_calls
            .iter()
            .map(|c| c.id.clone())
            .zip(response)
            .map(|(c, r)| match r {
                Ok(o) => ChatCompletionRequestToolMessageArgs::default()
                    .tool_call_id(c)
                    .content(o.to_string())
                    .build()
                    .unwrap()
                    .into(),
                Err(e) => ChatCompletionRequestToolMessageArgs::default()
                    .tool_call_id(c)
                    .content(e.to_string())
                    .build()
                    .unwrap()
                    .into(),
            })
            .collect::<Vec<ChatCompletionRequestMessage>>();
        let mut result = vec![assistant_message.into()];
        result.extend_from_slice(&tool_rets);
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
