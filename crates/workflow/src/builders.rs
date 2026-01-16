use indexmap::IndexMap;
use minijinja::context;
use std::{collections::HashMap, path::PathBuf};
use tracing::Instrument;

use crate::workflow_builder::build_workflow_executable;

use oxy::{
    adapters::{
        project::manager::ProjectManager, runs::RunsManager, secrets::SecretsManager,
        session_filters::SessionFilters,
    },
    checkpoint::{RunInfo, types::RetryStrategy},
    config::{
        ConfigManager,
        constants::WORKFLOW_SOURCE,
        model::{ConnectionOverrides, Task},
    },
    execute::{
        Executable, ExecutionContext, ExecutionContextBuilder,
        types::{EventKind, OutputContainer, Source},
        writer::{BufWriter, EventHandler},
    },
    observability::events::workflow as workflow_events,
    semantic::SemanticManager,
    theme::StyledText,
    utils::file_path_to_source_id,
};
use oxy_shared::errors::OxyError;

#[derive(Clone, Debug)]
pub struct WorkflowInput {
    pub workflow_ref: String,
    pub retry: RetryStrategy,
}

pub struct WorkflowLauncher {
    execution_context: Option<ExecutionContext>,
    buf_writer: BufWriter,
    filters: Option<SessionFilters>,
    connections: Option<ConnectionOverrides>,
    globals: Option<indexmap::IndexMap<String, serde_json::Value>>,
}

impl Default for WorkflowLauncher {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowLauncher {
    pub fn new() -> Self {
        Self {
            execution_context: None,
            buf_writer: BufWriter::new(),
            filters: None,
            connections: None,
            globals: None,
        }
    }

    pub fn with_filters(mut self, filters: impl Into<Option<SessionFilters>>) -> Self {
        self.filters = filters.into();
        self
    }

    pub fn with_connections(mut self, connections: impl Into<Option<ConnectionOverrides>>) -> Self {
        self.connections = connections.into();
        self
    }

    pub fn with_globals(
        mut self,
        globals: impl Into<Option<indexmap::IndexMap<String, serde_json::Value>>>,
    ) -> Self {
        self.globals = globals.into();
        self
    }

    #[tracing::instrument(skip_all, err, fields(
        otel.name = workflow_events::launcher::get_global_context::NAME,
        oxy.span_type = workflow_events::launcher::get_global_context::TYPE,
    ))]
    async fn get_global_context(
        &self,
        config: ConfigManager,
        secrets_manager: SecretsManager,
    ) -> Result<minijinja::Value, OxyError> {
        workflow_events::launcher::get_global_context::input();

        let mut semantic_manager =
            SemanticManager::from_config(config, secrets_manager, false).await?;

        // Apply global overrides to the GlobalRegistry before loading semantics
        if let Some(globals) = &self.globals {
            semantic_manager.set_global_overrides(globals.clone())?;
        }

        let semantic_variables_contexts =
            semantic_manager.get_semantic_variables_contexts().await?;
        let semantic_dimensions_contexts = semantic_manager
            .get_semantic_dimensions_contexts(&semantic_variables_contexts)
            .await?;

        // Get globals from the semantic manager
        let globals_value = semantic_manager.get_globals_value()?;

        // Convert serde_yaml::Value to minijinja::Value
        let globals = minijinja::Value::from_serialize(&globals_value);

        let global_context = context! {
            models => minijinja::Value::from_object(semantic_variables_contexts.clone()),
            dimensions => minijinja::Value::from_object(semantic_dimensions_contexts.clone()),
            globals => globals,
        };

        workflow_events::launcher::get_global_context::output(
            true, // models context is always created
            !semantic_dimensions_contexts.dimensions.is_empty(),
            !globals_value.is_null(),
        );

        Ok(global_context)
    }

    pub async fn with_external_context(
        mut self,
        execution_context: &ExecutionContext,
    ) -> Result<Self, OxyError> {
        let config_manager = execution_context.project.config_manager.clone();
        let secrets_manager = execution_context.project.secrets_manager.clone();
        let tx = self.buf_writer.create_writer(None)?;
        let global_context = self
            .get_global_context(config_manager, secrets_manager)
            .await?;
        self.execution_context = Some(
            execution_context
                .wrap_writer(tx)
                .wrap_global_context(global_context),
        );
        Ok(self)
    }

    #[tracing::instrument(skip_all, err, fields(
        otel.name = workflow_events::launcher::with_project::NAME,
        oxy.span_type = workflow_events::launcher::with_project::TYPE,
    ))]
    pub async fn with_project(mut self, project_manager: ProjectManager) -> Result<Self, OxyError> {
        workflow_events::launcher::with_project::input(
            project_manager
                .config_manager
                .project_path()
                .to_str()
                .unwrap_or("unknown"),
        );

        let tx = self.buf_writer.create_writer(None)?;
        let mut execution_context = ExecutionContextBuilder::new()
            .with_project_manager(project_manager)
            .with_global_context(minijinja::Value::UNDEFINED)
            .with_writer(tx)
            .with_source(Source {
                parent_id: None,
                id: "workflow".to_string(),
                kind: WORKFLOW_SOURCE.to_string(),
            })
            .with_filters(self.filters.clone())
            .with_connections(self.connections.clone())
            .build()?;

        let config_manager = execution_context.project.config_manager.clone();
        let secrets_manager = execution_context.project.secrets_manager.clone();

        let global_context = self
            .get_global_context(config_manager, secrets_manager)
            .await?;
        execution_context = execution_context.wrap_global_context(global_context);
        self.execution_context = Some(execution_context);

        workflow_events::launcher::with_project::output();
        Ok(self)
    }

    async fn get_run_info(
        workflow_ref: &str,
        retry: &RetryStrategy,
        runs_manager: &RunsManager,
        user_id: Option<uuid::Uuid>,
    ) -> Result<RunInfo, OxyError> {
        let source_id = file_path_to_source_id(workflow_ref);
        match retry {
            RetryStrategy::Retry {
                replay_id,
                run_index,
            } => {
                let run_info = runs_manager
                    .find_run(
                        &source_id,
                        Some((*run_index).try_into().map_err(|_| {
                            OxyError::RuntimeError("Run index conversion failed".to_string())
                        })?),
                    )
                    .await?;
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
            RetryStrategy::RetryWithVariables {
                replay_id,
                run_index,
                variables,
            } => {
                let run_info = runs_manager
                    .update_run_variables(
                        &source_id,
                        (*run_index).try_into().map_err(|_| {
                            OxyError::RuntimeError("Run index conversion failed".to_string())
                        })?,
                        variables.clone(),
                    )
                    .await?;
                let mut run_info: RunInfo = run_info.try_into()?;
                run_info.set_replay_id(replay_id.clone());
                Ok(run_info)
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
            RetryStrategy::NoRetry { variables } => runs_manager
                .new_run(&source_id, variables.clone(), None, user_id)
                .await
                .map(|run| run.try_into())?,
            RetryStrategy::Preview => {
                todo!("Preview mode is not implemented yet")
            }
        }
    }

    #[tracing::instrument(skip_all, err, fields(
        otel.name = workflow_events::launcher::launch::NAME,
        oxy.span_type = workflow_events::launcher::launch::TYPE,
        oxy.workflow.ref = %workflow_input.workflow_ref,
    ))]
    pub async fn launch<H: EventHandler + Send + 'static>(
        self,
        workflow_input: WorkflowInput,
        event_handler: H,
        user_id: Option<uuid::Uuid>,
    ) -> Result<OutputContainer, OxyError> {
        workflow_events::launcher::launch::input(
            &workflow_input.workflow_ref,
            &format!("{:?}", workflow_input.retry),
        );

        let mut execution_context = self.execution_context.ok_or(OxyError::RuntimeError(
            "ExecutionContext is required".to_string(),
        ))?;

        // Update user_id if provided and create child source
        execution_context = if let Some(uid) = user_id {
            execution_context.with_user_id(Some(uid))
        } else {
            execution_context
        };
        execution_context = execution_context.with_child_source(
            workflow_input.workflow_ref.to_string(),
            WORKFLOW_SOURCE.to_string(),
        );

        let runs_manager =
            execution_context
                .project
                .runs_manager
                .clone()
                .ok_or(OxyError::RuntimeError(
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
            user_id,
        )
        .await?;
        let workflow_config = execution_context
            .project
            .config_manager
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
                retry: _,
            } = workflow_input;
            let response = {
                let mut executable = build_workflow_executable();
                executable
                    .execute(&execution_context, (workflow_ref, run_info))
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

        match &response {
            Ok(output) => workflow_events::launcher::launch::output(output),
            Err(e) => workflow_events::launcher::launch::error(&e.to_string()),
        }

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

        // Capture the current span to propagate trace context to the spawned task
        let current_span = tracing::Span::current();

        // Convert tasks to TaskInput
        let task_inputs: Vec<crate::task_builder::TaskInput> = tasks
            .into_iter()
            .map(|task| crate::task_builder::TaskInput {
                loop_idx: None,
                task,
                value: None,
                runtime_input: None,
                workflow_consistency_prompt: None,
            })
            .collect();

        let handle = tokio::spawn(
            async move {
                use oxy::execute::Executable;
                let mut task_executable = crate::task_builder::TaskExecutable;

                // Execute all tasks
                let mut results = IndexMap::new();
                for task_input in task_inputs {
                    let task_name = task_input.task.name.clone();
                    match task_executable
                        .execute(&execution_context, task_input)
                        .await
                    {
                        Ok(result) => {
                            results.insert(task_name, result);
                        }
                        Err(e) => {
                            execution_context
                                .write_kind(EventKind::Finished {
                                    attributes: Default::default(),
                                    message: "\n❌Workflow failed".to_string(),
                                    error: Some(e.to_string()),
                                })
                                .await?;
                            return Err(e);
                        }
                    }
                }

                execution_context
                    .write_kind(EventKind::Finished {
                        attributes: Default::default(),
                        message: "\n✅Workflow executed successfully".success().to_string(),
                        error: None,
                    })
                    .await?;

                // Return the last result or a default OutputContainer
                Ok(OutputContainer::Map(results))
            }
            .instrument(current_span),
        );
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
            .launch(
                input,
                execution_context.writer.clone(),
                execution_context.user_id,
            )
            .await
    }
}
