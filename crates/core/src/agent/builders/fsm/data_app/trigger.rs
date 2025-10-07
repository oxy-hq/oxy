use async_openai::types::{
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestAssistantMessageContent,
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestSystemMessageContent,
};
use itertools::Itertools;
use short_uuid::ShortUuid;

use crate::{
    adapters::openai::OpenAIAdapter,
    agent::builders::fsm::{
        control::TransitionContext,
        data_app::{CollectInsights, config::Insight},
        query::PrepareData,
        viz::CollectViz,
    },
    config::model::{
        AppConfig, Display, ExecuteSQLTask, MarkdownDisplay, SQL, TableDisplay, Task, TaskType,
    },
    errors::OxyError,
    execute::{Executable, ExecutionContext, builders::fsm::Trigger},
    tools::{
        create_data_app::{CreateDataAppExecutable, types::CreateDataAppParams},
        types::CreateDataAppInput,
    },
};

pub struct BuildDataApp<S> {
    objective: String,
    _state: std::marker::PhantomData<S>,
}

impl<S> BuildDataApp<S> {
    pub fn new(objective: String) -> Self {
        Self {
            objective,
            _state: std::marker::PhantomData,
        }
    }
}

#[async_trait::async_trait]
impl<S> Trigger for BuildDataApp<S>
where
    S: PrepareData + TransitionContext + CollectViz + CollectInsights + Send + Sync,
{
    type State = S;

    async fn run(
        &self,
        execution_context: &ExecutionContext,
        mut current_state: Self::State,
    ) -> Result<Self::State, OxyError> {
        let tasks = current_state
            .get_tables()
            .iter()
            .map(|t| {
                let table_ref = t.reference.clone().ok_or(OxyError::RuntimeError(
                    "Table must have a reference to build data app.".to_string(),
                ))?;
                Result::<Task, OxyError>::Ok(Task {
                    name: t
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("table_{}", ShortUuid::generate())),
                    cache: None,
                    task_type: TaskType::ExecuteSQL(ExecuteSQLTask {
                        database: table_ref.database_ref,
                        sql: SQL::Query {
                            sql_query: table_ref.sql,
                        },
                        variables: None,
                        dry_run_limit: None,
                        export: None,
                    }),
                })
            })
            .try_collect::<Task, Vec<_>, OxyError>()?;
        let table_displays = tasks
            .iter()
            .map(|t| {
                Display::Table(TableDisplay {
                    title: Some(t.name.clone()),
                    data: t.name.clone(),
                })
            })
            .collect::<Vec<_>>();
        let viz_displays = current_state
            .list_viz()
            .iter()
            .map(|v| v.clone().into())
            .collect::<Vec<Display>>();
        let markdown_display = current_state
            .get_insights()
            .iter()
            .map(|insight| {
                Display::Markdown(MarkdownDisplay {
                    content: insight.clone(),
                })
            })
            .collect::<Vec<_>>();
        let app_config = AppConfig {
            display: [markdown_display, viz_displays, table_displays].concat(),
            tasks,
        };
        let file_name = format!("data_app_{}", ShortUuid::generate());
        let mut executable = CreateDataAppExecutable;
        let response = executable
            .execute(
                execution_context,
                CreateDataAppInput {
                    param: CreateDataAppParams {
                        file_name,
                        app_config,
                    },
                },
            )
            .await?;
        current_state.add_message(response.to_string());
        Ok(current_state)
    }
}

pub struct GenerateInsight<S> {
    objective: String,
    config: Insight,
    adapter: OpenAIAdapter,
    _state: std::marker::PhantomData<S>,
}

impl<S> GenerateInsight<S>
where
    S: PrepareData,
{
    pub fn new(adapter: OpenAIAdapter, objective: String, config: Insight) -> Self {
        Self {
            adapter,
            objective,
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

    async fn request_insight(
        &self,
        messages: Vec<ChatCompletionRequestMessage>,
    ) -> Result<String, OxyError> {
        let response = self.adapter.generate_text(messages).await?;
        Ok(response)
    }

    async fn run_insight(
        &self,
        execution_context: &ExecutionContext,
        current_state: &S,
    ) -> Result<String, OxyError> {
        let instructions = self
            .prepare_instructions(execution_context, current_state)
            .await?;
        let insight = self.request_insight(instructions).await?;
        Ok(insight)
    }
}

#[async_trait::async_trait]
impl<S> Trigger for GenerateInsight<S>
where
    S: PrepareData + TransitionContext + CollectViz + CollectInsights + Send + Sync,
{
    type State = S;
    async fn run(
        &self,
        execution_context: &ExecutionContext,
        mut current_state: Self::State,
    ) -> Result<Self::State, OxyError> {
        let content = self.run_insight(execution_context, &current_state).await?;
        current_state.collect_insight(content.clone());
        current_state.add_message(content);
        Ok(current_state)
    }
}
