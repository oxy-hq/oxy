use crate::{
    connector::Connector,
    execute::{Executable, ExecutionContext, types::Output},
    observability::events,
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

    #[tracing::instrument(skip_all, err, fields(
        otel.name = events::tool::VALIDATE_SQL_EXECUTE,
        oxy.span_type = events::tool::TOOL_CALL_TYPE,
    ))]
    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: SQLInput,
    ) -> Result<Self::Response, OxyError> {
        events::tool::tool_call_input(&input);
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
        let result = match connector.explain_query(&input.sql).await {
            Ok(_) => Output::Bool(true),
            Err(err) => {
                let error_message = format!("SQL validation failed: {err}");
                Output::Text(error_message)
            }
        };

        match &result {
            Output::Bool(true) => events::tool::tool_call_output(&result),
            _ => events::tool::tool_call_error(&format!("{:?}", result)),
        }
        Ok(result)
    }
}
