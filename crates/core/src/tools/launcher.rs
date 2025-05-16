use minijinja::Value;

use crate::{
    adapters::openai::OpenAIToolConfig,
    config::{ConfigManager, constants::TOOL_SOURCE, model::ToolType},
    errors::OxyError,
    execute::{
        Executable, ExecutionContext, ExecutionContextBuilder,
        builders::{ExecutableBuilder, map::ParamMapper},
        types::{OutputContainer, Source},
        writer::{BufWriter, EventHandler},
    },
};

use super::{ToolExecutable, types::ToolRawInput};

pub struct ToolInput {
    pub agent_name: String,
    pub raw: ToolRawInput,
    pub tools: Vec<ToolType>,
}

pub struct ToolLauncher {
    execution_context: Option<ExecutionContext>,
    buf_writer: BufWriter,
}

impl ToolLauncher {
    pub fn new() -> Self {
        ToolLauncher {
            execution_context: None,
            buf_writer: BufWriter::new(),
        }
    }

    pub fn with_config(
        mut self,
        config: ConfigManager,
        source: Option<Source>,
    ) -> Result<Self, OxyError> {
        let source = source.unwrap_or(Source {
            parent_id: None,
            id: TOOL_SOURCE.to_string(),
            kind: TOOL_SOURCE.to_string(),
        });
        self.execution_context = Some(
            ExecutionContextBuilder::new()
                .with_config_manager(config)
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
        let handle =
            tokio::spawn(async move { executable.execute(&execution_context, tool_input).await });
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

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: ToolInput,
    ) -> Result<Self::Response, OxyError> {
        ToolLauncher::new()
            .with_external_context(execution_context)?
            .launch(input, execution_context.writer.clone())
            .await
    }
}

#[derive(Clone, Debug)]
struct ToolMapper;

#[async_trait::async_trait]
impl ParamMapper<ToolInput, (String, Option<ToolType>, ToolRawInput)> for ToolMapper {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: ToolInput,
    ) -> Result<
        (
            (String, Option<ToolType>, ToolRawInput),
            Option<ExecutionContext>,
        ),
        OxyError,
    > {
        let ToolInput {
            agent_name,
            raw,
            tools,
        } = input;
        let tool = tools
            .into_iter()
            .find(|tool| tool.handle() == raw.handle.as_str());
        let tool_kind = match &tool {
            Some(tool) => tool.tool_kind(),
            None => "unknown".to_string(),
        };
        let source_id = match &tool {
            Some(tool) => tool.handle(),
            None => "unknown".to_string(),
        };
        let execution_context = execution_context.with_child_source(source_id, tool_kind);
        Ok(((agent_name, tool, raw), Some(execution_context)))
    }
}

fn build_tool_executable() -> impl Executable<ToolInput, Response = OutputContainer> {
    ExecutableBuilder::new()
        .map(ToolMapper)
        .executable(ToolExecutable)
}
