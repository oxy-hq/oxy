use crate::{
    connector::Connector,
    execute::{Executable, ExecutionContext, types::Output},
    tools::types::SQLInput,
};
use oxy_shared::errors::OxyError;

#[derive(Debug, Clone)]
pub struct ValidateSQLExecutable;

impl Default for ValidateSQLExecutable {
    fn default() -> Self {
        Self::new()
    }
}

impl ValidateSQLExecutable {
    pub fn new() -> Self {
        Self
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
        let config_manager = &execution_context.project.config_manager;
        let secrets_manager = &execution_context.project.secrets_manager;
        let connector = Connector::from_database(
            &input.database,
            config_manager,
            secrets_manager,
            None,
            execution_context.filters.clone(),
            execution_context.connections.clone(),
        )
        .await?;
        let success = match connector.explain_query(&input.sql).await {
            Ok(_) => Output::Bool(true),
            Err(err) => {
                let error_message = format!("SQL validation failed: {err}");
                Output::Text(error_message)
            }
        };
        Ok(success)
    }
}
