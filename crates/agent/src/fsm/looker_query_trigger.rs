use async_openai::types::chat::{
    ChatCompletionMessageToolCall, ChatCompletionMessageToolCalls,
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestAssistantMessageContent,
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestSystemMessageContent, ChatCompletionRequestToolMessage,
    ChatCompletionRequestToolMessageContent, ChatCompletionToolChoiceOption, ToolChoiceOptions,
};

use crate::fsm::{looker_query::config::LookerQuery, state::MachineContext, types::TableSource};
use oxy::adapters::looker_tool_description::get_looker_query_description;
use oxy::adapters::openai::OpenAIAdapter;
use oxy::config::model::{LookerQueryParams, LookerQueryTask, LookerQueryTool};
use oxy::execute::{Executable, ExecutionContext, builders::fsm::Trigger, types::Table};
use oxy::tools::looker::{executable::LookerQueryExecutable, types::LookerQueryInput};
use oxy_shared::errors::OxyError;

pub struct AutoLookerQuery<S> {
    openai_adapter: OpenAIAdapter,
    config: LookerQuery,
    objective: String,
    _state: std::marker::PhantomData<S>,
}

impl<S> AutoLookerQuery<S> {
    pub fn new(openai_adapter: OpenAIAdapter, config: LookerQuery, objective: String) -> Self {
        Self {
            openai_adapter,
            config,
            objective,
            _state: std::marker::PhantomData,
        }
    }

    fn build_looker_tool(&self) -> LookerQueryTool {
        LookerQueryTool {
            name: self.config.name.clone(),
            description: self.config.description.clone(),
            integration: self.config.integration.clone(),
            model: self.config.model.clone(),
            explore: self.config.explore.clone(),
        }
    }

    async fn prepare_instructions(
        &self,
        execution_context: &ExecutionContext,
    ) -> Result<Vec<ChatCompletionRequestMessage>, OxyError> {
        tracing::info!("LookerQuery Objective: {}", self.objective);

        let instruction = execution_context
            .renderer
            .render_async(&self.config.instruction)
            .await?;

        let looker_tool = self.build_looker_tool();
        let looker_description = match get_looker_query_description(
            &looker_tool,
            &execution_context.project.config_manager,
        )
        .await
        {
            Ok(description) => description,
            Err(e) => {
                tracing::warn!("Failed to enrich Looker context for prompt: {}", e);
                format!(
                    "You are querying Looker integration '{}' using model '{}' and explore '{}'. Return valid query params for this explore.",
                    self.config.integration, self.config.model, self.config.explore,
                )
            }
        };

        let messages = vec![
            ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(instruction),
                ..Default::default()
            }
            .into(),
            ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(format!(
                    "You have access to the following Looker explore metadata:\n{}",
                    looker_description
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

    async fn execute_looker_query(
        &self,
        execution_context: &ExecutionContext,
        tool_call: &ChatCompletionMessageToolCall,
    ) -> Result<Table, OxyError> {
        let params: LookerQueryParams = serde_json::from_str(&tool_call.function.arguments)
            .map_err(|e| {
                OxyError::SerializerError(format!("Failed to parse Looker query params: {e}"))
            })?;

        let mut executable = LookerQueryExecutable::new();
        let output = Executable::execute(
            &mut executable,
            execution_context,
            LookerQueryInput {
                params,
                integration: self.config.integration.clone(),
                model: self.config.model.clone(),
                explore: self.config.explore.clone(),
            },
        )
        .await?;

        match output {
            oxy::execute::types::Output::Table(table) => Ok(table),
            other => Err(OxyError::RuntimeError(format!(
                "Looker query returned unexpected output type: {:?}",
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
                .execute_looker_query(execution_context, &tool_call)
                .await
            {
                Ok(table) => return Ok((table, tool_call)),
                Err(e) => {
                    if failed_messages.len() as u32 / 2 >= max_retries {
                        return Err(OxyError::RuntimeError(format!(
                            "Looker query failed after {max_retries} retries: {e}",
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
                                "The previous Looker query failed with error: {}. Please try again.",
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
impl Trigger for AutoLookerQuery<MachineContext> {
    type State = MachineContext;

    async fn run(
        &self,
        execution_context: &ExecutionContext,
        state: &mut Self::State,
    ) -> Result<(), OxyError> {
        let query_context = execution_context
            .with_child_source(uuid::Uuid::new_v4().to_string(), "looker_query".to_string());

        tracing::info!(
            "Running AutoLookerQuery Trigger for objective: {}",
            self.objective
        );
        let (table, tool_call) = self.run_with_retry(&query_context).await?;
        tracing::info!("AutoLookerQuery Tool Call: {:?}", tool_call);

        let source = serde_json::from_str::<LookerQueryParams>(&tool_call.function.arguments)
            .map(|params| TableSource::Looker {
                task: LookerQueryTask {
                    integration: self.config.integration.clone(),
                    model: self.config.model.clone(),
                    explore: self.config.explore.clone(),
                    query: params,
                    export: None,
                },
            })
            .unwrap_or_else(|_| TableSource::Looker {
                task: LookerQueryTask {
                    integration: self.config.integration.clone(),
                    model: self.config.model.clone(),
                    explore: self.config.explore.clone(),
                    query: LookerQueryParams {
                        fields: vec![],
                        filters: None,
                        filter_expression: None,
                        sorts: None,
                        limit: None,
                    },
                    export: None,
                },
            });

        state.add_table(self.objective.clone(), tool_call.into(), table, source);
        Ok(())
    }
}
