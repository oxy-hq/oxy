use std::{collections::HashMap, fs};

use minijinja::{Value, context};

use crate::{
    config::model::{ExecuteSQLTask, SQL},
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, map::ParamMapper},
        types::Output,
    },
    tools::{SQLExecutable, types::SQLInput},
};

#[derive(Clone)]
struct SQLTaskMapper;

#[async_trait::async_trait]
impl ParamMapper<ExecuteSQLTask, SQLInput> for SQLTaskMapper {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: ExecuteSQLTask,
    ) -> Result<(SQLInput, Option<ExecutionContext>), OxyError> {
        let mut variables = HashMap::new();
        if let Some(vars) = &input.variables {
            for (key, value) in vars {
                let rendered_value = execution_context.renderer.render(value)?;
                variables.insert(key.clone(), rendered_value);
            }
        }

        let sql = match &input.sql {
            SQL::Query { sql_query } => {
                let query = execution_context.renderer.render(sql_query)?;
                if !variables.is_empty() {
                    execution_context
                        .renderer
                        .render_once(&query, Value::from_serialize(&variables))?
                } else {
                    query
                }
            }
            SQL::File { sql_file } => {
                let rendered_sql_file = execution_context.renderer.render(sql_file)?;
                let query_file = execution_context
                    .config
                    .resolve_file(&rendered_sql_file)
                    .await?;
                match fs::read_to_string(&query_file) {
                    Ok(query) => {
                        let context = if variables.is_empty() {
                            execution_context.renderer.get_context()
                        } else {
                            context! {
                                ..execution_context.renderer.get_context(),
                                ..Value::from_serialize(&variables)
                            }
                        };
                        execution_context.renderer.render_once(&query, context)?
                    }
                    Err(e) => {
                        return Err(OxyError::RuntimeError(format!(
                            "Error reading query file {}: {}",
                            &query_file, e
                        )));
                    }
                }
            }
        };

        Ok((
            SQLInput {
                sql,
                database: input.database,
                dry_run_limit: input.dry_run_limit,
            },
            None,
        ))
    }
}

pub fn build_sql_task_executable() -> impl Executable<ExecuteSQLTask, Response = Output> {
    ExecutableBuilder::new()
        .map(SQLTaskMapper)
        .executable(SQLExecutable::new())
}
