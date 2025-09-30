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
        control::TransitionContext,
        query::config::{Query, SQLParams},
    },
    errors::OxyError,
    execute::{Executable, ExecutionContext, builders::fsm::Trigger, types::Table},
    semantic::SemanticManager,
    tools::{SQLExecutable, types::SQLInput},
};

pub trait PrepareData {
    fn get_tables(&self) -> &[Table];
    fn add_table(&mut self, table: Table);
}

pub trait PrepareDataDelegator {
    fn target(&self) -> &dyn PrepareData;
    fn target_mut(&mut self) -> &mut dyn PrepareData;
}

impl<T> PrepareData for T
where
    T: PrepareDataDelegator,
{
    fn add_table(&mut self, table: Table) {
        self.target_mut().add_table(table)
    }

    fn get_tables(&self) -> &[Table] {
        self.target().get_tables()
    }
}

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
        messages: Vec<ChatCompletionRequestMessage>,
    ) -> Result<ChatCompletionMessageToolCall, OxyError> {
        let tool_calls = self
            .openai_adapter
            .request_tool_call(
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
                    name: Some(sql_params.title.to_string()),
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
                .request_sql_tool_call(vec![instructions.clone(), failed_messages.clone()].concat())
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
                            tool_calls: Some(vec![tool_call.clone()]),
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
impl<S> Trigger for AutoSQL<S>
where
    S: PrepareData + TransitionContext + Send + Sync,
{
    type State = S;

    async fn run(
        &self,
        execution_context: &ExecutionContext,
        mut state: Self::State,
    ) -> Result<Self::State, OxyError> {
        let (table, tool_call) = self.run_with_retry(execution_context).await?;
        state.add_tool_call(&self.objective, tool_call, (&table).to_string());
        state.add_table(table);
        Ok(state)
    }
}
