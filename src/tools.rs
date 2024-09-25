use std::{collections::HashMap, error::Error};

use arrow_cast::pretty::pretty_format_batches;
use async_trait::async_trait;
use schemars::{schema_for, JsonSchema};
use serde::Deserialize;
use serde_json::{json, Value};
use syntect::{
    easy::HighlightLines,
    highlighting::{Style, ThemeSet},
    parsing::SyntaxSet,
    util::{as_24_bit_terminal_escaped, LinesWithEndings},
};

use crate::{
    connector::Connector,
    prompt::PromptBuilder,
    yaml_parsers::config_parser::{ParsedConfig, Warehouse},
};

#[async_trait]
pub trait Tool<S>
where
    S: for<'a> Deserialize<'a> + JsonSchema,
{
    fn name(&self) -> String;
    fn description(&self) -> String;
    fn param_spec(&self) -> serde_json::Value {
        json!(&schema_for!(S))
    }
    fn validate(&self, parameters: &String) -> Result<S, Box<dyn Error>> {
        serde_json::from_str::<S>(parameters).map_err(|e| e.into())
    }
    async fn call(&self, parameters: String) -> Result<String, Box<dyn Error>> {
        let params = self.validate(&parameters)?;
        self.call_internal(params).await
    }
    async fn setup(&mut self) {}
    async fn call_internal(&self, parameters: S) -> Result<String, Box<dyn Error>>;
}

#[derive(Deserialize, Debug, JsonSchema)]
struct ExecuteSQLParams {
    sql: String,
}

#[derive(Clone)]
pub struct ExecuteSQLTool {
    config: Warehouse,
    tool_description: String,
}

#[async_trait]
impl Tool<ExecuteSQLParams> for ExecuteSQLTool {
    fn name(&self) -> String {
        "execute_sql".to_string()
    }
    fn description(&self) -> String {
        self.tool_description.clone()
    }
    async fn call_internal(&self, parameters: ExecuteSQLParams) -> Result<String, Box<dyn Error>> {
        println!("\n\x1b[1;32mSQL query:\x1b[0m");
        print_colored_sql(&parameters.sql);
        let config = self.config.clone();
        let connector = Connector::new(config);
        let batches = connector.run_query(&parameters.sql).await?;
        let batches_display = pretty_format_batches(&batches)?;
        println!("\n\x1b[1;32mResults:\x1b[0m");
        println!("{}", batches_display);
        Ok(batches_display.to_string())
    }
}

type ToolParams = ExecuteSQLParams;
type ToolImpl = Box<dyn Tool<ToolParams> + Sync + Send>;

#[derive(Default)]
pub struct ToolBox {
    tools: HashMap<String, ToolImpl>,
}

type SpecSerializer<Ret> = fn(String, String, Value) -> Ret;

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

fn print_colored_sql(sql: &str) {
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let syntax = ps.find_syntax_by_extension("sql").unwrap();
    let mut h = HighlightLines::new(syntax, &ts.themes["base16-ocean.dark"]);

    for line in LinesWithEndings::from(sql) {
        let ranges: Vec<(Style, &str)> = h.highlight_line(line, &ps).unwrap();
        let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
        print!("{}", escaped);
    }
    println!();
}
