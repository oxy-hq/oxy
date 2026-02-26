use async_openai::types::chat::{
    ChatCompletionMessageToolCall, ChatCompletionMessageToolCalls,
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestAssistantMessageContent,
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestSystemMessageContent, ChatCompletionRequestToolMessage,
    ChatCompletionRequestToolMessageContent, ChatCompletionToolChoiceOption, ToolChoiceOptions,
};

use crate::fsm::{
    semantic_query::config::SemanticQuery, state::MachineContext, types::TableSource,
};
use oxy::adapters::openai::OpenAIAdapter;
use oxy::adapters::semantic_tool_description::get_semantic_query_description;
use oxy::config::model::{SemanticQueryTask, SemanticQueryTool, ToolType};
use oxy::execute::{ExecutionContext, builders::fsm::Trigger, types::Table};
use oxy::tools::{ToolRawInput, global_registry};
use oxy::types::SemanticQueryParams;
use oxy_shared::errors::OxyError;

pub struct AutoSemanticQuery<S> {
    openai_adapter: OpenAIAdapter,
    config: SemanticQuery,
    objective: String,
    _state: std::marker::PhantomData<S>,
}

impl<S> AutoSemanticQuery<S> {
    pub fn new(openai_adapter: OpenAIAdapter, config: SemanticQuery, objective: String) -> Self {
        Self {
            openai_adapter,
            config,
            objective,
            _state: std::marker::PhantomData,
        }
    }

    fn build_semantic_tool(&self) -> SemanticQueryTool {
        SemanticQueryTool {
            name: self.config.name.clone(),
            description: self.config.description.clone(),
            dry_run_limit: None,
            topic: Some(self.config.topic.clone()),
            variables: self.config.variables.clone(),
        }
    }

    async fn prepare_instructions(
        &self,
        execution_context: &ExecutionContext,
    ) -> Result<Vec<ChatCompletionRequestMessage>, OxyError> {
        tracing::info!("SemanticQuery Objective: {}", self.objective);

        // Build a SemanticQueryTool to get the enriched description with semantic metadata
        let semantic_tool = self.build_semantic_tool();

        let semantic_description = get_semantic_query_description(
            &semantic_tool,
            &execution_context.project.config_manager,
        )?;

        let instruction = execution_context
            .renderer
            .render_async(&self.config.instruction)
            .await?;

        let messages = vec![
            ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(instruction),
                ..Default::default()
            }
            .into(),
            ChatCompletionRequestSystemMessage {
                name: None,
                content: ChatCompletionRequestSystemMessageContent::Text(format!(
                    "You have access to the following semantic layer:\n{}",
                    semantic_description
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

    async fn request_tool_call(
        &self,
        execution_context: &ExecutionContext,
        messages: Vec<ChatCompletionRequestMessage>,
    ) -> Result<ChatCompletionMessageToolCall, OxyError> {
        let (_content, tool_calls) = self
            .openai_adapter
            .request_tool_call_with_usage(
                execution_context,
                messages,
                vec![self.config.get_tool()],
                Some(ChatCompletionToolChoiceOption::Mode(
                    ToolChoiceOptions::Required,
                )),
                None,
            )
            .await?;
        let tool_call = tool_calls.first().ok_or_else(|| {
            OxyError::RuntimeError("No tool call returned from OpenAI".to_string())
        })?;
        Ok(tool_call.clone())
    }

    async fn execute_semantic_query(
        &self,
        execution_context: &ExecutionContext,
        tool_call: &ChatCompletionMessageToolCall,
    ) -> Result<Table, OxyError> {
        // Build the ToolType for the global registry
        let tool_type = ToolType::SemanticQuery(self.build_semantic_tool());

        // Build the raw input from the LLM's tool call arguments
        let input = ToolRawInput::from(tool_call);

        // Execute via the global tool registry (which delegates to SemanticQueryToolExecutor)
        let result = global_registry()
            .execute(execution_context, &tool_type, &input)
            .await?;

        let output_container = result.ok_or_else(|| {
            OxyError::RuntimeError(
                "SemanticQuery execution not available: No executor registered.".to_string(),
            )
        })?;

        // Extract Table from the OutputContainer::Single(Output::Table(...))
        match output_container {
            oxy::execute::types::OutputContainer::Single(oxy::execute::types::Output::Table(
                table,
            )) => Ok(table),
            other => Err(OxyError::RuntimeError(format!(
                "Semantic query returned unexpected output type: {:?}",
                other
            ))),
        }
    }

    async fn run_with_retry(
        &self,
        execution_context: &ExecutionContext,
    ) -> Result<(Table, ChatCompletionMessageToolCall), OxyError> {
        let instructions = self.prepare_instructions(execution_context).await?;
        let max_retries = self.config.max_retries;
        let mut failed_messages = vec![];

        loop {
            let tool_call = self
                .request_tool_call(
                    execution_context,
                    [instructions.clone(), failed_messages.clone()].concat(),
                )
                .await?;
            match self
                .execute_semantic_query(execution_context, &tool_call)
                .await
            {
                Ok(table) => return Ok((table, tool_call)),
                Err(e) => {
                    if failed_messages.len() as u32 / 2 >= max_retries {
                        return Err(OxyError::RuntimeError(format!(
                            "Semantic query failed after {max_retries} retries: {e}",
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
                                "The previous semantic query failed with error: {}. Please try again.",
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
impl Trigger for AutoSemanticQuery<MachineContext> {
    type State = MachineContext;

    async fn run(
        &self,
        execution_context: &ExecutionContext,
        state: &mut Self::State,
    ) -> Result<(), OxyError> {
        let query_context = execution_context.with_child_source(
            uuid::Uuid::new_v4().to_string(),
            "semantic_query".to_string(),
        );
        tracing::info!(
            "Running AutoSemanticQuery Trigger for objective: {}",
            self.objective
        );
        let (table, tool_call) = self.run_with_retry(&query_context).await?;
        tracing::info!("AutoSemanticQuery Tool Call: {:?}", tool_call);
        // Build the SemanticQueryTask from the LLM's tool call args so we can
        // reconstruct an executable task when saving an automation.
        let source = serde_json::from_str::<SemanticQueryParams>(&tool_call.function.arguments)
            .map(|mut params| {
                // Topic may not be in the tool call args; inject from config.
                if params.topic.is_none() {
                    params.topic = Some(self.config.topic.clone());
                }
                TableSource::Semantic {
                    task: SemanticQueryTask {
                        query: params,
                        export: None,
                        variables: self.config.variables.clone(),
                    },
                }
            })
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to parse SemanticQueryParams from tool call: {}", e);
                TableSource::Semantic {
                    task: SemanticQueryTask {
                        query: SemanticQueryParams {
                            topic: Some(self.config.topic.clone()),
                            ..Default::default()
                        },
                        export: None,
                        variables: self.config.variables.clone(),
                    },
                }
            });
        state.add_table(self.objective.clone(), tool_call.into(), table, source);
        Ok(())
    }
}
