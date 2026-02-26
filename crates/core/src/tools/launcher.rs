use minijinja::Value;
use tracing::Instrument;

use crate::{
    adapters::project::manager::ProjectManager,
    config::{constants::TOOL_SOURCE, model::ToolType},
    execute::{
        Executable, ExecutionContext, ExecutionContextBuilder,
        types::{OutputContainer, Source},
        writer::{BufWriter, EventHandler},
    },
    observability::events,
};
use oxy_shared::errors::OxyError;

use super::types::ToolRawInput;

pub struct ToolInput {
    pub agent_name: String,
    pub raw: ToolRawInput,
    pub tools: Vec<ToolType>,
}

pub struct ToolLauncher {
    execution_context: Option<ExecutionContext>,
    buf_writer: BufWriter,
}

impl Default for ToolLauncher {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolLauncher {
    pub fn new() -> Self {
        ToolLauncher {
            execution_context: None,
            buf_writer: BufWriter::new(),
        }
    }

    pub fn with_project(
        mut self,
        project_manager: ProjectManager,
        source: Option<Source>,
    ) -> Result<Self, OxyError> {
        let source = source.unwrap_or(Source {
            parent_id: None,
            id: TOOL_SOURCE.to_string(),
            kind: TOOL_SOURCE.to_string(),
        });
        self.execution_context = Some(
            ExecutionContextBuilder::new()
                .with_project_manager(project_manager)
                .with_source(source)
                .with_writer(self.buf_writer.create_writer(None)?)
                .with_global_context(Value::UNDEFINED)
                .build()?,
        );
        Ok(self)
    }

    pub fn with_external_context(
        mut self,
        execution_context: &ExecutionContext,
    ) -> Result<Self, OxyError> {
        self.execution_context =
            Some(execution_context.wrap_writer(self.buf_writer.create_writer(None)?));
        Ok(self)
    }

    pub async fn launch<H: EventHandler + Send + 'static>(
        self,
        tool_input: ToolInput,
        event_handler: H,
    ) -> Result<OutputContainer, OxyError> {
        let execution_context = self.execution_context.ok_or(OxyError::RuntimeError(
            "ExecutionContext is required".to_string(),
        ))?;
        let mut executable = build_tool_executable();

        // Capture the current span to propagate trace context to the spawned task
        let current_span = tracing::Span::current();

        // Find the matching tool type from the tools list
        let tool_name = &tool_input.raw.handle;
        let tool_type = tool_input
            .tools
            .iter()
            .find(|t| {
                let name = match t {
                    ToolType::ExecuteSQL(t) => &t.name,
                    ToolType::ValidateSQL(t) => &t.name,
                    ToolType::Retrieval(t) => &t.name,
                    ToolType::Workflow(t) => &t.name,
                    ToolType::Agent(t) => &t.name,
                    ToolType::Visualize(t) => &t.name,
                    ToolType::CreateDataApp(t) => &t.name,
                    ToolType::EditDataApp(t) => &t.name,
                    ToolType::ReadDataApp(t) => &t.name,
                    ToolType::CreateV0App(t) => &t.name,
                    ToolType::OmniQuery(t) => &t.name,
                    ToolType::SemanticQuery(t) => &t.name,
                    ToolType::SaveAutomation(t) => &t.name,
                };
                name == tool_name
            })
            .cloned();

        let agent_name = tool_input.agent_name.clone();
        let raw_input = tool_input.raw;

        let handle = tokio::spawn(
            async move {
                executable
                    .execute(&execution_context, (agent_name, tool_type, raw_input))
                    .await
            }
            .instrument(current_span),
        );
        let buf_writer = self.buf_writer;
        let event_handle =
            tokio::spawn(async move { buf_writer.write_to_handler(event_handler).await });
        let response = handle.await?;
        event_handle.await??;
        response
    }
}

#[derive(Clone, Debug)]
pub struct ToolLauncherExecutable;

#[async_trait::async_trait]
impl Executable<ToolInput> for ToolLauncherExecutable {
    type Response = OutputContainer;

    #[tracing::instrument(skip_all, err, fields(
        otel.name = events::tool::TOOL_LAUNCHER_EXECUTE,
        oxy.span_type = events::tool::TOOL_CALL_TYPE,
    ))]
    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: ToolInput,
    ) -> Result<Self::Response, OxyError> {
        let result = ToolLauncher::new()
            .with_external_context(execution_context)?
            .launch(input, execution_context.writer.clone())
            .await;

        match &result {
            Ok(output) => events::tool::tool_call_output(output),
            Err(e) => events::tool::tool_call_error(&e.to_string()),
        }
        result
    }
}

fn build_tool_executable()
-> impl Executable<(String, Option<ToolType>, ToolRawInput), Response = OutputContainer> {
    super::tool::ToolExecutable
}
