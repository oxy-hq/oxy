use crate::{
    adapters::{checkpoint::RunInfo, runs::RunsManager},
    config::{ConfigManager, constants::WORKFLOW_SOURCE, model::Task},
    errors::OxyError,
    execute::{
        Executable, ExecutionContext, ExecutionContextBuilder,
        types::{EventKind, OutputContainer, Source},
        writer::{BufWriter, EventHandler},
    },
    semantic::SemanticManager,
    theme::StyledText,
    utils::file_path_to_source_id,
};
use minijinja::context;
use serde_json::Value as JsonValue;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tokio_util::sync::CancellationToken;
use workflow::{build_tasks_executable, build_workflow_executable};

mod cache;
mod consistency;
mod export;
mod loop_concurrency;
mod sql;
mod task;
mod template;
mod workflow;

#[derive(Debug, Clone)]
pub enum RetryStrategy {
    Retry {
        replay_id: Option<String>,
        run_index: u32,
    },
    LastFailure,
    NoRetry,
    Preview,
}

#[derive(Clone, Debug)]
pub struct WorkflowInput {
    pub workflow_ref: String,
    pub variables: Option<HashMap<String, JsonValue>>,
    pub retry: RetryStrategy,
}

pub struct WorkflowLauncher {
    runs_manager: Option<RunsManager>,
    execution_context: Option<ExecutionContext>,
    buf_writer: BufWriter,
}

impl Default for WorkflowLauncher {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowLauncher {
    pub fn new() -> Self {
        Self {
            runs_manager: None,
            execution_context: None,
            buf_writer: BufWriter::new(),
        }
    }

    async fn get_global_context(
        &self,
        config: ConfigManager,
    ) -> Result<minijinja::Value, OxyError> {
        let semantic_manager = SemanticManager::from_config(config, false).await?;
        let semantic_variables_contexts =
            semantic_manager.get_semantic_variables_contexts().await?;
        let semantic_dimensions_contexts = semantic_manager
            .get_semantic_dimensions_contexts(&semantic_variables_contexts)
            .await?;
        let global_context = context! {
            models => minijinja::Value::from_object(semantic_variables_contexts),
            dimensions => minijinja::Value::from_object(semantic_dimensions_contexts),
        };
        Ok(global_context)
    }

    pub async fn with_external_context(
        mut self,
        execution_context: &ExecutionContext,
    ) -> Result<Self, OxyError> {
        let tx = self.buf_writer.create_writer(None)?;
        let global_context = self
            .get_global_context(execution_context.config.clone())
            .await?;
        self.execution_context = Some(
            execution_context
                .wrap_writer(tx)
                .wrap_global_context(global_context),
        );
        self.runs_manager = Some(RunsManager::default().await?);
        Ok(self)
    }

    pub async fn with_local_context<P: AsRef<Path>>(
        mut self,
        project_path: P,
    ) -> Result<Self, OxyError> {
        let tx = self.buf_writer.create_writer(None)?;
        let mut execution_context = ExecutionContextBuilder::new()
            .with_project_path(project_path)
            .await?
            .with_global_context(minijinja::Value::UNDEFINED)
            .with_writer(tx)
            .with_source(Source {
                parent_id: None,
                id: "workflow".to_string(),
                kind: WORKFLOW_SOURCE.to_string(),
            })
            .build()?;
        self.runs_manager = Some(RunsManager::default().await?);

        let global_context = self
            .get_global_context(execution_context.config.clone())
            .await?;
        execution_context = execution_context.wrap_global_context(global_context);
        self.execution_context = Some(execution_context);
        Ok(self)
    }

    async fn get_run_info(
        workflow_ref: &str,
        retry: &RetryStrategy,
        runs_manager: &RunsManager,
    ) -> Result<RunInfo, OxyError> {
        let source_id = file_path_to_source_id(workflow_ref);
        match retry {
            RetryStrategy::Retry {
                replay_id,
                run_index,
            } => {
                let run_info = runs_manager.find_run(&source_id, Some(*run_index)).await?;
                match run_info {
                    Some(run_info) => {
                        let mut run_info: RunInfo = run_info.try_into()?;
                        run_info.set_replay_id(replay_id.clone());
                        Ok(run_info)
                    }
                    None => Err(OxyError::RuntimeError(format!(
                        "Run with index {run_index} not found for workflow {source_id}"
                    ))),
                }
            }
            RetryStrategy::LastFailure => {
                let run_info = runs_manager.last_run(&source_id).await?;
                match run_info {
                    Some(run_info) => {
                        let mut run_info: RunInfo = run_info.try_into()?;
                        run_info.set_replay_id(None);
                        Ok(run_info)
                    }
                    None => Err(OxyError::RuntimeError(format!(
                        "Last failure run not found for workflow {source_id}"
                    ))),
                }
            }
            RetryStrategy::NoRetry => runs_manager
                .new_run(&source_id)
                .await
                .map(|run| run.try_into())?,
            RetryStrategy::Preview => {
                todo!("Preview mode is not implemented yet")
            }
        }
    }

    pub async fn launch<H: EventHandler + Send + 'static>(
        self,
        workflow_input: WorkflowInput,
        event_handler: H,
    ) -> Result<OutputContainer, OxyError> {
        let execution_context = self
            .execution_context
            .ok_or(OxyError::RuntimeError(
                "ExecutionContext is required".to_string(),
            ))?
            .with_child_source(
                workflow_input.workflow_ref.to_string(),
                WORKFLOW_SOURCE.to_string(),
            );
        let runs_manager = self.runs_manager.ok_or(OxyError::RuntimeError(
            "RunsManager is required".to_string(),
        ))?;
        let workflow_name = PathBuf::from(&workflow_input.workflow_ref)
            .file_stem()
            .and_then(|fname| {
                fname
                    .to_string_lossy()
                    .to_string()
                    .split(".")
                    .next()
                    .map(|s| s.to_string())
            })
            .unwrap_or(workflow_input.workflow_ref.to_string());

        let run_info = WorkflowLauncher::get_run_info(
            &workflow_input.workflow_ref,
            &workflow_input.retry,
            &runs_manager,
        )
        .await?;
        let workflow_config = execution_context
            .config
            .resolve_workflow(&workflow_input.workflow_ref)
            .await?;
        let attributes = HashMap::from([
            ("run_id".to_string(), run_info.get_run_index().to_string()),
            (
                "workflow_config".to_string(),
                serde_json::to_string(&workflow_config).unwrap(),
            ),
        ]);
        execution_context
            .write_kind(EventKind::Started {
                name: workflow_name,
                attributes: attributes.clone(),
            })
            .await?;
        let buf_writer = self.buf_writer;
        let event_handle =
            tokio::spawn(async move { buf_writer.write_to_handler(event_handler).await });
        let response = {
            let WorkflowInput {
                workflow_ref,
                variables,
                retry,
            } = workflow_input;
            let response = {
                let mut executable = build_workflow_executable();
                executable
                    .execute(&execution_context, (workflow_ref, variables, run_info))
                    .await
            };
            execution_context
                .write_kind(EventKind::Finished {
                    attributes,
                    message: "\n✅Workflow executed successfully".success().to_string(),
                    error: response.as_ref().err().map(|e| e.to_string()),
                })
                .await?;
            drop(execution_context);
            response
        };
        event_handle.await??;
        response
    }

    pub async fn launch_tasks<H: EventHandler + Send + 'static>(
        self,
        tasks: Vec<Task>,
        event_handler: H,
    ) -> Result<OutputContainer, OxyError> {
        let execution_context = self
            .execution_context
            .ok_or(OxyError::RuntimeError(
                "ExecutionContext is required".to_string(),
            ))?
            .with_child_source("tasks".to_string(), WORKFLOW_SOURCE.to_string());
        execution_context
            .write_kind(EventKind::Started {
                name: "tasks".to_string(),
                attributes: Default::default(),
            })
            .await?;
        let handle = tokio::spawn(async move {
            let mut executable = build_tasks_executable();
            let response = executable.execute(&execution_context, tasks).await;
            execution_context
                .write_kind(EventKind::Finished {
                    attributes: Default::default(),
                    message: "\n✅Workflow executed successfully".success().to_string(),
                    error: response.as_ref().err().map(|e| e.to_string()),
                })
                .await?;
            response
        });
        let buf_writer = self.buf_writer;
        let event_handle =
            tokio::spawn(async move { buf_writer.write_to_handler(event_handler).await });
        let response = handle.await?;
        event_handle.await??;
        response
    }
}

#[derive(Debug, Clone)]
pub struct WorkflowLauncherExecutable;

#[async_trait::async_trait]
impl Executable<WorkflowInput> for WorkflowLauncherExecutable {
    type Response = OutputContainer;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: WorkflowInput,
    ) -> Result<Self::Response, OxyError> {
        WorkflowLauncher::new()
            .with_external_context(execution_context)
            .await?
            .launch(input, execution_context.writer.clone())
            .await
    }
}
