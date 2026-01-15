use async_openai::types::chat::{
    ChatCompletionMessageToolCall, ChatCompletionMessageToolCalls,
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestAssistantMessageContent,
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestSystemMessageContent, ChatCompletionRequestToolMessage,
    ChatCompletionRequestToolMessageContent, ChatCompletionToolChoiceOption, ToolChoiceOptions,
};

use crate::fsm::{
    state::MachineContext,
    viz::{
        config::Visualize,
        recommendations::{
            ChartHeuristicsAnalyzerBuilder, ChartResponseParser, ChartSelectionSchema,
        },
    },
};
use oxy::adapters::openai::OpenAIAdapter;
use oxy::execute::{
    ExecutionContext,
    builders::fsm::Trigger,
    types::{EventKind, Table, VizParams},
};
use oxy_shared::errors::OxyError;

pub struct GenerateViz<S> {
    objective: String,
    adapter: OpenAIAdapter,
    config: Visualize,
    _state: std::marker::PhantomData<S>,
}

impl<S> GenerateViz<S> {
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
    ) -> Result<Vec<ChatCompletionRequestMessage>, OxyError> {
        let instruction = execution_context
            .renderer
            .render_async(&self.config.instruction)
            .await?;
        let messages = vec![
            ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(format!(
                    "##Instruction: {}\n",
                    instruction
                )),
                ..Default::default()
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
        tables: &[&Table],
        messages: Vec<ChatCompletionRequestMessage>,
    ) -> Result<ChatCompletionMessageToolCall, OxyError> {
        // Use heuristic recommendations to guide the visualization tool call
        let recommendations = ChartHeuristicsAnalyzerBuilder::default()
            .with_all_defaults()
            .build()
            .top_recommendations(tables, 50);
        if recommendations.is_empty() {
            return Err(OxyError::RuntimeError(format!(
                "No chart recommendations could be generated from the following tables:\n\n
{}
\n\n
Please pay attention to column types and data distributions when generating visualizations.",
                tables
                    .iter()
                    .map(|t| t.summary_yml())
                    .collect::<Vec<String>>()
                    .join("\n---\n")
            )));
        }
        // Use the recommendations to build a function call
        let tool_call = ChartSelectionSchema::build(recommendations.as_slice());

        let (content, tool_calls) = self
            .adapter
            .request_tool_call_with_usage(
                execution_context,
                messages,
                vec![tool_call],
                Some(ChatCompletionToolChoiceOption::Mode(
                    ToolChoiceOptions::Auto,
                )),
                None,
            )
            .await?;
        let tool_call = tool_calls.first().ok_or_else(|| {
            OxyError::RuntimeError(format!(
                "No tool call returned from OpenAI. {}",
                content.unwrap_or_default()
            ))
        })?;
        Ok(tool_call.clone())
    }

    async fn validate_viz(
        &self,
        tool_call: &ChatCompletionMessageToolCall,
    ) -> Result<VizParams, OxyError> {
        ChartResponseParser::parse(&tool_call.function.arguments).map(|c| c.into())
    }

    async fn run_with_retry(
        &self,
        execution_context: &ExecutionContext,
        tables: &[&Table],
    ) -> Result<(VizParams, ChatCompletionMessageToolCall), OxyError> {
        let instructions = self.prepare_instructions(execution_context).await?;
        let config = &self.config;
        let max_retries = config.max_retries;
        let mut failed_messages = vec![];

        loop {
            let tool_call = self
                .request_viz_tool_call(
                    execution_context,
                    tables,
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
                            tool_calls: Some(vec![ChatCompletionMessageToolCalls::Function(
                                tool_call.clone(),
                            )]),
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

#[async_trait::async_trait]
impl Trigger for GenerateViz<MachineContext> {
    type State = MachineContext;

    async fn run(
        &self,
        execution_context: &ExecutionContext,
        current_state: &mut Self::State,
    ) -> Result<(), OxyError> {
        let viz_context = execution_context
            .with_child_source(uuid::Uuid::new_v4().to_string(), "visualize".to_string());
        let (viz, tool_call) = self
            .run_with_retry(&viz_context, current_state.list_tables().as_slice())
            .await?;
        let persisted_viz_data =
            current_state.add_viz(self.objective.clone(), tool_call.into(), viz)?;
        viz_context
            .write_kind(EventKind::VizGenerated {
                viz: persisted_viz_data,
            })
            .await?;

        Ok(())
    }
}
