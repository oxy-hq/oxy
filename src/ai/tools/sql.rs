use super::Tool;
use crate::{
    ai::utils::record_batches_to_markdown,
    config::model::{OutputFormat, Warehouse},
    connector::load_result,
    connector::Connector,
    utils::print_colored_sql,
};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, Debug, JsonSchema)]
pub struct ExecuteSQLParams {
    pub sql: String,
}

pub struct ExecuteSQLTool {
    pub config: Warehouse,
    pub tool_description: String,
    pub output_format: OutputFormat,
}

#[async_trait]
impl Tool for ExecuteSQLTool {
    type Input = ExecuteSQLParams;

    fn name(&self) -> String {
        "execute_sql".to_string()
    }
    fn description(&self) -> String {
        let mut description = self.tool_description.clone();
        if let OutputFormat::File = self.output_format {
            description
                .push_str(" Output of this tool is a <file_path> used to retrieve the result.");
        }
        description
    }
    async fn call_internal(&self, parameters: &ExecuteSQLParams) -> anyhow::Result<String> {
        print_colored_sql(&parameters.sql);
        let connector = Connector::new(&self.config);
        let file_path = connector.run_query(&parameters.sql).await?;

        match self.output_format {
            OutputFormat::Default => {
                let (datasets, schema) = load_result(&file_path)?;
                let markdown_table = record_batches_to_markdown(&datasets, &schema)?;
                Ok(markdown_table.to_string())
            }
            OutputFormat::File => Ok(file_path),
        }
    }
}
