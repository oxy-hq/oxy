use crate::{
    config::{
        constants::AGENT_SOURCE,
        model::{AgentConfig, AgentType},
    },
    errors::OxyError,
    execute::{
        Executable, ExecutionContext, ExecutionContextBuilder,
        renderer::{Renderer, TemplateRegister},
        types::{OutputContainer, Source},
        writer::{BufWriter, EventHandler},
    },
};
use builders::AgentExecutable;
pub use builders::{OneShotInput, OpenAIExecutableResponse, build_openai_executable};
use minijinja::Value;
pub use references::AgentReferencesHandler;
use std::path::Path;
use types::AgentInput;

mod builders;
mod contexts;
mod databases;
mod references;
pub mod types;

impl TemplateRegister for AgentConfig {
    fn register_template(&self, renderer: &Renderer) -> Result<(), OxyError> {
        match &self.r#type {
            AgentType::Default(default_agent) => {
                renderer.register_template(&default_agent.system_instructions)?;
            }
            _ => {}
        }
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
    ) -> Result<OutputContainer, OxyError> {
        let execution_context = self
            .execution_context
            .ok_or(OxyError::RuntimeError(
                "ExecutionContext is required".to_string(),
            ))?
            .with_child_source(agent_input.agent_ref.to_string(), AGENT_SOURCE.to_string());
        let handle = tokio::spawn(async move {
            AgentExecutable
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
    type Response = OutputContainer;

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
