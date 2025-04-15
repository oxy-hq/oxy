pub mod types;
use crate::{
    config::{ConfigManager, constants::AGENT_SOURCE, model::AgentConfig},
    errors::OxyError,
    execute::{
        Executable, ExecutionContext, ExecutionContextBuilder,
        builders::{ExecutableBuilder, map::ParamMapper},
        renderer::{Renderer, TemplateRegister},
        types::{Output, Source},
        writer::{BufWriter, EventHandler},
    },
    tools::ToolsContext,
};
pub use builders::{AgentExecutable, OpenAIExecutableResponse, build_openai_executable};
use contexts::Contexts;
use databases::DatabasesContext;
use minijinja::{Value, context};
use std::path::Path;
use types::AgentInput;

mod builders;
mod contexts;
mod databases;

impl TemplateRegister for AgentConfig {
    fn register_template(&self, renderer: &Renderer) -> Result<(), OxyError> {
        renderer.register_template(&self.system_instructions)?;
        Ok(())
    }
}

pub struct AgentLauncher {
    execution_context: Option<ExecutionContext>,
    buf_writer: BufWriter,
}

impl AgentLauncher {
    pub fn new() -> Self {
        Self {
            execution_context: None,
            buf_writer: BufWriter::new(),
        }
    }

    pub fn with_external_context(
        mut self,
        execution_context: &ExecutionContext,
    ) -> Result<Self, OxyError> {
        self.execution_context =
            Some(execution_context.wrap_writer(self.buf_writer.create_writer(None)?));
        Ok(self)
    }

    pub async fn with_local_context<P: AsRef<Path>>(
        mut self,
        project_path: P,
    ) -> Result<Self, OxyError> {
        let tx = self.buf_writer.create_writer(None)?;
        self.execution_context = Some(
            ExecutionContextBuilder::new()
                .with_project_path(project_path)
                .await?
                .with_global_context(Value::UNDEFINED)
                .with_writer(tx)
                .with_source(Source {
                    parent_id: None,
                    id: AGENT_SOURCE.to_string(),
                    kind: AGENT_SOURCE.to_string(),
                })
                .build()?,
        );
        Ok(self)
    }

    pub async fn launch<H: EventHandler + Send + 'static>(
        self,
        agent_input: AgentInput,
        event_handler: H,
    ) -> Result<Output, OxyError> {
        let execution_context = self
            .execution_context
            .ok_or(OxyError::RuntimeError(
                "ExecutionContext is required".to_string(),
            ))?
            .with_child_source(agent_input.agent_ref.to_string(), AGENT_SOURCE.to_string());
        let mut agent_executable = build_agent_executable();

        let handle = tokio::spawn(async move {
            agent_executable
                .execute(&execution_context, agent_input)
                .await
        });

        let buf_writer = self.buf_writer;
        let event_handle =
            tokio::spawn(async move { buf_writer.write_to_handler(event_handler).await });
        let response = handle.await?;
        event_handle.await??;
        response
    }
}

#[derive(Clone, Debug)]
pub struct AgentLauncherExecutable;

#[async_trait::async_trait]
impl Executable<AgentInput> for AgentLauncherExecutable {
    type Response = Output;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: AgentInput,
    ) -> Result<Self::Response, OxyError> {
        AgentLauncher::new()
            .with_external_context(execution_context)?
            .launch(input, execution_context.writer.clone())
            .await
    }
}

#[derive(Clone)]
pub struct AgentMapper;

#[async_trait::async_trait]
impl ParamMapper<AgentInput, (AgentConfig, String)> for AgentMapper {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: AgentInput,
    ) -> Result<((AgentConfig, String), Option<ExecutionContext>), OxyError> {
        let AgentInput { agent_ref, prompt } = input;
        let agent_config = execution_context.config.resolve_agent(&agent_ref).await?;
        let global_context =
            build_global_context(&execution_context.config, &agent_config, &prompt);
        let renderer = Renderer::from_template(global_context, &agent_config)?;
        let execution_context = execution_context.wrap_renderer(renderer);
        Ok(((agent_config, prompt), Some(execution_context)))
    }
}

fn build_agent_executable() -> impl Executable<AgentInput, Response = Output> {
    ExecutableBuilder::new()
        .map(AgentMapper)
        .executable(AgentExecutable)
}

fn build_global_context(config: &ConfigManager, agent_config: &AgentConfig, prompt: &str) -> Value {
    let contexts = Contexts::new(
        agent_config.context.clone().unwrap_or_default(),
        config.clone(),
    );
    let databases = DatabasesContext::new(config.clone());
    let tools = ToolsContext::new(
        config.clone(),
        agent_config.name.to_string(),
        agent_config.tools_config.tools.clone(),
        prompt.to_string(),
    );
    context! {
        context => Value::from_object(contexts),
        databases => Value::from_object(databases),
        tools => Value::from_object(tools)
    }
}
