use crate::{
    adapters::connector::Connector,
    errors::OxyError,
    execute::{Executable, ExecutionContext, types::Output},
    tools::types::SQLInput,
};

#[derive(Debug, Clone)]
pub struct ValidateSQLExecutable;

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
