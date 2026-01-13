use async_openai::types::chat::{
    ChatCompletionMessageToolCall, ChatCompletionMessageToolCalls,
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestAssistantMessageContent,
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestSystemMessageContent, ChatCompletionRequestToolMessage,
    ChatCompletionRequestToolMessageContent, ChatCompletionToolChoiceOption, ToolChoiceOptions,
};

use crate::{
    adapters::openai::OpenAIAdapter,
    agent::builders::fsm::{
        query::config::{Query, SQLParams},
        state::MachineContext,
    },
    errors::OxyError,
    execute::{Executable, ExecutionContext, builders::fsm::Trigger, types::Table},
    semantic::SemanticManager,
    tools::{SQLExecutable, types::SQLInput},
};

pub struct AutoSQL<S> {
    openai_adapter: OpenAIAdapter,
    config: Query,
    objective: String,
    _state: std::marker::PhantomData<S>,
}

impl<S> AutoSQL<S> {
    pub fn new(openai_adapter: OpenAIAdapter, config: Query, objective: String) -> Self {
        Self {
            openai_adapter,
            config,
            objective,
            _state: std::marker::PhantomData,
        }
    }

    async fn prepare_instructions(
        &self,
        execution_context: &ExecutionContext,
    ) -> Result<Vec<ChatCompletionRequestMessage>, OxyError> {
        tracing::info!("Query Objective: {}", self.objective);
        let semantic_manager = SemanticManager::from_config(
            execution_context.project.config_manager.clone(),
            execution_context.project.secrets_manager.clone(),
            false,
        )
        .await?;
        let database_info = semantic_manager
            .load_database_info(&self.config.database)
            .await?;
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
                    "You have access to the following database:\n{}",
                    serde_json::to_string_pretty(&database_info)?
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

    async fn request_sql_tool_call(
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

    async fn execute_query(
        &self,
        execution_context: &ExecutionContext,
        database: &str,
        tool_call: &ChatCompletionMessageToolCall,
    ) -> Result<Table, OxyError> {
        let mut executable = SQLExecutable::new();
        let sql_params: SQLParams = serde_json::from_str(&tool_call.function.arguments)
            .map_err(|e| OxyError::SerializerError(format!("Failed to parse SQL params: {e}")))?;
        let response = executable
            .execute(
                execution_context,
                SQLInput {
                    sql: sql_params.sql.to_string(),
                    database: database.to_string(),
                    dry_run_limit: None,
                    name: Some(slugify::slugify(&sql_params.title, "", "_", None)),
                    persist: false,
                },
            )
            .await?;
        Ok(response)
    }

    async fn run_with_retry(
        &self,
        execution_context: &ExecutionContext,
    ) -> Result<(Table, ChatCompletionMessageToolCall), OxyError> {
        let instructions = self.prepare_instructions(execution_context).await?;
        let config = &self.config;
        let max_retries = config.max_retries;
        let mut failed_messages = vec![];

        loop {
            let tool_call = self
                .request_sql_tool_call(
                    execution_context,
                    [instructions.clone(), failed_messages.clone()].concat(),
                )
                .await?;
            match self
                .execute_query(execution_context, &config.database, &tool_call)
                .await
            {
                Ok(table) => return Ok((table, tool_call)),
                Err(e) => {
                    if failed_messages.len() as u32 / 2 >= max_retries {
                        return Err(OxyError::RuntimeError(format!(
                            "Query failed after {max_retries} retries: {e}",
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
                                "The previous query failed with error: {}. Please try again.",
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
impl Trigger for AutoSQL<MachineContext> {
    type State = MachineContext;

    async fn run(
        &self,
        execution_context: &ExecutionContext,
        state: &mut Self::State,
    ) -> Result<(), OxyError> {
        let query_context = execution_context
            .with_child_source(uuid::Uuid::new_v4().to_string(), "query".to_string());
        tracing::info!("Running AutoSQL Trigger for objective: {}", self.objective);
        let (table, tool_call) = self.run_with_retry(&query_context).await?;
        tracing::info!("AutoSQL Tool Call: {:?}", tool_call);
        state.add_table(self.objective.clone(), tool_call.into(), table);
        Ok(())
    }
}
