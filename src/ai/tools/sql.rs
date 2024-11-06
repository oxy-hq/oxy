use super::Tool;
use crate::{connector::Connector, yaml_parsers::config_parser::Warehouse};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use syntect::{
    easy::HighlightLines,
    highlighting::{Style, ThemeSet},
    parsing::SyntaxSet,
    util::{as_24_bit_terminal_escaped, LinesWithEndings},
};

#[derive(Deserialize, Debug, JsonSchema)]
pub struct ExecuteSQLParams {
    pub sql: String,
}

pub struct ExecuteSQLTool {
    pub config: Warehouse,
    pub tool_description: String,
}

#[async_trait]
impl Tool for ExecuteSQLTool {
    type Input = ExecuteSQLParams;

    fn name(&self) -> String {
        "execute_sql".to_string()
    }
    fn description(&self) -> String {
        self.tool_description.clone()
    }
    async fn call_internal(&self, parameters: &ExecuteSQLParams) -> anyhow::Result<String> {
        println!("\n\x1b[1;32mSQL query:\x1b[0m");
        print_colored_sql(&parameters.sql);
        let connector = Connector::new(&self.config);
        let file_path = connector.run_query(&parameters.sql).await?;
        Ok(file_path)
    }
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
