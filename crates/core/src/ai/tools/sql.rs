use super::Tool;
use crate::{
    adapters::connector::{Connector, load_result},
    ai::utils::record_batches_to_markdown,
    config::model::OutputFormat,
    execute::agent::{ToolCall, ToolMetadata},
    utils::truncate_datasets,
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
    pub tool_name: String,
    pub tool_description: String,
    pub output_format: OutputFormat,
    pub connector: Connector,
    pub validate_mode: bool,
}

#[async_trait]
impl Tool for ExecuteSQLTool {
    type Input = ExecuteSQLParams;

    fn name(&self) -> String {
        self.tool_name.to_string()
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
        let (output, metadata) = match self.validate_mode {
            true => {
                let (dataset, schema) = self
                    .connector
                    .run_query_and_load(&format!("EXPLAIN ({})", &parameters.sql))
                    .await?;
                log::info!("Validate mode");
                log::info!("Schema: {:?}", schema);
                log::info!("Dataset: {:?}", dataset);
                (true.to_string(), None)
            }
            false => {
                let file_path = self.connector.run_query(&parameters.sql).await?;
                let (datasets, schema) = load_result(&file_path)?;
                let (_truncated_results, _truncated) = truncate_datasets(datasets.clone());
                let output = match self.output_format {
                    OutputFormat::Default => {
                        let markdown_table = record_batches_to_markdown(&datasets, &schema)?;
                        markdown_table.to_string()
                    }
                    OutputFormat::File => file_path.to_string(),
                };
                (
                    output,
                    Some(ToolMetadata::ExecuteSQL {
                        sql_query: parameters.sql.to_string(),
                        output_file: file_path,
                        database: self.connector.database_ref.clone(),
                    }),
                )
            }
        };

        Ok(ToolCall {
            name: self.name(),
            output,
            metadata,
        })
    }
}
