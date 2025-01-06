use std::path::PathBuf;

use contexts::Contexts;
use minijinja::{context, Value};
use tools::ToolsContext;

use crate::{
    ai::{agent::AgentResult, setup_agent},
    config::model::{AgentConfig, FileFormat},
    errors::OnyxError,
};

use super::{
    core::{Executable, ExecutionContext, OutputCollector},
    renderer::{Renderer, TemplateRegister},
    warehouses::WarehousesContext,
};

pub mod contexts;
pub mod tools;

impl TemplateRegister for AgentConfig {
    fn register_template(&self, renderer: &mut Renderer) -> Result<(), OnyxError> {
        renderer.register_template(&self.system_instructions)?;
        Ok(())
    }
}

pub async fn run_agent(
    agent_file: Option<&PathBuf>,
    file_format: &FileFormat,
    prompt: &str,
) -> Result<AgentResult, OnyxError> {
    let (agent, agent_config, config) = setup_agent(agent_file, file_format)?;
    let contexts = Contexts::new(agent_config.context.clone().unwrap_or_default());
    let warehouses = WarehousesContext::new(config.warehouses.clone());
    let tools_context = ToolsContext::new(agent.tools.clone(), prompt.to_string());
    let global_context = context! {
        context => Value::from_object(contexts),
        warehouses => Value::from_object(warehouses),
        tools => Value::from_object(tools_context),
    };
    let mut renderer = Renderer::new();
    renderer.register(&agent_config)?;
    let mut output_collector = OutputCollector::default();
    let mut execution_context = ExecutionContext::new(
        Value::from_safe_string(prompt.to_string()),
        &mut renderer,
        &global_context,
        &mut output_collector,
    );
    agent.execute(&mut execution_context).await?;
    let output = output_collector.output.unwrap_or_default();
    Ok(output.into())
}
