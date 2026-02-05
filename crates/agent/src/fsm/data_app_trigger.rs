use async_openai::types::chat::{
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestAssistantMessageContent,
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestSystemMessageContent,
};
use futures::StreamExt;
use itertools::Itertools;
use short_uuid::ShortUuid;

use crate::fsm::{data_app::config::Insight, state::MachineContext};
use oxy::adapters::openai::OpenAIAdapter;
use oxy::config::model::{
    AppConfig, Display, ExecuteSQLTask, MarkdownDisplay, SQL, TableDisplay, Task, TaskType,
};
use oxy::execute::{
    Executable, ExecutionContext,
    builders::fsm::Trigger,
    types::{Chunk, Output, VizParams, VizParamsType},
};
use oxy::tools::create_data_app::{
    CreateDataAppExecutable, CreateDataAppInput, CreateDataAppParams,
};
use oxy_shared::errors::OxyError;

fn viz_params_to_display(viz: VizParams) -> Display {
    match viz.config {
        VizParamsType::Bar(bar) => Display::BarChart(bar),
        VizParamsType::Line(line) => Display::LineChart(line),
        VizParamsType::Pie(pie) => Display::PieChart(pie),
    }
}

pub struct BuildDataApp<S> {
    #[allow(dead_code)]
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

pub struct GenerateInsight<S> {
    objective: String,
    config: Insight,
    adapter: OpenAIAdapter,
    _state: std::marker::PhantomData<S>,
}

impl<S> GenerateInsight<S> {
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
        insight_context: String,
    ) -> Result<Vec<ChatCompletionRequestMessage>, OxyError> {
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
                content: ChatCompletionRequestSystemMessageContent::Text(insight_context),
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

    async fn run_insight(
        &self,
        execution_context: &ExecutionContext,
        insight_context: String,
    ) -> Result<impl tokio_stream::Stream<Item = Result<Option<String>, OxyError>>, OxyError> {
        let instructions = self
            .prepare_instructions(execution_context, insight_context)
            .await?;
        self.adapter.stream_text(instructions).await
    }
}

#[async_trait::async_trait]
impl Trigger for GenerateInsight<MachineContext> {
    type State = MachineContext;

    async fn run(
        &self,
        execution_context: &ExecutionContext,
        current_state: &mut Self::State,
    ) -> Result<(), OxyError> {
        let insight_context = execution_context
            .with_child_source(uuid::Uuid::new_v4().to_string(), "insight".to_string());
        let instruction_context = format!(
            "You have access to the following artifacts:\n{}",
            current_state.artifacts_context()
        );
        let mut stream = self
            .run_insight(&insight_context, instruction_context)
            .await?;
        let mut content = String::new();
        while let Some(chunk) = stream.next().await.transpose()?.flatten() {
            content.push_str(&chunk);
            insight_context
                .write_chunk(Chunk {
                    key: None,
                    delta: Output::Text(chunk),
                    finished: false,
                })
                .await?;
        }
        insight_context
            .write_chunk(Chunk {
                key: None,
                delta: Output::Text("".to_string()),
                finished: true,
            })
            .await?;
        current_state.add_insight(content.clone());
        Ok(())
    }
}

#[async_trait::async_trait]
impl Trigger for BuildDataApp<MachineContext> {
    type State = MachineContext;

    async fn run(
        &self,
        execution_context: &ExecutionContext,
        current_state: &mut Self::State,
    ) -> Result<(), OxyError> {
        let tasks = current_state
            .list_tables()
            .iter()
            .map(|t| {
                let table_ref = t.reference.clone().ok_or(OxyError::RuntimeError(
                    "Table must have a reference to build data app.".to_string(),
                ))?;
                Result::<Task, OxyError>::Ok(Task {
                    name: t.name.clone(),
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
            .map(|v| viz_params_to_display((*v).clone()))
            .collect::<Vec<Display>>();
        let markdown_display = current_state
            .list_insights()
            .iter()
            .map(|insight| {
                Display::Markdown(MarkdownDisplay {
                    content: insight.to_string(),
                })
            })
            .collect::<Vec<_>>();
        let app_config = AppConfig {
            display: [markdown_display, viz_displays, table_displays].concat(),
            tasks,
            ..Default::default()
        };
        let file_name = format!("data_app_{}", ShortUuid::generate());
        let mut executable = CreateDataAppExecutable;
        let build_app_context = execution_context
            .with_child_source(uuid::Uuid::new_v4().to_string(), "data_app".to_string());
        let response = executable
            .execute(
                &build_app_context,
                CreateDataAppInput {
                    param: CreateDataAppParams {
                        file_name,
                        app_config,
                    },
                },
            )
            .await?;
        current_state.add_message(response.to_string());
        Ok(())
    }
}
