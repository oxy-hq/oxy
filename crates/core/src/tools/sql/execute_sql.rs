use crate::{
    adapters::connector::Connector,
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        types::{Chunk, Output, SQL, TableReference},
    },
    tools::{
        tool::Tool,
        types::{SQLInput, SQLParams},
    },
};

#[derive(Debug, Clone)]
pub struct SQLExecutable;

impl SQLExecutable {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for SQLExecutable {
    type Param = SQLParams;
    type Output = String;

    fn serialize_output(&self, output: &Self::Output) -> Result<String, OxyError> {
        Ok(output.to_string())
    }
}

#[async_trait::async_trait]
impl Executable<SQLInput> for SQLExecutable {
    type Response = Output;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: SQLInput,
    ) -> Result<Self::Response, OxyError> {
        execution_context
            .write_chunk(Chunk {
                key: None,
                delta: SQL::new(input.sql.clone()).into(),
                finished: true,
            })
            .await?;
        let connector =
            Connector::from_database(&input.database, &execution_context.config).await?;
        let file_path = connector.run_query(&input.sql).await?;
        let table = Output::table_with_reference(
            file_path,
            TableReference {
                sql: input.sql.clone(),
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
