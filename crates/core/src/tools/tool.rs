use secrecy::SecretString;
use serde_json::Value;

use super::{
    create_data_app::{CreateDataAppExecutable, CreateDataAppInput, CreateDataAppParams},
    registry::global_registry,
    retrieval::RetrievalExecutable,
    sql::SQLExecutable,
    types::{
        CreateV0AppParams, RetrievalInput, RetrievalParams, SQLInput, SQLParams, ToolRawInput,
    },
    v0::{CreateV0App, CreateV0AppInput},
    visualize::VisualizeExecutable,
};
use crate::{
    adapters::openai::OpenAIToolConfig,
    config::{
        constants::ARTIFACT_SOURCE,
        model::{RetrievalConfig, ToolType},
    },
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, map::ParamMapper},
        types::{Chunk, EventKind, Output, OutputContainer, Table},
    },
    observability::events,
    tools::{
        omni::{executable::OmniQueryExecutable, types::OmniQueryInput},
        sql::validate_sql::ValidateSQLExecutable,
        visualize::VisualizeParams,
    },
    types::SemanticQuery,
};
use oxy_shared::errors::OxyError;

const TOOL_NOT_FOUND_ERR: &str = "Tool not found";

#[derive(Clone)]
pub struct ToolExecutable;

#[async_trait::async_trait]
impl Executable<(String, Option<ToolType>, ToolRawInput)> for ToolExecutable {
    type Response = OutputContainer;

    #[tracing::instrument(skip_all, err, fields(
        otel.name = events::tool::TOOL_CALL_EXECUTE,
        oxy.span_type = events::tool::TOOL_CALL_TYPE,
    ))]
    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: (String, Option<ToolType>, ToolRawInput),
    ) -> Result<Self::Response, OxyError> {
        events::tool::tool_call_input(&input);
        let (agent_name, tool_type, input) = input;
        tracing::info!("Executing tool: {:?}", input);
        let artifact_id = uuid::Uuid::new_v4().to_string();
        let artifact_context =
            execution_context.with_child_source(artifact_id, ARTIFACT_SOURCE.to_string());
        if let Some(tool_type) = &tool_type {
            let is_verified = match tool_type {
                ToolType::Workflow(workflow_tool) => workflow_tool.is_verified,
                ToolType::Agent(_) => false, // Agent's answer are not verified
                ToolType::ExecuteSQL(sql_config) => sql_config.sql.is_some(),
                ToolType::SemanticQuery(_semantic_query_tool) => true,
                ToolType::OmniQuery(_omni_query_tool) => true,
                _ => false,
            };
            events::tool::tool_call_is_verified(is_verified);

            let artifact = tool_type.artifact();
            if let Some((title, kind)) = &artifact {
                tracing::info!("Starting artifact: {} of kind {}", title, kind);
                artifact_context
                    .write_kind(EventKind::ArtifactStarted {
                        kind: kind.clone(),
                        title: title.to_string(),
                        is_verified,
                    })
                    .await?;
            }

            let tool_ret: Result<OutputContainer, OxyError> = match tool_type {
                ToolType::ExecuteSQL(sql_config) => {
                    let (param, sql_query) = match sql_config.sql {
                        Some(ref sql) => (
                            serde_json::to_string(&SQLParams {
                                sql: sql.clone(),
                                persist: false,
                            })?,
                            sql.clone(),
                        ),
                        None => {
                            tracing::debug!(
                                "Attempting to deserialize SQL params from: {}",
                                &input.param
                            );
                            let SQLParams { sql, persist: _ } =
                                serde_json::from_str::<SQLParams>(&input.param)?;
                            (input.param.clone(), sql)
                        }
                    };
                    artifact_context
                        .write_kind(EventKind::SQLQueryGenerated {
                            is_verified,
                            query: sql_query,
                            database: sql_config.database.clone(),
                            source: agent_name,
                        })
                        .await?;
                    build_sql_executable(SQLExecutable::new())
                        .execute(
                            execution_context,
                            SQLToolInput {
                                database: sql_config.database.clone(),
                                param,
                                dry_run_limit: sql_config.dry_run_limit,
                            },
                        )
                        .await
                        .map(|output| output.into())
                }
                ToolType::ValidateSQL(sql_config) => {
                    // ValidateSQL expects the SQL query in the param field
                    let param = input.param.clone();

                    build_validate_sql_executable(ValidateSQLExecutable::new())
                        .execute(
                            execution_context,
                            SQLToolInput {
                                database: sql_config.database.clone(),
                                param,
                                dry_run_limit: None,
                            },
                        )
                        .await
                        .map(|output| output.into())
                }
                ToolType::Retrieval(retrieval_config) => build_retrieval_executable()
                    .execute(
                        execution_context,
                        RetrievalToolInput {
                            agent_name,
                            param: input.param.clone(),
                            retrieval_config: retrieval_config.clone(),
                        },
                    )
                    .await
                    .map(|output| output.into()),
                ToolType::Workflow(_workflow_config) => {
                    // Try to use registered executor from higher layers
                    if let Some(result) = global_registry()
                        .execute(execution_context, tool_type, &input)
                        .await?
                    {
                        Ok(result)
                    } else {
                        // No executor registered - return helpful error
                        Err(OxyError::RuntimeError(
                            "Workflow execution not available: No executor registered. \
                            Register a WorkflowExecutor at the application level."
                                .to_string(),
                        ))
                    }
                }
                ToolType::Agent(_agent_config) => {
                    // Try to use registered executor from higher layers
                    if let Some(result) = global_registry()
                        .execute(execution_context, tool_type, &input)
                        .await?
                    {
                        Ok(result)
                    } else {
                        // No executor registered - return helpful error
                        Err(OxyError::RuntimeError(
                            "Agent execution not available: No executor registered. \
                            Register an AgentExecutor at the application level."
                                .to_string(),
                        ))
                    }
                }
                ToolType::Visualize(_visualize_config) => {
                    let params = serde_json::from_str::<VisualizeParams>(&input.param)?;
                    build_visualize_executable()
                        .execute(execution_context, params)
                        .await
                        .map(|output| output.into())
                }
                ToolType::CreateDataApp(_create_data_app_tool) => {
                    build_create_data_app_executable()
                        .execute(
                            execution_context,
                            CreateDataAppToolInput {
                                param: input.param.clone(),
                            },
                        )
                        .await
                        .map(|output| output.into())
                }
                ToolType::OmniQuery(omni_query_tool) => {
                    let params = serde_json::from_str(&input.param).unwrap_or_else(|_| {
                        crate::types::tool_params::OmniQueryParams {
                            fields: vec![],
                            limit: None,
                            sorts: None,
                        }
                    });
                    build_omni_query_tool_executable()
                        .execute(
                            execution_context,
                            OmniQueryInput {
                                params,
                                topic: omni_query_tool.topic.clone(),
                                integration: omni_query_tool.integration.clone(),
                            },
                        )
                        .await
                        .map(|output| output.into())
                }
                ToolType::CreateV0App(create_v0_app_tool) => build_create_v0_app_executable()
                    .execute(
                        execution_context,
                        CreateV0AppToolInput {
                            param: input.param.clone(),
                            system_instruction: create_v0_app_tool.system_instruction.clone(),
                            github_repo: create_v0_app_tool.github_repo.clone(),
                            oxy_api_key_var: create_v0_app_tool.oxy_api_key_var.clone(),
                            v0_api_key_var: create_v0_app_tool.v0_api_key_var.clone(),
                        },
                    )
                    .await
                    .map(|output| output.into()),
                ToolType::SemanticQuery(_semantic_query_tool) => {
                    let param_obj = serde_json::from_str::<Value>(&input.param).unwrap_or_default();
                    let mut artifact_value = create_semantic_query_artifact(&param_obj);

                    tracing::debug!(
                        "SemanticQueryToolInput param_obj: {}",
                        param_obj.to_string()
                    );
                    let _ = artifact_context
                        .write_chunk(Chunk {
                            key: None,
                            delta: Output::SemanticQuery(artifact_value.clone()),
                            finished: true,
                        })
                        .await;

                    let result_t: Result<OutputContainer, OxyError> = match global_registry()
                        .execute(execution_context, tool_type, &input)
                        .await
                    {
                        Ok(rs) => {
                            match rs {
                                Some(result) => Ok(result),
                                None => {
                                    tracing::error!("No SemanticQueryExecutor registered");
                                    artifact_value.validation_error =
                                        Some("No SemanticQueryExecutor registered".to_string());
                                    let _ = artifact_context
                                        .write_chunk(Chunk {
                                            key: None,
                                            delta: Output::SemanticQuery(artifact_value.clone()),
                                            finished: true,
                                        })
                                        .await;
                                    // No executor registered - return helpful error
                                    Err(OxyError::RuntimeError(
                                "SemanticQuery execution not available: No executor registered. \
                                Register a SemanticQueryExecutor at the application level."
                                    .to_string(),
                            ))
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to parse SemanticQueryParams: {}", e);
                            artifact_value.validation_error = Some(format!("{}", e));
                            let _ = artifact_context
                                .write_chunk(Chunk {
                                    key: None,
                                    delta: Output::SemanticQuery(artifact_value.clone()),
                                    finished: true,
                                })
                                .await;
                            Err(OxyError::ArgumentError(format!(
                                "Invalid SemanticQueryParams: {}",
                                e
                            )))
                        }
                    };
                    result_t
                }
            };

            tracing::info!("Tool execution completed: {:?}", tool_ret);

            // Write output to artifact_context if artifact exists
            if artifact.is_some() {
                let error = tool_ret.as_ref().err().map(|e| e.to_string());
                artifact_context
                    .write_kind(EventKind::ArtifactFinished { error })
                    .await?;
            }

            let ToolRawInput {
                call_id,
                handle,
                param,
            } = input;
            return tool_ret.map_err(|err| {
                tracing::error!("Tool execution {} error: {}", handle, err);
                OxyError::ToolCallError {
                    call_id,
                    handle,
                    param,
                    msg: err.to_string(),
                }
            });
        } else {
            let ToolRawInput {
                call_id,
                handle,
                param,
            } = input;
            events::tool::tool_call_error(TOOL_NOT_FOUND_ERR);
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
    dry_run_limit: Option<u64>,
}

#[derive(Clone)]
struct SQLMapper;

#[derive(Clone)]
struct CreateDataAppMapper;

#[derive(Clone)]
struct CreateV0AppMapper;

#[derive(Clone)]
#[allow(dead_code)]
struct OmniQueryMapper;

#[async_trait::async_trait]
impl ParamMapper<SQLToolInput, SQLInput> for SQLMapper {
    async fn map(
        &self,
        _execution_context: &ExecutionContext,
        input: SQLToolInput,
    ) -> Result<(SQLInput, Option<ExecutionContext>), OxyError> {
        let SQLToolInput {
            param,
            database,
            dry_run_limit,
        } = input;
        let SQLParams { sql, persist } = serde_json::from_str::<SQLParams>(&param)?;
        Ok((
            SQLInput {
                sql,
                database,
                dry_run_limit,
                name: None,
                persist,
            },
            None,
        ))
    }
}

#[async_trait::async_trait]
impl ParamMapper<CreateDataAppToolInput, CreateDataAppInput> for CreateDataAppMapper {
    async fn map(
        &self,
        _execution_context: &ExecutionContext,
        input: CreateDataAppToolInput,
    ) -> Result<(CreateDataAppInput, Option<ExecutionContext>), OxyError> {
        let CreateDataAppToolInput { param, .. } = input;
        tracing::debug!("CreateDataAppToolInput param: {}", &param);
        let params = serde_json::from_str::<CreateDataAppParams>(&param)?;
        Ok((CreateDataAppInput { param: params }, None))
    }
}

#[async_trait::async_trait]
impl ParamMapper<CreateV0AppToolInput, CreateV0AppInput> for CreateV0AppMapper {
    async fn map(
        &self,
        _execution_context: &ExecutionContext,
        input: CreateV0AppToolInput,
    ) -> Result<(CreateV0AppInput, Option<ExecutionContext>), OxyError> {
        let CreateV0AppToolInput {
            param,
            github_repo,
            system_instruction,
            oxy_api_key_var,
            v0_api_key_var,
        } = input;
        tracing::debug!("CreateV0AppToolInput param: {}", &param);
        let params = serde_json::from_str::<CreateV0AppParams>(&param)?;
        let oxy_api_key = std::env::var(&oxy_api_key_var).ok().map(SecretString::from);
        let v0_api_key = std::env::var(&v0_api_key_var).map_err(|e| {
            OxyError::ArgumentError(format!(
                "V0 API key not found in environment variable {}: {}",
                v0_api_key_var, e
            ))
        })?;
        Ok((
            CreateV0AppInput {
                name: params.name,
                prompt: params.prompt,
                system_instruction,
                github_repo,
                oxy_api_key,
                v0_api_key: SecretString::from(v0_api_key),
            },
            None,
        ))
    }
}

fn build_sql_executable<E>(executable: E) -> impl Executable<SQLToolInput, Response = Table>
where
    E: Executable<SQLInput, Response = Table> + Send,
{
    ExecutableBuilder::new()
        .map(SQLMapper)
        .executable(executable)
}
fn build_validate_sql_executable<E>(
    executable: E,
) -> impl Executable<SQLToolInput, Response = Output>
where
    E: Executable<SQLInput, Response = Output> + Send,
{
    ExecutableBuilder::new()
        .map(SQLMapper)
        .executable(executable)
}
fn build_omni_query_tool_executable() -> impl Executable<OmniQueryInput, Response = Output> {
    // OmniQuery doesn't need a mapper - just use executable directly
    OmniQueryExecutable::new()
}

fn build_visualize_executable() -> impl Executable<VisualizeParams, Response = Output> {
    // Visualize doesn't need a mapper - just use executable directly
    VisualizeExecutable::new()
}

// Removed: SemanticQueryMapper - requires workflow crate types

struct CreateDataAppToolInput {
    param: String,
}

#[derive(Clone)]
struct CreateV0AppToolInput {
    param: String,
    system_instruction: String,
    github_repo: Option<String>,
    oxy_api_key_var: String,
    v0_api_key_var: String,
}

// Removed: SemanticQueryToolInput, VisualizeToolInput, VisualizeMapper - tools removed in refactor

fn build_create_data_app_executable() -> impl Executable<CreateDataAppToolInput, Response = Output>
{
    ExecutableBuilder::new()
        .map(CreateDataAppMapper)
        .executable(CreateDataAppExecutable {})
}

fn build_create_v0_app_executable() -> impl Executable<CreateV0AppToolInput, Response = Output> {
    ExecutableBuilder::new()
        .map(CreateV0AppMapper)
        .executable(CreateV0App)
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
                query,
                agent_name,
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

fn create_semantic_query_artifact(param_obj: &Value) -> SemanticQuery {
    let extract_string_array = |key: &str| {
        param_obj
            .get(key)
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|val| val.as_str().map(|s| s.to_string()))
            .collect()
    };

    SemanticQuery {
        database: "".to_string(),
        sql_query: "".to_string(),
        result: vec![],
        error: None,
        validation_error: None,
        sql_generation_error: None,
        is_result_truncated: false,
        topic: param_obj
            .get("topic")
            .and_then(|v| v.as_str().map(|s| s.to_string())),
        dimensions: extract_string_array("dimensions"),
        measures: extract_string_array("measures"),
        time_dimensions: param_obj
            .get("time_dimensions")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|val| serde_json::from_value(val).ok())
            .collect(),
        filters: param_obj
            .get("filters")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|val| serde_json::from_value(val).ok())
            .collect(),
        orders: param_obj
            .get("orders")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|val| serde_json::from_value(val).ok())
            .collect(),
        limit: param_obj.get("limit").and_then(|v| v.as_u64()),
        offset: param_obj.get("offset").and_then(|v| v.as_u64()),
    }
}

// Note: Workflow, Agent, and SemanticQuery tool executors are registered
// at the application level via the ToolRegistry to avoid circular dependencies.
// See crates/core/src/tools/registry.rs for the registration interface.
