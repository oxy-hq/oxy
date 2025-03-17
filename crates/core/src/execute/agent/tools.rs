use std::sync::Arc;

use minijinja::value::{Object, Value};
use tokio::runtime::Handle;

use crate::ai::{MultiTool, toolbox::ToolBox};

#[derive(Debug, Clone)]
pub struct ToolsContext {
    tools: Arc<ToolBox<MultiTool>>,
    prompt: String,
}

impl ToolsContext {
    pub fn new(tools: Arc<ToolBox<MultiTool>>, prompt: String) -> Self {
        ToolsContext { tools, prompt }
    }
}

impl Object for ToolsContext {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let tool_key = key.as_str();
        match tool_key {
            Some(tool_key) => match Handle::try_current() {
                Ok(rt) => {
                    let output = rt.block_on(self.tools.run_tool(tool_key, self.prompt.clone()));
                    Some(Value::from(output.get_truncated_output()))
                }
                Err(err) => {
                    log::error!("No tokio runtime found: {:?}", err);
                    None
                }
            },
            _ => None,
        }
    }
}
