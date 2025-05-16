use crate::{
    adapters::checkpoint::{CheckpointBuilder, CheckpointManager},
    config::{
        constants::{CHECKPOINT_ROOT_PATH, WORKFLOW_SOURCE},
        model::Task,
    },
    errors::OxyError,
    execute::{
        Executable, ExecutionContext, ExecutionContextBuilder,
        builders::checkpoint::{LastRunFailed, NoRestore},
        types::{EventKind, OutputContainer, Source},
        writer::{BufWriter, EventHandler},
    },
    theme::StyledText,
};
use itertools::Itertools;
use minijinja::Value;
use serde_json::Value as JsonValue;
use std::{
    collections::HashMap,
    hash::Hash,
    path::{Path, PathBuf},
};
use workflow::{build_tasks_executable, build_workflow_executable};

mod cache;
mod consistency;
mod export;
mod loop_concurrency;
mod sql;
mod task;
mod template;
mod workflow;

#[derive(Clone, Debug)]
pub struct WorkflowInput {
    pub restore_from_checkpoint: bool,
    pub workflow_ref: String,
    pub variables: Option<HashMap<String, JsonValue>>,
}

impl Hash for WorkflowInput {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.workflow_ref.hash(state);
        if let Some(variables) = &self.variables {
            for (key, value) in variables.iter().sorted_by_cached_key(|(k, _)| *k) {
                key.hash(state);
                value.hash(state);
            }
        }
    }
}

pub struct WorkflowLauncher {
    checkpoint_manager: Option<CheckpointManager>,
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
            checkpoint_manager: None,
            execution_context: None,
            buf_writer: BufWriter::new(),
        }
    }

    pub async fn with_external_context(
        mut self,
        execution_context: &ExecutionContext,
    ) -> Result<Self, OxyError> {
        let tx = self.buf_writer.create_writer(None)?;
        self.execution_context = Some(execution_context.wrap_writer(tx));
        self.checkpoint_manager = CheckpointBuilder::from_config(&execution_context.config)
            .await
            .ok();
        Ok(self)
    }

    pub async fn with_local_context<P: AsRef<Path>>(
        mut self,
        project_path: P,
    ) -> Result<Self, OxyError> {
        let checkpoint_path = project_path.as_ref().join(CHECKPOINT_ROOT_PATH);
        let tx = self.buf_writer.create_writer(None)?;
        self.execution_context = Some(
            ExecutionContextBuilder::new()
                .with_project_path(project_path)
                .await?
                .with_global_context(Value::UNDEFINED)
                .with_writer(tx)
                .with_source(Source {
                    parent_id: None,
                    id: "workflow".to_string(),
                    kind: WORKFLOW_SOURCE.to_string(),
                })
                .build()?,
        );
        self.checkpoint_manager = Some(
            CheckpointBuilder::new()
                .with_local_path(checkpoint_path)
                .build()?,
        );
        Ok(self)
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
        let checkpoint_manager = self.checkpoint_manager.ok_or(OxyError::RuntimeError(
            "CheckpointManager is required".to_string(),
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
        execution_context
            .write_kind(EventKind::Started {
                name: workflow_name,
            })
            .await?;
        let handle = tokio::spawn(async move {
            let WorkflowInput {
                workflow_ref,
                variables,
                restore_from_checkpoint,
            } = workflow_input;
            let response = match restore_from_checkpoint {
                true => {
                    let mut executable =
                        build_workflow_executable(checkpoint_manager, LastRunFailed);
                    executable
                        .execute(&execution_context, (workflow_ref, variables))
                        .await
                }
                false => {
                    let mut executable = build_workflow_executable(checkpoint_manager, NoRestore);
                    executable
                        .execute(&execution_context, (workflow_ref, variables))
                        .await
                }
            }?;
            execution_context
                .write_kind(EventKind::Finished {
                    message: "\n✅Workflow executed successfully".success().to_string(),
                })
                .await?;
            Ok(response)
        });
        let buf_writer = self.buf_writer;
        let event_handle =
            tokio::spawn(async move { buf_writer.write_to_handler(event_handler).await });
        let response = handle.await?;
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
        let checkpoint_manager = self.checkpoint_manager.ok_or(OxyError::RuntimeError(
            "CheckpointManager is required".to_string(),
        ))?;
        execution_context
            .write_kind(EventKind::Started {
                name: "tasks".to_string(),
            })
            .await?;
        let handle = tokio::spawn(async move {
            let mut executable = build_tasks_executable(checkpoint_manager, NoRestore);
            let response = executable.execute(&execution_context, tasks).await;
            execution_context
                .write_kind(EventKind::Finished {
                    message: "\n✅Workflow executed successfully".success().to_string(),
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
