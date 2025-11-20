use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessage,
    ChatCompletionRequestAssistantMessageContent, ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessage, ChatCompletionRequestSystemMessageContent,
    ChatCompletionRequestToolMessage, ChatCompletionRequestToolMessageContent,
    ChatCompletionToolChoiceOption,
};

use crate::{
    adapters::openai::OpenAIAdapter,
    agent::builders::fsm::{
        control::TransitionContext, query::PrepareData, viz::config::Visualize,
    },
    errors::OxyError,
    execute::{
        ExecutionContext,
        builders::fsm::Trigger,
        types::{EventKind, VizParams},
    },
};

pub struct GenerateViz<S> {
    objective: String,
    adapter: OpenAIAdapter,
    config: Visualize,
    _state: std::marker::PhantomData<S>,
}

impl<S> GenerateViz<S>
where
    S: PrepareData,
{
    pub fn new(objective: String, adapter: OpenAIAdapter, config: Visualize) -> Self {
        Self {
            objective,
            adapter,
            config,
            _state: std::marker::PhantomData,
        }
    }

    async fn prepare_instructions(
        &self,
        execution_context: &ExecutionContext,
        current_state: &S,
    ) -> Result<Vec<ChatCompletionRequestMessage>, OxyError> {
        let instruction = execution_context
            .renderer
            .render_async(&self.config.instruction)
            .await?;
        let tables = current_state.get_tables();
        let messages = vec![
            ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(instruction),
                ..Default::default()
            }
            .into(),
            ChatCompletionRequestSystemMessage {
                name: None,
                content: ChatCompletionRequestSystemMessageContent::Text(format!(
                    "You have access to the following tables:\n{}",
                    tables
                        .iter()
                        .map(|t| t.to_string())
                        .collect::<Vec<String>>()
                        .join("\n")
                )),
            }
            .into(),
            ChatCompletionRequestAssistantMessage {
                content: Some(ChatCompletionRequestAssistantMessageContent::Text(
                    self.objective.to_string(),
                )),
                ..Default::default()
            }
            .into(),
        ];
        Ok(messages)
    }

    async fn request_viz_tool_call(
        &self,
        execution_context: &ExecutionContext,
        messages: Vec<ChatCompletionRequestMessage>,
    ) -> Result<ChatCompletionMessageToolCall, OxyError> {
        let tool_calls = self
            .adapter
            .request_tool_call_with_usage(
                execution_context,
                messages,
                vec![self.config.get_tool()],
                Some(ChatCompletionToolChoiceOption::Required),
                None,
            )
            .await?;
        let tool_call = tool_calls.first().ok_or_else(|| {
            OxyError::RuntimeError("No tool call returned from OpenAI".to_string())
        })?;
        Ok(tool_call.clone())
    }

    async fn validate_viz(
        &self,
        tool_call: &ChatCompletionMessageToolCall,
    ) -> Result<VizParams, OxyError> {
        let viz: VizParams = serde_json::from_str(&tool_call.function.arguments)?;
        Ok(viz)
    }

    async fn run_with_retry(
        &self,
        execution_context: &ExecutionContext,
        current_state: &S,
    ) -> Result<(VizParams, ChatCompletionMessageToolCall), OxyError> {
        let instructions = self
            .prepare_instructions(execution_context, current_state)
            .await?;
        let config = &self.config;
        let max_retries = config.max_retries;
        let mut failed_messages = vec![];

        loop {
            let tool_call = self
                .request_viz_tool_call(
                    execution_context,
                    [instructions.clone(), failed_messages.clone()].concat(),
                )
                .await?;
            match self.validate_viz(&tool_call).await {
                Ok(viz) => return Ok((viz, tool_call)),
                Err(e) => {
                    if failed_messages.len() as u32 / 2 >= max_retries {
                        return Err(OxyError::RuntimeError(format!(
                            "Visualize failed after {max_retries} retries: {e}",
                        )));
                    }
                    failed_messages.push(
                        ChatCompletionRequestAssistantMessage {
                            tool_calls: Some(vec![tool_call.clone()]),
                            ..Default::default()
                        }
                        .into(),
                    );
                    failed_messages.push(
                        ChatCompletionRequestToolMessage {
                            tool_call_id: tool_call.id.clone(),
                            content: ChatCompletionRequestToolMessageContent::Text(format!(
                                "The previous visualization failed with error: {}. Please try again.",
                                e
                            )),
                        }
                        .into(),
                    );
                }
            }
        }
    }
}

pub trait CollectViz {
    fn list_viz(&self) -> &[VizParams];
    fn collect_viz(&mut self, viz: VizParams);
}

pub trait CollectVizDelegator {
    fn target(&self) -> &dyn CollectViz;
    fn target_mut(&mut self) -> &mut dyn CollectViz;
}

impl<T> CollectViz for T
where
    T: CollectVizDelegator,
{
    fn list_viz(&self) -> &[VizParams] {
        self.target().list_viz()
    }

    fn collect_viz(&mut self, viz: VizParams) {
        self.target_mut().collect_viz(viz)
    }
}

#[async_trait::async_trait]
impl<S> Trigger for GenerateViz<S>
where
    S: TransitionContext + PrepareData + CollectViz + Send + Sync,
{
    type State = S;

    async fn run(
        &self,
        execution_context: &ExecutionContext,
        mut current_state: Self::State,
    ) -> Result<Self::State, OxyError> {
        let viz_context = execution_context
            .with_child_source(uuid::Uuid::new_v4().to_string(), "visualize".to_string());
        let (viz, tool_call) = self.run_with_retry(&viz_context, &current_state).await?;
        viz_context
            .write_kind(EventKind::VizGenerated { viz: viz.clone() })
            .await?;
        current_state.add_tool_call(&self.objective, tool_call, viz.to_string());
        current_state.collect_viz(viz);
        Ok(current_state)
    }
}
