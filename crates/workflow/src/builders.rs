use indexmap::IndexMap;
use minijinja::context;
use std::{collections::HashMap, path::PathBuf};
use tracing::Instrument;

use crate::{task_builder::create_runtime_input, workflow_builder::build_workflow_executable};

use oxy::{
    adapters::{
        runs::RunsManager, secrets::SecretsManager, session_filters::SessionFilters,
        workspace::manager::WorkspaceManager,
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
    metrics::{MetricContext, SourceType},
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
    controls: HashMap<String, serde_json::Value>,
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
            controls: HashMap::new(),
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

    pub fn with_controls(mut self, controls: HashMap<String, serde_json::Value>) -> Self {
        self.controls = controls;
        self
    }

    #[tracing::instrument(skip_all, err, fields(
        oxy.name = workflow_events::launcher::get_global_context::NAME,
        oxy.span_type = workflow_events::launcher::get_global_context::TYPE,
    ))]
    async fn get_global_context(
        &self,
        config: ConfigManager,
        secrets_manager: SecretsManager,
    ) -> Result<minijinja::Value, OxyError> {
        workflow_events::launcher::get_global_context::input();

        let semantic_manager = SemanticManager::from_config(config, secrets_manager, false).await?;

        let semantic_variables_contexts =
            semantic_manager.get_semantic_variables_contexts().await?;
        let semantic_dimensions_contexts =
            semantic_manager.get_semantic_dimensions_contexts().await?;

        let controls = minijinja::Value::from_serialize(&self.controls);
        let global_context = context! {
            models => minijinja::Value::from_object(semantic_variables_contexts.clone()),
            dimensions => minijinja::Value::from_object(semantic_dimensions_contexts.clone()),
            controls => controls,
        };

        workflow_events::launcher::get_global_context::output(
            true, // models context is always created
            !semantic_dimensions_contexts.dimensions.is_empty(),
        );

        Ok(global_context)
    }

    pub async fn with_external_context(
        mut self,
        execution_context: &ExecutionContext,
    ) -> Result<Self, OxyError> {
        let config_manager = execution_context.workspace.config_manager.clone();
        let secrets_manager = execution_context.workspace.secrets_manager.clone();
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
        oxy.name = workflow_events::launcher::with_workspace::NAME,
        oxy.span_type = workflow_events::launcher::with_workspace::TYPE,
    ))]
    pub async fn with_workspace(
        mut self,
        workspace_manager: WorkspaceManager,
    ) -> Result<Self, OxyError> {
        workflow_events::launcher::with_workspace::input(
            workspace_manager
                .config_manager
                .workspace_path()
                .to_str()
                .unwrap_or("unknown"),
        );

        let tx = self.buf_writer.create_writer(None)?;
        let mut execution_context = ExecutionContextBuilder::new()
            .with_workspace_manager(workspace_manager)
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

        let config_manager = execution_context.workspace.config_manager.clone();
        let secrets_manager = execution_context.workspace.secrets_manager.clone();

        let global_context = self
            .get_global_context(config_manager, secrets_manager)
            .await?;
        execution_context = execution_context.wrap_global_context(global_context);
        self.execution_context = Some(execution_context);

        workflow_events::launcher::with_workspace::output();
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
        oxy.name = workflow_events::launcher::launch::NAME,
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

        // Create execution context with metric context for this workflow execution
        // Note: We must not keep base_context alive after creating execution_context,
        // because it holds a clone of the writer channel sender. If base_context stays
        // alive, event_handle.await will hang forever waiting for all senders to drop.
        let execution_context = {
            let base_context = self.execution_context.ok_or(OxyError::RuntimeError(
                "ExecutionContext is required".to_string(),
            ))?;

            // Update user_id if provided
            let ctx = if let Some(uid) = user_id {
                base_context.with_user_id(Some(uid))
            } else {
                base_context
            };

            // Create metric context for this workflow execution
            // If parent has metric context, create child; otherwise create new root
            if ctx.metric_context.is_some() {
                ctx.with_child_source(
                    workflow_input.workflow_ref.to_string(),
                    WORKFLOW_SOURCE.to_string(),
                )
                .with_child_metric_context(SourceType::Workflow, &workflow_input.workflow_ref)
            } else {
                let metric_ctx =
                    MetricContext::new(SourceType::Workflow, workflow_input.workflow_ref.clone())
                        .shared();
                ctx.with_child_source(
                    workflow_input.workflow_ref.to_string(),
                    WORKFLOW_SOURCE.to_string(),
                )
                .with_metric_context(metric_ctx)
            }
            // base_context and ctx are dropped here at end of block
        };

        let runs_manager =
            execution_context
                .workspace
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
            .workspace
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

            // Record output/error via observability events (handles metrics finalization)
            match &response {
                Ok(output) => {
                    workflow_events::launcher::launch::output(&execution_context, output);
                }
                Err(e) => {
                    workflow_events::launcher::launch::error(&execution_context, &e.to_string());
                }
            }

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

        // Capture the current span to propagate trace context to the spawned task
        let current_span = tracing::Span::current();

        // Convert tasks to TaskInput with proper runtime_input
        let mut task_inputs = Vec::new();
        for task in tasks {
            // Use the shared helper to create runtime_input
            // No checkpoint context in launch_tasks, so replay_id is None
            let runtime_input =
                create_runtime_input(&task.task_type, &execution_context, None).await?;

            task_inputs.push(crate::task_builder::TaskInput {
                loop_idx: None,
                task,
                value: None,
                runtime_input,
                workflow_consistency_prompt: None,
            });
        }

        let handle = tokio::spawn(
            async move {
                use oxy::execute::Executable;
                let mut task_executable = crate::task_builder::TaskExecutable;

                // Execute all tasks
                let mut results = IndexMap::new();
                let all_tasks_start = std::time::Instant::now();
                for task_input in task_inputs {
                    // Override result data so it can be referenced by subsequent tasks
                    let ctx_start = std::time::Instant::now();
                    let current_context = minijinja::Value::from_iter(
                        results
                            .iter()
                            .map(|(k, v)| (k, Into::<minijinja::Value>::into(v)))
                            .collect::<Vec<_>>(),
                    );
                    let execution_context = execution_context.wrap_render_context(&current_context);
                    tracing::info!(
                        elapsed_ms = ctx_start.elapsed().as_millis(),
                        result_count = results.len(),
                        "⏱ jinja context update"
                    );
                    let task_name = task_input.task.name.clone();
                    let task_start = std::time::Instant::now();
                    match task_executable
                        .execute(&execution_context, task_input)
                        .await
                    {
                        Ok(result) => {
                            tracing::info!(
                                elapsed_ms = task_start.elapsed().as_millis(),
                                task = %task_name,
                                "⏱ task completed"
                            );
                            results.insert(task_name, result);
                        }
                        Err(e) => {
                            tracing::info!(
                                elapsed_ms = task_start.elapsed().as_millis(),
                                task = %task_name,
                                "⏱ task failed"
                            );
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
                tracing::info!(
                    elapsed_ms = all_tasks_start.elapsed().as_millis(),
                    task_count = results.len(),
                    "⏱ all tasks completed"
                );

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
