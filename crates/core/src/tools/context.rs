use std::sync::Arc;

use minijinja::value::{Object, Value};
use tokio::runtime::Handle;

use crate::{
    config::{ConfigManager, model::ToolType},
    execute::writer::NoopHandler,
    utils::truncate_with_ellipsis,
};

use super::{ToolInput, ToolLauncher, types::ToolRawInput};

#[derive(Debug, Clone)]
pub struct ToolsContext {
    config: ConfigManager,
    tools_config: Vec<ToolType>,
    prompt: String,
}

impl ToolsContext {
    pub fn new(
        config: ConfigManager,
        tools: impl IntoIterator<Item = ToolType>,
        prompt: String,
    ) -> Self {
        ToolsContext {
            config,
            tools_config: tools.into_iter().collect(),
            prompt,
        }
    }
}

impl Object for ToolsContext {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let tool_key = key.as_str();
        match tool_key {
            Some(tool_key) => match Handle::try_current() {
                Ok(rt) => {
                    let launcher = ToolLauncher::new().with_config(self.config.clone()).ok()?;
                    let output = rt
                        .block_on(launcher.launch(
                            ToolInput {
                                raw: ToolRawInput {
                                    call_id: "tools_context".to_string(),
                                    handle: tool_key.to_string(),
                                    param: self.prompt.to_string(),
                                },
                                tools: self.tools_config.clone(),
                            },
                            NoopHandler,
                        ))
                        .ok()?;
                    let parsed_output =
                        truncate_with_ellipsis(&Value::from_object(output).to_string(), None);
                    Some(Value::from_safe_string(parsed_output))
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
