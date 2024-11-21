use super::Tool;
use crate::{config::model::Warehouse, connector::Connector, utils::print_colored_sql};
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
        print_colored_sql(&parameters.sql);
        let connector = Connector::new(&self.config);
        let file_path = connector.run_query(&parameters.sql).await?;
        Ok(file_path)
    }
}
