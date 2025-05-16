use std::sync::Arc;

use minijinja::value::{Object, Value};
use tokio::{runtime::Handle, sync::mpsc::Sender};

use crate::{
    config::{ConfigManager, model::ToolType},
    execute::{
        ExecutionContext,
        types::{Event, Source},
    },
    utils::truncate_with_ellipsis,
};

use super::{ToolInput, ToolLauncher, types::ToolRawInput};

#[derive(Debug, Clone)]
pub struct ToolsContext {
    config: ConfigManager,
    agent_name: String,
    tools_config: Vec<ToolType>,
    prompt: String,
    sender: Sender<Event>,
    source: Source,
}

impl ToolsContext {
    pub fn new(
        config: ConfigManager,
        agent_name: String,
        tools: impl IntoIterator<Item = ToolType>,
        prompt: String,
        sender: Sender<Event>,
        source: Source,
    ) -> Self {
        ToolsContext {
            config,
            agent_name,
            tools_config: tools.into_iter().collect(),
            prompt,
            sender,
            source,
        }
    }

    pub fn from_execution_context(
        execution_context: &ExecutionContext,
        agent_name: String,
        tools: impl IntoIterator<Item = ToolType>,
        prompt: String,
    ) -> Self {
        let config = execution_context.config.clone();
        let sender = execution_context.writer.clone();
        ToolsContext::new(
            config,
            agent_name,
            tools,
            prompt,
            sender,
            execution_context.source.clone(),
        )
    }
}

impl Object for ToolsContext {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let tool_key = key.as_str();
        match tool_key {
            Some(tool_key) => match Handle::try_current() {
                Ok(rt) => {
                    let launcher = ToolLauncher::new()
                        .with_config(self.config.clone(), Some(self.source.clone()))
                        .ok()?;
                    let output = rt
                        .block_on(launcher.launch(
                            ToolInput {
                                agent_name: self.agent_name.to_string(),
                                raw: ToolRawInput {
                                    call_id: "tools_context".to_string(),
                                    handle: tool_key.to_string(),
                                    param: self.prompt.to_string(),
                                },
                                tools: self.tools_config.clone(),
                            },
                            self.sender.clone(),
                        ))
                        .map_err(|err| {
                            tracing::error!("Error launching tool: {:?}", err);
                            err
                        })
                        .ok()?;
                    let parsed_output =
                        truncate_with_ellipsis(&(Into::<Value>::into(&output)).to_string(), None);
                    Some(Value::from_safe_string(parsed_output))
                }
                Err(err) => {
                    tracing::error!("No tokio runtime found: {:?}", err);
                    None
                }
            },
            _ => None,
        }
    }
}
