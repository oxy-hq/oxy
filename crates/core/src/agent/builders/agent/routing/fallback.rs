use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionTool, ChatCompletionToolChoiceOption,
};

use crate::{
    adapters::{
        openai::{AsyncFunctionObject, IntoOpenAIConfig, OpenAIClient},
        secrets::SecretsManager,
    },
    agent::{
        OpenAIExecutableResponse,
        builders::{
            openai::{OpenAIExecutable, OpenAIOrOSSExecutable, build_openai_executable},
            tool::OpenAITool,
        },
    },
    config::{
        ConfigManager,
        model::{Model, ReasoningConfig, ToolType},
    },
    errors::OxyError,
    execute::{Executable, ExecutionContext},
};

#[derive(Debug, Clone)]
pub struct FallbackAgent {
    agent: OpenAIOrOSSExecutable,
    tool_executable: OpenAITool,
}

impl FallbackAgent {
    pub async fn new(
        agent_name: &str,
        model: &Model,
        tool_config: ToolType,
        config: &ConfigManager,
        secrets_manager: &SecretsManager,
        reasoning_config: Option<ReasoningConfig>,
    ) -> Result<Self, OxyError> {
        let model_name = model.model_name();
        Ok(Self {
            agent: build_openai_executable(
                OpenAIClient::with_config(model.into_openai_config(secrets_manager).await?),
                model_name.to_string(),
                vec![ChatCompletionTool::from_tool_async(&tool_config, config).await],
                Some(ChatCompletionToolChoiceOption::Named(
                    tool_config.clone().into(),
                )),
                reasoning_config.map(|rc| rc.into()),
                true,
            ),
            tool_executable: OpenAITool::new(agent_name.to_string(), vec![tool_config], 1),
        })
    }
}

#[async_trait::async_trait]
impl Executable<Vec<ChatCompletionRequestMessage>> for FallbackAgent {
    type Response = Vec<OpenAIExecutableResponse>;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: Vec<ChatCompletionRequestMessage>,
    ) -> Result<Self::Response, OxyError> {
        let mut memo = input.clone();
        let response = self.agent.execute(execution_context, input).await?;
        let tool_rets = self
            .tool_executable
            .execute(execution_context, response.clone())
            .await?;
        match tool_rets {
            Some(tool_calls) => {
                memo.extend(tool_calls);
                let fallback_response = self.agent.execute(execution_context, memo).await?;
                Ok(vec![response, fallback_response])
            }
            None => Ok(vec![response]),
        }
    }
}
