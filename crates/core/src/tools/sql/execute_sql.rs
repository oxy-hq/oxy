use std::collections::HashMap;

use crate::{
    adapters::connector::Connector,
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        types::{Chunk, EventKind, SQL, Table, TableReference},
    },
    tools::types::SQLInput,
};

#[derive(Debug, Clone)]
pub struct SQLExecutable;

impl SQLExecutable {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl Executable<SQLInput> for SQLExecutable {
    type Response = Table;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: SQLInput,
    ) -> Result<Self::Response, OxyError> {
        execution_context
            .write_kind(EventKind::Started {
                name: input.sql.to_string(),
                attributes: HashMap::from_iter([
                    ("database".to_string(), input.database.to_string()),
                    ("sql_query".to_string(), input.sql.to_string()),
                ]),
            })
            .await?;
        execution_context
            .write_chunk(Chunk {
                key: None,
                delta: SQL::new(input.sql.clone()).into(),
                finished: true,
            })
            .await?;
        let config_manager = &execution_context.project.config_manager;
        let secrets_manager = &execution_context.project.secrets_manager;
        let result: Result<Table, OxyError> = {
            let connector = Connector::from_database(
                &input.database,
                config_manager,
                secrets_manager,
                input.dry_run_limit,
            )
            .await?;
            let file_path = connector.run_query(&input.sql).await?;
            let table = Table::with_reference(
                file_path,
                TableReference {
                    sql: input.sql.clone(),
                    database_ref: input.database.clone(),
                },
                input.name.clone(),
                None,
            );
            Ok(table)
        };
        match result.as_ref() {
            Ok(table) => {
                execution_context
                    .write_chunk(Chunk {
                        key: None,
                        delta: table.clone().into(),
                        finished: true,
                    })
                    .await?;
                execution_context
                    .write_kind(EventKind::Finished {
                        message: "".to_string(),
                        attributes: Default::default(),
                        error: None,
                    })
                    .await?;
            }
            Err(e) => {
                execution_context
                    .write_kind(EventKind::Finished {
                        message: "".to_string(),
                        attributes: Default::default(),
                        error: Some(e.to_string()),
                    })
                    .await?;
            }
        }

        result
    }
}
