use crate::{
    adapters::connector::Connector,
    errors::OxyError,
    execute::{Executable, ExecutionContext, types::Output},
    tools::{
        tool::Tool,
        types::{SQLInput, SQLParams},
    },
};

#[derive(Debug, Clone)]
pub struct ValidateSQLExecutable;

impl ValidateSQLExecutable {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for ValidateSQLExecutable {
    type Param = SQLParams;
    type Output = bool;

    fn serialize_output(&self, output: &Self::Output) -> Result<String, OxyError> {
        Ok(output.to_string())
    }
}

#[async_trait::async_trait]
impl Executable<SQLInput> for ValidateSQLExecutable {
    type Response = Output;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: SQLInput,
    ) -> Result<Self::Response, OxyError> {
        let connector =
            Connector::from_database(&input.database, &execution_context.config).await?;
        let explain_sql = format!("EXPLAIN ({})", &input.sql.trim().trim_end_matches(';'));
        let success = match connector.run_query(&explain_sql).await {
            Ok(_) => Output::Bool(true),
            Err(err) => {
                let error_message = format!("SQL validation failed: {}", err);
                Output::Text(error_message)
            }
        };
        Ok(success)
    }
}
