use super::tools::Tool;
use crate::{execute::agent::ToolCall, utils::truncate_with_ellipsis};
use serde_json::Value;
use std::{
    collections::HashMap,
    fmt::{Debug, Formatter, Result as FmtResult},
};

pub struct ToolBox<T> {
    tools: HashMap<String, T>,
}

impl<T> Debug for ToolBox<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("ToolBox").finish()
    }
}

pub type SpecSerializer<Ret> = fn(String, String, Value) -> Ret;

impl<T> Default for ToolBox<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> ToolBox<T> {
    pub fn new() -> Self {
        ToolBox {
            tools: HashMap::new(),
        }
    }
}

impl<T> ToolBox<T>
where
    T: Tool + Send + Sync,
{
    pub fn add_tool(&mut self, name: String, tool: T) {
        self.tools.insert(name, tool);
    }

    pub fn to_spec<Ret>(&self, spec_serializer: SpecSerializer<Ret>) -> Vec<Ret> {
        let mut spec = Vec::new();
        for tool in self.tools.values() {
            spec.insert(
                spec.len(),
                spec_serializer(tool.name(), tool.description(), tool.param_spec().unwrap()),
            );
        }
        spec
    }

    pub async fn run_tool(&self, name: &str, parameters: String) -> ToolCall {
        let tool = self.tools.get(name);

        match tool {
            None => ToolCall {
                name: name.to_string(),
                output: format!("Tool {} not found", name),
                metadata: None,
            },
            Some(tool) => match tool.call(&parameters).await {
                Ok(tool_call) => tool_call,
                Err(e) => {
                    let err_msg =
                        truncate_with_ellipsis(&format!("Error executing tool: {:?}", e), None);
                    log::info!("{}", err_msg);
                    ToolCall {
                        name: name.to_string(),
                        output: err_msg,
                        metadata: None,
                    }
                }
            },
        }
    }
}
