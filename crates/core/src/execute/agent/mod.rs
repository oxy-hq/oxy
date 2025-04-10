use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use contexts::Contexts;
use minijinja::{Value, context};
use serde::Serialize;
use tools::ToolsContext;

use super::{
    core::{event::Handler, run},
    databases::DatabasesContext,
    renderer::{Renderer, TemplateRegister},
    workflow::WorkflowLogger,
};
use crate::{
    adapters::connector::load_result, execute::workflow::WorkflowCLILogger,
    utils::truncate_datasets,
};
use crate::{
    ai::{
        agent::{AgentResult, OpenAIAgent},
        setup_agent,
        utils::record_batches_to_2d_array,
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
    fn register_template(&self, renderer: &Renderer) -> Result<(), OxyError> {
        renderer.register_template(&self.system_instructions)?;
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ToolMetadata {
    ExecuteSQL {
        sql_query: String,
        database: String,
        output_file: String,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentReference {
    SqlQuery(SqlQueryReference),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SqlQueryReference {
    pub sql_query: String,
    pub database: String,
    pub result: Vec<Vec<String>>,
    pub is_result_truncated: bool,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, Serialize)]
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
    pub references_collector: Option<Arc<Mutex<ReferenceCollector>>>,
}

impl AgentReceiver {
    pub fn new(logger: Box<dyn WorkflowLogger>) -> Self {
        AgentReceiver {
            logger,
            references_collector: None,
        }
    }

    pub fn references(&self) -> Option<Vec<AgentReference>> {
        self.references_collector.as_ref()?;
        Some(
            self.references_collector
                .as_ref()
                .map(|collector| collector.lock().unwrap().references.clone())
                .unwrap_or_default(),
        )
    }
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
                if let Some(collector) = &self.references_collector {
                    if let Ok(mut collector) = collector.lock() {
                        collector.collect(tool_call.clone());
                    }
                }
            }
        }
    }
}

pub async fn build_agent<P: AsRef<Path>>(
    agent_file: P,
    file_format: &FileFormat,
    prompt: Option<String>,
    config: ConfigManager,
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

pub struct ReferenceCollector {
    pub references: Vec<AgentReference>,
}

impl Default for ReferenceCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl ReferenceCollector {
    pub fn new() -> Self {
        ReferenceCollector { references: vec![] }
    }

    pub fn collect(&mut self, tool_call: ToolCall) {
        match tool_call.metadata {
            Some(ToolMetadata::ExecuteSQL {
                sql_query,
                database,
                output_file,
            }) => match load_result(&output_file) {
                Err(_) => {}
                Ok((datasets, schema)) => {
                    let (truncated_results, truncated) = truncate_datasets(datasets.clone());
                    let formatted_results =
                        record_batches_to_2d_array(&truncated_results, &schema).unwrap_or_default();
                    let reference = SqlQueryReference {
                        sql_query,
                        database,
                        result: formatted_results,
                        is_result_truncated: truncated,
                    };
                    self.references.push(AgentReference::SqlQuery(reference));
                }
            },
            None => {}
        }
    }
}

pub async fn run_agent<P: AsRef<Path>>(
    agent_file: P,
    file_format: &FileFormat,
    prompt: Option<String>,
    config: ConfigManager,
    logger: Option<Box<dyn WorkflowLogger>>,
) -> Result<AgentResult, OxyError> {
    let (agent, agent_config, global_context) =
        build_agent(agent_file, file_format, prompt.clone(), config.clone()).await?;

    use std::sync::{Arc, Mutex};

    let references_collector = Arc::new(Mutex::new(ReferenceCollector::new()));

    let agent_logger = logger.unwrap_or_else(|| Box::new(WorkflowCLILogger {}));

    let handler = AgentReceiver {
        logger: agent_logger,
        references_collector: Some(Arc::clone(&references_collector)),
    };
    let output = run(
        &agent,
        AgentInput {
            prompt,
            system_instructions: agent_config.system_instructions.clone(),
        },
        config,
        global_context,
        Some(&agent_config),
        handler,
    )
    .await?;
    let references = references_collector
        .lock()
        .map_or(vec![], |guard| guard.references.clone());
    Ok(AgentResult { output, references })
}
