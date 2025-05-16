use crate::{
    adapters::connector::Connector,
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        types::{Chunk, Output, SQL, TableReference},
    },
};

use super::types::OmniInput;

mod bigquery;
mod engine;
mod topic_info;
mod types;
mod utils;

pub use bigquery::BigquerySqlGenerationEngine;
pub use engine::SqlGenerationEngine;
pub use topic_info::OmniTopicInfoExecutable;
pub use types::OmniExecutable;

impl OmniExecutable {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl Executable<OmniInput> for OmniExecutable {
    type Response = Output;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: OmniInput,
    ) -> Result<Self::Response, OxyError> {
        tracing::debug!(
            "{}",
            format!(
                "Executing Omni tool on database {} with input: {:?}",
                input.database, input.params
            )
        );
        // TODO: right now support only bigquery
        let engine = BigquerySqlGenerationEngine::new(input.semantic_model.clone());
        let sql = engine.generate_sql(&input.params)?;
        tracing::debug!("{}", format!("Generated SQL: {}", sql));
        execution_context
            .write_chunk(Chunk {
                key: None,
                delta: SQL::new(sql.clone()).into(),
                finished: true,
            })
            .await?;
        let connector =
            Connector::from_database(&input.database, &execution_context.config, None).await?;
        let file_path = connector.run_query(sql.as_str()).await?;
        let table = Output::table_with_reference(
            file_path,
            TableReference {
                sql: sql.clone(),
                database_ref: input.database.clone(),
            },
        );
        execution_context
            .write_chunk(Chunk {
                key: None,
                delta: table.clone(),
                finished: true,
            })
            .await?;
        Ok(table)
    }
}
