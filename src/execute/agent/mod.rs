use std::path::PathBuf;

use contexts::Contexts;
use minijinja::{context, Value};
use tools::ToolsContext;

use crate::{
    ai::{agent::AgentResult, setup_agent, utils::record_batches_to_table},
    config::model::{AgentConfig, FileFormat},
    connector::load_result,
    errors::OnyxError,
    utils::{print_colored_sql, truncate_datasets, truncate_with_ellipsis, MAX_DISPLAY_ROWS},
    StyledText,
};

use super::{
    core::{event::Handler, write::OutputCollector, Executable, ExecutionContext},
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

#[derive(Debug, Clone)]
pub enum ToolMetadata {
    ExecuteSQL {
        sql_query: String,
        output_file: String,
    },
}

impl ToolMetadata {
    pub fn copy(&self) -> Self {
        match self {
            ToolMetadata::ExecuteSQL {
                sql_query,
                output_file,
            } => ToolMetadata::ExecuteSQL {
                sql_query: sql_query.clone(),
                output_file: output_file.clone(),
            },
        }
    }
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

impl AgentEvent {
    pub fn propagate(&self, handler: &dyn Handler<Event = AgentEvent>) {
        match self {
            AgentEvent::ToolCall(tool_call) => match &tool_call.metadata {
                Some(metadata) => match metadata {
                    ToolMetadata::ExecuteSQL { .. } => {
                        handler.handle(&AgentEvent::ToolCall(
                            tool_call.with_metadata(metadata.copy()),
                        ));
                    }
                },
                _ => {
                    log::debug!("Unhandled tool event: {:?}", &tool_call.name);
                }
            },
            _ => {}
        }
    }
}

pub struct AgentReceiver<'handler> {
    handler: Option<&'handler (dyn Handler<Event = AgentEvent> + 'handler)>,
}

impl<'handler> AgentReceiver<'handler> {
    pub fn new(handler: Option<&'handler (dyn Handler<Event = AgentEvent> + 'handler)>) -> Self {
        Self { handler: handler }
    }
}

impl Handler for AgentReceiver<'_> {
    type Event = AgentEvent;

    fn handle(&self, event: &Self::Event) {
        if let Some(handler) = self.handler {
            let _ = &event.propagate(handler);
        }

        match &event {
            AgentEvent::Started => {}
            AgentEvent::Finished { output } => {
                println!("{}", "\nOutput:".primary());
                println!("{}", output);
            }
            AgentEvent::ToolCall(tool_call) => match &tool_call.metadata {
                Some(ToolMetadata::ExecuteSQL {
                    sql_query,
                    output_file,
                }) => {
                    print_colored_sql(&sql_query);
                    match load_result(&output_file) {
                        Ok((batches, schema)) => {
                            let (batches, truncated) = truncate_datasets(batches);
                            match record_batches_to_table(&batches, &schema) {
                                Ok(table) => {
                                    println!("{}", "\nResult:".primary());
                                    println!("{}", table);
                                    if truncated {
                                        println!("{}", format!(
                                                "Results have been truncated. Showing only the first {} rows.",
                                                MAX_DISPLAY_ROWS
                                            ).warning());
                                    }
                                }
                                Err(e) => {
                                    eprintln!(
                                        "{}",
                                        format!("Error in converting record batch to table: {}", e)
                                            .error()
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("{}", format!("Error loading result: {}", e).error());
                        }
                    }
                }
                None => {
                    log::debug!("Unhandled tool call: {:?}", &tool_call);
                }
            },
        }
    }
}

pub async fn run_agent(
    agent_file: Option<&PathBuf>,
    file_format: &FileFormat,
    prompt: &str,
    event_handler: Option<&dyn Handler<Event = AgentEvent>>,
) -> Result<AgentResult, OnyxError> {
    let (agent, agent_config, config) = setup_agent(agent_file, file_format)?;
    let contexts = Contexts::new(
        agent_config.context.clone().unwrap_or_default(),
        config.clone(),
    );
    let warehouses = WarehousesContext::new(config.warehouses.clone(), config.clone());
    let tools_context = ToolsContext::new(agent.tools.clone(), prompt.to_string());
    let global_context = context! {
        context => Value::from_object(contexts),
        warehouses => Value::from_object(warehouses),
        tools => Value::from_object(tools_context),
    };
    let mut renderer = Renderer::new();
    renderer.register(&agent_config)?;
    let agent_receiver = AgentReceiver::new(event_handler);

    let mut output_collector = OutputCollector::new(&agent_receiver);
    let mut execution_context = ExecutionContext::new(
        Value::from_safe_string(prompt.to_string()),
        &mut renderer,
        &global_context,
        &mut output_collector,
        config.clone(),
    );
    agent.execute(&mut execution_context).await?;
    let output = output_collector.output.unwrap_or_default();
    Ok(AgentResult { output })
}
