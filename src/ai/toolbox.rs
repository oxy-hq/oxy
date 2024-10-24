use super::{
    prompt::PromptBuilder,
    tools::{ExecuteSQLParams, ExecuteSQLTool, Tool},
};
use crate::yaml_parsers::config_parser::ParsedConfig;
use serde_json::Value;
use std::collections::HashMap;

type ToolParams = ExecuteSQLParams;
type ToolImpl = Box<dyn Tool<ToolParams> + Sync + Send>;

#[derive(Default)]
pub struct ToolBox {
    tools: HashMap<String, ToolImpl>,
}

pub type SpecSerializer<Ret> = fn(String, String, Value) -> Ret;

impl ToolBox {
    pub async fn fill_toolbox(&mut self, config: &ParsedConfig, prompt_builder: &PromptBuilder) {
        let sql_tool = ExecuteSQLTool {
            config: config.warehouse.clone(),
            tool_description: prompt_builder.sql_tool(),
        };
        self.tools
            .insert(sql_tool.name(), Box::new(sql_tool) as ToolImpl);
        for (_name, tool) in &mut self.tools {
            tool.setup().await;
        }
    }

    pub fn to_spec<Ret>(&self, spec_serializer: SpecSerializer<Ret>) -> Vec<Ret> {
        let mut spec = Vec::new();
        for (_name, tool) in &self.tools {
            spec.insert(
                spec.len(),
                spec_serializer(tool.name(), tool.description(), tool.param_spec()),
            );
        }
        spec
    }

    pub async fn run_tool(&self, name: String, parameters: String) -> String {
        let tool = self.tools.get(&name);

        if tool.is_none() {
            return format!("Tool {} not found", name);
        }

        match tool.unwrap().call(parameters).await {
            Ok(result) => truncate_with_ellipsis(&result, 1000),
            Err(e) => {
                log::debug!("Error executing tool: {}", e);
                truncate_with_ellipsis(&format!("Error executing tool: {:?}", e), 1000)
            }
        }
    }
}

fn truncate_with_ellipsis(s: &str, max_width: usize) -> String {
    // We should truncate at grapheme-boundary and compute character-widths,
    // yet the dependencies on unicode-segmentation and unicode-width are
    // not worth it.
    let mut chars = s.chars();
    let mut prefix = (&mut chars).take(max_width - 1).collect::<String>();
    if chars.next().is_some() {
        prefix.push('…');
    }
    prefix
}
