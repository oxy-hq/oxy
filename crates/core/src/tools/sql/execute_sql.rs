use std::{collections::HashMap, path::PathBuf};

use crate::{
    connector::Connector,
    execute::{
        Executable, ExecutionContext,
        types::{Chunk, EventKind, SQL, Table, TableReference},
    },
    observability::events,
    tools::types::SQLInput,
};
use oxy_shared::errors::OxyError;

#[derive(Debug, Clone)]
pub struct SQLExecutable;

impl Default for SQLExecutable {
    fn default() -> Self {
        Self::new()
    }
}

impl SQLExecutable {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl Executable<SQLInput> for SQLExecutable {
    type Response = Table;

    #[tracing::instrument(skip_all, err, fields(
        otel.name = events::tool::SQL_EXECUTE,
        oxy.span_type = events::tool::TOOL_CALL_TYPE,
        oxy.execution_type = tracing::field::Empty,
        oxy.is_verified = tracing::field::Empty,
        oxy.database = tracing::field::Empty,
        oxy.sql = tracing::field::Empty,
        oxy.sql_ref = tracing::field::Empty,
    ))]
    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: SQLInput,
    ) -> Result<Self::Response, OxyError> {
        // Record execution analytics fields
        let span = tracing::Span::current();
        let execution_type = events::tool::EXECUTION_TYPE_SQL_GENERATED;
        span.record("oxy.execution_type", execution_type);
        span.record("oxy.is_verified", &false);
        span.record("oxy.database", &input.database);
        span.record("oxy.sql", &input.sql);

        events::tool::tool_call_input(&input);
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
        let mut result: Result<Table, OxyError> = async {
            let connector = Connector::from_database(
                &input.database,
                config_manager,
                secrets_manager,
                input.dry_run_limit,
                execution_context.filters.clone(),
                execution_context.connections.clone(),
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
        }
        .await;
        // Record SQL for metrics and emit tracing event
        events::tool::add_sql(execution_context, &input.sql);

        match result.as_mut() {
            Ok(table) => {
                if input.persist {
                    let file_name = PathBuf::from(&table.name)
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| table.name.to_string());
                    let file_path = PathBuf::from("contexts")
                        .join("tables")
                        .join(format!("{}.parquet", file_name));
                    let state_dir = execution_context
                        .project
                        .config_manager
                        .resolve_state_dir()
                        .await?;
                    table.save_data(state_dir.join(&file_path))?;
                    table.relative_path = Some(file_path.to_string_lossy().to_string());
                }
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
                events::tool::tool_call_output(table);
            }
            Err(e) => {
                events::tool::tool_call_error(&e.to_string());
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
