use std::collections::HashMap;

use super::{
    create_data_app::CreateDataAppExecutable,
    retrieval::RetrievalExecutable,
    sql::{SQLExecutable, ValidateSQLExecutable},
    types::{
        AgentParams, CreateDataAppInput, RetrievalInput, RetrievalParams, SQLInput, SQLParams,
        ToolRawInput, VisualizeInput, WorkflowInput,
    },
    visualize::{types::VisualizeParams, visualize::VisualizeExecutable},
    workflow::WorkflowExecutable,
};
use crate::{
    adapters::openai::OpenAIToolConfig,
    agent::{AgentLauncherExecutable, types::AgentInput},
    config::{
        constants::ARTIFACT_SOURCE,
        model::{
            AgentTool, RetrievalConfig, SemanticQueryTask, ToolType, WorkflowTool,
            omni::OmniSemanticModel,
        },
    },
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, map::ParamMapper},
        types::{EventKind, Output, OutputContainer},
    },
    service::types::SemanticQueryParams,
    tools::{
        create_data_app::types::CreateDataAppParams,
        omni::{OmniExecutable, OmniTopicInfoExecutable},
        types::{ExecuteOmniParams, OmniInput, OmniTopicInfoInput, OmniTopicInfoParams},
    },
    workflow::{SemanticQueryExecutable, ValidatedSemanticQuery, validate_semantic_query_task},
};

const TOOL_NOT_FOUND_ERR: &str = "Tool not found";

#[derive(Clone)]
pub struct ToolExecutable;

#[derive(Clone)]
struct OmniToolInput {
    database: String,
    semantic_model: OmniSemanticModel,
    param: String,
}

#[derive(Clone)]
struct OmniTopicInfoToolInput {
    param: String,
    semantic_model: OmniSemanticModel,
}

#[async_trait::async_trait]
impl ParamMapper<OmniTopicInfoToolInput, OmniTopicInfoInput> for OmniTopicInfoMapper {
    async fn map(
        &self,
        _execution_context: &ExecutionContext,
        input: OmniTopicInfoToolInput,
    ) -> Result<(OmniTopicInfoInput, Option<ExecutionContext>), OxyError> {
        let OmniTopicInfoToolInput {
            param,
            semantic_model,
        } = input;
        let omni_params = serde_json::from_str::<OmniTopicInfoParams>(&param)?;
        Ok((
            OmniTopicInfoInput {
                semantic_model,
                topic: omni_params.topic,
            },
            None,
        ))
    }
}

#[async_trait::async_trait]
impl ParamMapper<OmniToolInput, OmniInput> for OmniMapper {
    async fn map(
        &self,
        _execution_context: &ExecutionContext,
        input: OmniToolInput,
    ) -> Result<(OmniInput, Option<ExecutionContext>), OxyError> {
        let OmniToolInput {
            param,
            database,
            semantic_model,
        } = input;
        let omni_params = serde_json::from_str::<ExecuteOmniParams>(&param)?;
        Ok((
            OmniInput {
                database,
                params: omni_params,
                semantic_model,
            },
            None,
        ))
    }
}

fn build_omni_executable<E>(executable: E) -> impl Executable<OmniToolInput, Response = Output>
where
    E: Executable<OmniInput, Response = Output> + Send,
{
    ExecutableBuilder::new()
        .map(OmniMapper)
        .executable(executable)
}

fn build_omni_topic_info_executable() -> impl Executable<OmniTopicInfoToolInput, Response = Output>
{
    ExecutableBuilder::new()
        .map(OmniTopicInfoMapper)
        .executable(OmniTopicInfoExecutable {})
}

#[async_trait::async_trait]
impl Executable<(String, Option<ToolType>, ToolRawInput)> for ToolExecutable {
    type Response = OutputContainer;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: (String, Option<ToolType>, ToolRawInput),
    ) -> Result<Self::Response, OxyError> {
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
                _ => false,
            };

            let artifact = tool_type.artifact();
            if let Some((title, kind)) = &artifact {
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
                            serde_json::to_string(&SQLParams { sql: sql.clone() })?,
                            sql.clone(),
                        ),
                        None => {
                            let SQLParams { sql } =
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
                    build_sql_executable(ValidateSQLExecutable::new())
                        .execute(
                            execution_context,
                            SQLToolInput {
                                database: sql_config.database.clone(),
                                param: input.param.clone(),
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
                ToolType::Workflow(workflow_config) => {
                    build_workflow_executable()
                        .execute(
                            execution_context,
                            WorkflowToolInput {
                                workflow_config: workflow_config.clone(),
                                param: input.param.clone(),
                            },
                        )
                        .await
                }
                ToolType::Agent(agent_config) => {
                    build_agent_executable()
                        .execute(
                            execution_context,
                            AgentToolInput {
                                agent_config: agent_config.clone(),
                                param: input.param.clone(),
                            },
                        )
                        .await
                }
                ToolType::Visualize(_visualize_config) => build_visualize_executable()
                    .execute(
                        execution_context,
                        VisualizeToolInput {
                            param: input.param.clone(),
                        },
                    )
                    .await
                    .map(|output| output.into()),
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
                ToolType::SemanticQuery(semantic_query_tool) => {
                    let semantic_params =
                        serde_json::from_str::<SemanticQueryParams>(&input.param)?;
                    build_semantic_query_executable()
                        .execute(
                            execution_context,
                            SemanticQueryToolInput {
                                param: semantic_params,
                                topic: semantic_query_tool.topic.clone(),
                            },
                        )
                        .await
                        .map(|output| output.into())
                }
                ToolType::ExecuteOmni(execute_omni_tool) => {
                    let semantic_model = execute_omni_tool.load_semantic_model()?;
                    build_omni_executable(OmniExecutable::new())
                        .execute(
                            execution_context,
                            OmniToolInput {
                                database: execute_omni_tool.database.clone(),
                                param: input.param.clone(),
                                semantic_model,
                            },
                        )
                        .await
                        .map(|output| output.into())
                }
                ToolType::OmniTopicInfo(omni_topic_info_tool) => {
                    let semantic_model = omni_topic_info_tool.load_semantic_model()?;
                    build_omni_topic_info_executable()
                        .execute(
                            execution_context,
                            OmniTopicInfoToolInput {
                                param: input.param.clone(),
                                semantic_model,
                            },
                        )
                        .await
                        .map(|output| output.into())
                }
            };

            if artifact.is_some() {
                artifact_context
                    .write_kind(EventKind::ArtifactFinished)
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
struct OmniMapper;

#[derive(Clone)]
struct OmniTopicInfoMapper;

#[derive(Clone)]
struct CreateDataAppMapper;

#[derive(Clone)]
struct SemanticQueryMapper;

#[derive(Clone)]
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
        let SQLParams { sql } = serde_json::from_str::<SQLParams>(&param)?;
        Ok((
            SQLInput {
                sql,
                database,
                dry_run_limit,
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

fn build_sql_executable<E>(executable: E) -> impl Executable<SQLToolInput, Response = Output>
where
    E: Executable<SQLInput, Response = Output> + Send,
{
    ExecutableBuilder::new()
        .map(SQLMapper)
        .executable(executable)
}

#[async_trait::async_trait]
impl ParamMapper<SemanticQueryToolInput, ValidatedSemanticQuery> for SemanticQueryMapper {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: SemanticQueryToolInput,
    ) -> Result<(ValidatedSemanticQuery, Option<ExecutionContext>), OxyError> {
        let SemanticQueryToolInput {
            param: semantic_params,
            topic,
        } = input;

        if let Some(ref topic) = topic {
            if topic.to_string() != semantic_params.topic {
                return Err(OxyError::ArgumentError(format!(
                    "Topic mismatch: expected '{}', got '{}'",
                    topic, semantic_params.topic
                )));
            }
        } else {
            tracing::debug!("SemanticQueryToolInput topic: None");
        }

        // Use the structured parameters directly (no JSON parsing needed)
        // SemanticQueryParams already contains the correct types, no conversion needed

        // For now, semantic query tools don't support export directly
        // Export functionality is handled at the task level
        let export = None;

        // Create a SemanticQueryTask from the parameters
        let task = SemanticQueryTask {
            query: semantic_params,
            export,
        };

        // Validate the semantic query task
        let validated = validate_semantic_query_task(&execution_context.config, &task).await?;

        Ok((validated, None))
    }
}

fn build_semantic_query_executable() -> impl Executable<SemanticQueryToolInput, Response = Output> {
    ExecutableBuilder::new()
        .map(SemanticQueryMapper)
        .executable(SemanticQueryExecutable::new())
}

#[derive(Clone)]
struct VisualizeToolInput {
    param: String,
}

struct CreateDataAppToolInput {
    param: String,
}

#[derive(Clone)]
struct SemanticQueryToolInput {
    param: SemanticQueryParams,
    topic: Option<String>,
}

#[derive(Clone)]
struct VisualizeMapper;

fn build_visualize_executable() -> impl Executable<VisualizeToolInput, Response = Output> {
    ExecutableBuilder::new()
        .map(VisualizeMapper)
        .executable(VisualizeExecutable::new())
}

fn build_create_data_app_executable() -> impl Executable<CreateDataAppToolInput, Response = Output>
{
    ExecutableBuilder::new()
        .map(CreateDataAppMapper)
        .executable(CreateDataAppExecutable {})
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
impl ParamMapper<RetrievalToolInput, RetrievalInput<RetrievalConfig>> for RetrievalMapper {
    async fn map(
        &self,
        _execution_context: &ExecutionContext,
        input: RetrievalToolInput,
    ) -> Result<(RetrievalInput<RetrievalConfig>, Option<ExecutionContext>), OxyError> {
        let RetrievalToolInput {
            agent_name,
            param,
            retrieval_config,
        } = input;
        let query = match serde_json::from_str::<RetrievalParams>(&param) {
            Ok(RetrievalParams { query }) => query,
            Err(_) => param,
        };
        let embedding_config = retrieval_config.embedding_config.clone();
        Ok((
            RetrievalInput {
                query,
                db_config: retrieval_config.db_config.clone(),
                db_name: format!("{}-{}", agent_name, retrieval_config.name),
                openai_config: retrieval_config,
                embedding_config,
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

#[derive(Clone)]
struct WorkflowToolInput {
    workflow_config: WorkflowTool,
    param: String,
}

#[derive(Clone)]
struct WorkflowMapper;

#[async_trait::async_trait]
impl ParamMapper<WorkflowToolInput, WorkflowInput> for WorkflowMapper {
    async fn map(
        &self,
        _execution_context: &ExecutionContext,
        input: WorkflowToolInput,
    ) -> Result<(WorkflowInput, Option<ExecutionContext>), OxyError> {
        let WorkflowToolInput { param, .. } = input;
        if param.is_empty() {
            return Ok((
                WorkflowInput {
                    workflow_config: input.workflow_config,
                    variables: None,
                },
                None,
            ));
        }
        let params = serde_json::from_str::<HashMap<String, serde_json::Value>>(&param)?;
        Ok((
            WorkflowInput {
                workflow_config: input.workflow_config,
                variables: Some(params),
            },
            None,
        ))
    }
}

fn build_workflow_executable() -> impl Executable<WorkflowToolInput, Response = OutputContainer> {
    ExecutableBuilder::new()
        .map(WorkflowMapper)
        .executable(WorkflowExecutable::new())
}

#[derive(Clone)]
pub struct AgentToolInput {
    pub agent_config: AgentTool,
    pub param: String,
}

#[derive(Clone)]
pub struct AgentMapper;

#[async_trait::async_trait]
impl ParamMapper<AgentToolInput, AgentInput> for AgentMapper {
    async fn map(
        &self,
        _execution_context: &ExecutionContext,
        input: AgentToolInput,
    ) -> Result<(AgentInput, Option<ExecutionContext>), OxyError> {
        let AgentToolInput {
            param,
            agent_config,
        } = input;
        let params = serde_json::from_str::<AgentParams>(&param)?;
        Ok((
            AgentInput {
                agent_ref: agent_config.agent_ref.to_string(),
                prompt: params.prompt.to_string(),
                memory: vec![],
            },
            None,
        ))
    }
}

fn build_agent_executable() -> impl Executable<AgentToolInput, Response = OutputContainer> {
    ExecutableBuilder::new()
        .map(AgentMapper)
        .executable(AgentLauncherExecutable)
}
