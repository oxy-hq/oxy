use super::Tool;
use crate::{
    ai::utils::record_batches_to_markdown,
    config::model::{Config, OutputFormat, Warehouse},
    connector::{load_result, Connector},
    execute::agent::{ToolCall, ToolMetadata},
};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, Debug, JsonSchema)]
pub struct ExecuteSQLParams {
    pub sql: String,
}

#[derive(Debug)]
pub struct ExecuteSQLTool {
    pub warehouse_config: Warehouse,
    pub tool_description: String,
    pub output_format: OutputFormat,
    pub config: Config,
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

    async fn call_internal(&self, parameters: &ExecuteSQLParams) -> anyhow::Result<ToolCall> {
        let connector = Connector::new(&self.warehouse_config, &self.config);
        let file_path = connector.run_query(&parameters.sql).await?;
        let output = match self.output_format {
            OutputFormat::Default => {
                let (datasets, schema) = load_result(&file_path)?;
                let markdown_table = record_batches_to_markdown(&datasets, &schema)?;
                markdown_table.to_string()
            }
            OutputFormat::File => file_path.to_string(),
        };

        Ok(ToolCall {
            name: self.name(),
            output,
            metadata: Some(ToolMetadata::ExecuteSQL {
                sql_query: parameters.sql.to_string(),
                output_file: file_path,
            }),
        })
    }
}
