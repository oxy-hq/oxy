use super::{
    retrieval::RetrievalExecutable,
    sql::{SQLExecutable, ValidateSQLExecutable},
    types::{RetrievalInput, RetrievalParams, SQLInput, SQLParams, ToolRawInput, VisualizeInput},
    visualize::{types::VisualizeParams, visualize::VisualizeExecutable},
};
use crate::{
    config::model::{RetrievalConfig, ToolType},
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, map::ParamMapper},
        types::Output,
    },
};

const TOOL_NOT_FOUND_ERR: &str = "Tool not found";

pub trait Tool {
    type Param: schemars::JsonSchema + serde::de::DeserializeOwned;
    type Output;

    fn serialize_output(&self, output: &Self::Output) -> Result<String, OxyError>;
}

#[derive(Clone)]
pub struct ToolExecutable;

#[async_trait::async_trait]
impl Executable<(String, Option<ToolType>, ToolRawInput)> for ToolExecutable {
    type Response = Output;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: (String, Option<ToolType>, ToolRawInput),
    ) -> Result<Self::Response, OxyError> {
        let (agent_name, tool_type, input) = input;
        if let Some(tool_type) = &tool_type {
            let tool_ret = match tool_type {
                ToolType::ExecuteSQL(sql_config) => {
                    build_sql_executable(SQLExecutable::new())
                        .execute(
                            execution_context,
                            SQLToolInput {
                                database: sql_config.database.clone(),
                                param: input.param.clone(),
                            },
                        )
                        .await
                }
                ToolType::ValidateSQL(sql_config) => {
                    build_sql_executable(ValidateSQLExecutable::new())
                        .execute(
                            execution_context,
                            SQLToolInput {
                                database: sql_config.database.clone(),
                                param: input.param.clone(),
                            },
                        )
                        .await
                }
                ToolType::Retrieval(retrieval_config) => {
                    build_retrieval_executable()
                        .execute(
                            execution_context,
                            RetrievalToolInput {
                                agent_name,
                                param: input.param.clone(),
                                retrieval_config: retrieval_config.clone(),
                            },
                        )
                        .await
                }
                ToolType::Visualize(_visualize_config) => {
                    build_visualize_executable()
                        .execute(
                            execution_context,
                            VisualizeToolInput {
                                param: input.param.clone(),
                            },
                        )
                        .await
                }
            };
            let ToolRawInput {
                call_id,
                handle,
                param,
            } = input;
            return tool_ret.map_err(|err| OxyError::ToolCallError {
                call_id,
                handle,
                param,
                msg: err.to_string(),
            });
        } else {
            let ToolRawInput {
                call_id,
                handle,
                param,
            } = input;
            return Err(OxyError::ToolCallError {
                call_id,
                msg: TOOL_NOT_FOUND_ERR.to_string(),
                handle,
                param,
            });
        }
    }
}

#[derive(Clone)]
struct SQLToolInput {
    database: String,
    param: String,
}

#[derive(Clone)]
struct SQLMapper;

#[async_trait::async_trait]
impl ParamMapper<SQLToolInput, SQLInput> for SQLMapper {
    async fn map(
        &self,
        _execution_context: &ExecutionContext,
        input: SQLToolInput,
    ) -> Result<(SQLInput, Option<ExecutionContext>), OxyError> {
        let SQLToolInput {
            param, database, ..
        } = input;
        let SQLParams { sql } = serde_json::from_str::<SQLParams>(&param)?;
        Ok((SQLInput { sql, database }, None))
    }
}

fn build_sql_executable<E>(executable: E) -> impl Executable<SQLToolInput, Response = Output>
where
    E: Executable<SQLInput, Response = Output> + Send,
{
    ExecutableBuilder::new()
        .map(SQLMapper)
        .executable(executable)
}

#[derive(Clone)]
struct VisualizeToolInput {
    param: String,
}

#[derive(Clone)]
struct VisualizeMapper;

#[async_trait::async_trait]
impl ParamMapper<VisualizeToolInput, VisualizeInput> for VisualizeMapper {
    async fn map(
        &self,
        _execution_context: &ExecutionContext,
        input: VisualizeToolInput,
    ) -> Result<(VisualizeInput, Option<ExecutionContext>), OxyError> {
        let VisualizeToolInput { param, .. } = input;
        let visualize_params = serde_json::from_str::<VisualizeParams>(&param)?;
        Ok((
            VisualizeInput {
                param: visualize_params,
            },
            None,
        ))
    }
}

fn build_visualize_executable() -> impl Executable<VisualizeToolInput, Response = Output> {
    ExecutableBuilder::new()
        .map(VisualizeMapper)
        .executable(VisualizeExecutable::new())
}

#[derive(Clone)]
struct RetrievalToolInput {
    agent_name: String,
    param: String,
    retrieval_config: RetrievalConfig,
}

#[derive(Clone)]
struct RetrievalMapper;

#[async_trait::async_trait]
impl ParamMapper<RetrievalToolInput, RetrievalInput> for RetrievalMapper {
    async fn map(
        &self,
        _execution_context: &ExecutionContext,
        input: RetrievalToolInput,
    ) -> Result<(RetrievalInput, Option<ExecutionContext>), OxyError> {
        let RetrievalToolInput {
            agent_name,
            param,
            retrieval_config,
        } = input;
        let query = match serde_json::from_str::<RetrievalParams>(&param) {
            Ok(RetrievalParams { query }) => query,
            Err(_) => param,
        };
        Ok((
            RetrievalInput {
                agent_name,
                query,
                retrieval_config,
            },
            None,
        ))
    }
}

fn build_retrieval_executable() -> impl Executable<RetrievalToolInput, Response = Output> {
    ExecutableBuilder::new()
        .map(RetrievalMapper)
        .executable(RetrievalExecutable::new())
}
