use super::tools::Tool;
use crate::{ utils::truncate_with_ellipsis};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Default)]
pub struct ToolBox<T> {
    tools: HashMap<String, T>,
}

pub type SpecSerializer<Ret> = fn(String, String, Value) -> Ret;

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
        for (_name, tool) in &self.tools {
            spec.insert(
                spec.len(),
                spec_serializer(tool.name(), tool.description(), tool.param_spec().unwrap()),
            );
        }
        spec
    }

    pub async fn run_tool(&self, name: String, parameters: String) -> String {
        let tool = self.tools.get(&name);

        if tool.is_none() {
            return format!("Tool {} not found", name);
        }
        let response = tool.unwrap().call(&parameters).await;
        match response {
            Ok(result) => truncate_with_ellipsis(&result, 1000),
            Err(e) => {
                let err_msg =
                    truncate_with_ellipsis(&format!("Error executing tool: {:?}", e), 1000);
                log::info!("{}", err_msg);
                err_msg
            }
        }
    }
}
