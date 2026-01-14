use async_openai::types::chat::{
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
            openai::{OpenAIOrOSSExecutable, build_openai_executable},
            tool::OpenAITool,
        },
    },
    config::{
        ConfigManager,
        model::{Model, ReasoningConfig, ToolType},
    },
    errors::OxyError,
    execute::{Executable, ExecutionContext},
    observability::events,
};

#[derive(Debug, Clone)]
pub struct FallbackAgent {
    agent_name: String,
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
            agent_name: agent_name.to_string(),
            agent: build_openai_executable(
                OpenAIClient::with_config(model.into_openai_config(secrets_manager).await?),
                model_name.to_string(),
                vec![ChatCompletionTool::from_tool_async(&tool_config, config).await],
                Some(ChatCompletionToolChoiceOption::Function(
                    tool_config.clone().into(),
                )),
                reasoning_config,
                true,
            ),
            tool_executable: OpenAITool::new(agent_name.to_string(), vec![tool_config], 1),
        })
    }
}

#[async_trait::async_trait]
impl Executable<Vec<ChatCompletionRequestMessage>> for FallbackAgent {
    type Response = Vec<OpenAIExecutableResponse>;

    #[tracing::instrument(skip_all, err, fields(
        otel.name = events::agent::fallback_agent::FALLBACK_NAME,
        oxy.span_type = events::agent::fallback_agent::FALLBACK_TYPE,
        oxy.agent.name = %self.agent_name,
    ))]
    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: Vec<ChatCompletionRequestMessage>,
    ) -> Result<Self::Response, OxyError> {
        events::agent::fallback_agent::input(&input);

        events::agent::fallback_agent::agent(&self.agent);

        let mut memo = input.clone();
        let response = self.agent.execute(execution_context, input).await?;
        let tool_rets = self
            .tool_executable
            .execute(execution_context, response.clone())
            .await?;

        let result = match tool_rets {
            Some(tool_calls) => {
                memo.extend(tool_calls);
                let fallback_response = self.agent.execute(execution_context, memo).await?;
                vec![response, fallback_response]
            }
            None => vec![response],
        };

        events::agent::fallback_agent::output(&result);

        Ok(result)
    }
}
