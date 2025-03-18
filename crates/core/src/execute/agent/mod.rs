use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use contexts::Contexts;
use minijinja::{Value, context};
use tools::ToolsContext;

use super::{
    core::{event::Handler, run},
    databases::DatabasesContext,
    renderer::{Renderer, TemplateRegister},
    workflow::WorkflowLogger,
};
use crate::execute::workflow::WorkflowCLILogger;
use crate::{
    ai::{
        agent::{AgentResult, OpenAIAgent},
        setup_agent,
    },
    config::{
        ConfigManager,
        model::{AgentConfig, FileFormat},
    },
    errors::OxyError,
    utils::truncate_with_ellipsis,
};

pub mod contexts;
pub mod tools;

impl TemplateRegister for AgentConfig {
    fn register_template(&self, renderer: &mut Renderer) -> Result<(), OxyError> {
        renderer.register_template(&self.system_instructions)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum ToolMetadata {
    ExecuteSQL {
        sql_query: String,
        output_file: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct ToolCall {
    pub name: String,
    pub output: String,
    pub metadata: Option<ToolMetadata>,
}

impl ToolCall {
    pub fn get_truncated_output(&self) -> String {
        truncate_with_ellipsis(&self.output, None)
    }

    pub fn with_metadata(&self, metadata: ToolMetadata) -> Self {
        ToolCall {
            name: self.name.clone(),
            output: self.output.clone(),
            metadata: Some(metadata),
        }
    }
}

#[derive(Debug, Clone)]
pub enum AgentEvent {
    Started,
    ToolCall(ToolCall),
    Finished { output: String },
}

#[derive(Debug, Clone)]
pub struct AgentInput {
    pub prompt: Option<String>,
    pub system_instructions: String,
}

pub struct AgentReceiver {
    pub logger: Box<dyn WorkflowLogger>,
}

impl Handler for AgentReceiver {
    type Event = AgentEvent;

    fn handle(&self, event: &Self::Event) {
        match &event {
            AgentEvent::Started => {}
            AgentEvent::Finished { output } => {
                self.logger.log_agent_finished(output);
            }
            AgentEvent::ToolCall(tool_call) => {
                self.logger.log_agent_tool_call(tool_call);
            }
        }
    }
}

pub async fn build_agent<P: AsRef<Path>>(
    agent_file: P,
    file_format: &FileFormat,
    prompt: Option<String>,
    config: Arc<ConfigManager>,
) -> Result<(OpenAIAgent, AgentConfig, Value), OxyError> {
    let (agent, agent_config) = setup_agent(agent_file, file_format, config.clone()).await?;
    let contexts = Contexts::new(
        agent_config.context.clone().unwrap_or_default(),
        config.clone(),
    );
    let databases = DatabasesContext::new(config);
    let tools_context = ToolsContext::new(agent.tools.clone(), prompt.unwrap_or_default());
    let global_context = context! {
        context => Value::from_object(contexts),
        databases => Value::from_object(databases),
        tools => Value::from_object(tools_context),
    };
    Ok((agent, agent_config, global_context))
}

pub async fn run_agent(
    agent_file: &PathBuf,
    file_format: &FileFormat,
    prompt: Option<String>,
    config: Arc<ConfigManager>,
) -> Result<AgentResult, OxyError> {
    let (agent, agent_config, global_context) =
        build_agent(agent_file, file_format, prompt.clone(), config.clone()).await?;

    let output = run(
        &agent,
        AgentInput {
            prompt,
            system_instructions: agent_config.system_instructions.clone(),
        },
        config.clone(),
        global_context,
        Some(&agent_config),
        AgentReceiver {
            logger: Box::new(WorkflowCLILogger),
        },
    )
    .await?;
    Ok(AgentResult { output })
}
