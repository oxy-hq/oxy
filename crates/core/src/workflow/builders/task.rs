use std::{collections::HashMap, hash::Hash};

use itertools::Itertools;
use serde_json::Value as JsonValue;

use crate::{
    adapters::checkpoint::RunInfo,
    agent::{AgentLauncherExecutable, types::AgentInput},
    config::{
        constants::TASK_SOURCE,
        model::{LoopValues, Task, TaskType},
    },
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::{
            ExecutableBuilder,
            cache::Cache,
            chain::{ContextMapper, UpdateInput},
            checkpoint::CheckpointId,
            export::Export,
            map::ParamMapper,
        },
        types::{Chunk, EventKind, Output, OutputContainer},
    },
    theme::StyledText,
    utils::file_path_to_source_id,
    workflow::builders::RetryStrategy,
};

use super::{
    WorkflowInput, WorkflowLauncherExecutable, cache::TaskCacheStorage, consistency::AgentPicker,
    export::TaskExporter, loop_concurrency::build_loop_executable,
    semantic::build_semantic_query_executable, sql::build_sql_task_executable,
};

#[derive(Clone)]
pub(super) struct TaskExecutable;

#[derive(Clone, Debug)]
pub enum RuntimeTaskInput {
    ChildRunInfo {
        run_info: Option<RunInfo>,
        variables: Option<HashMap<String, JsonValue>>,
    },
    Loop {
        values: Vec<minijinja::Value>,
    },
}

#[derive(Debug, Clone)]
pub(super) struct TaskInput {
    pub loop_idx: Option<usize>,
    pub task: Task,
    pub value: Option<OutputContainer>,
    pub runtime_input: Option<RuntimeTaskInput>,
}

impl Hash for TaskInput {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.task.hash(state);
        self.loop_idx.hash(state);
        self.value.hash(state);
    }
}

impl CheckpointId for TaskInput {
    fn checkpoint_hash(&self) -> String {
        let value = fxhash::hash(&self);
        format!("{value:x}")
    }
    fn replay_id(&self) -> String {
        match &self.loop_idx {
            Some(idx) => format!("{}-{}", self.task.name, idx),
            None => self.task.name.clone(),
        }
    }

    fn child_run_info(&self) -> Option<crate::adapters::checkpoint::RunInfo> {
        match &self.runtime_input {
            Some(RuntimeTaskInput::ChildRunInfo { run_info, .. }) => run_info.clone(),
            _ => None,
        }
    }

    fn loop_values(&self) -> Option<Vec<serde_json::Value>> {
        match &self.runtime_input {
            Some(RuntimeTaskInput::Loop { values }) => Some(
                values
                    .iter()
                    .map(|v| serde_json::to_value(v).unwrap_or_default())
                    .collect(),
            ),
            _ => None,
        }
    }
}

impl UpdateInput<OutputContainer> for TaskInput {
    fn update_input(self, value: &OutputContainer) -> Self {
        Self {
            loop_idx: self.loop_idx,
            task: self.task,
            value: Some(value.clone()),
            runtime_input: self.runtime_input,
        }
    }
}

#[async_trait::async_trait]
impl Executable<TaskInput> for TaskExecutable {
    type Response = OutputContainer;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: TaskInput,
    ) -> Result<Self::Response, OxyError> {
        let task_source_id = match &execution_context.checkpoint {
            Some(checkpoint) => {
                tracing::info!("Executing task: {}", checkpoint.current_ref_str(),);
                checkpoint.current_ref_str()
            }
            None => input.replay_id(),
        };
        let TaskInput {
            task,
            value,
            loop_idx: _,
            runtime_input,
        } = input;
        let task_execution_context =
            execution_context.with_child_source(task_source_id.clone(), TASK_SOURCE.to_string());
        task_execution_context
            .write_kind(EventKind::Started {
                name: task.name.to_string(),
                attributes: Default::default(),
            })
            .await?;
        let execution_context = task_execution_context.with_child_source(
            format!("{}-{}", &task.name, fxhash::hash(&value)),
            (&task.kind()).to_string(),
        );

        let new_value = match task.task_type {
            TaskType::Agent(agent_task) => {
                let prompt = execution_context.renderer.render(&agent_task.prompt)?;
                match &agent_task.consistency_run {
                    consistency_run if *consistency_run > 1 => {
                        let mut executable = ExecutableBuilder::new()
                            .consistency(
                                AgentPicker {
                                    task_description: prompt.clone(),
                                    agent_ref: agent_task.agent_ref.to_string(),
                                },
                                *consistency_run,
                                10,
                            )
                            .executable(AgentLauncherExecutable);
                        executable
                            .execute(
                                &execution_context,
                                AgentInput {
                                    agent_ref: agent_task.agent_ref.to_string(),
                                    prompt,
                                    memory: vec![],
                                },
                            )
                            .await
                            .map(|(output, score)| {
                                output
                                    .try_get_metadata()
                                    .map(|value| OutputContainer::Consistency { value, score })
                            })?
                    }
                    _ => {
                        AgentLauncherExecutable
                            .execute(
                                &execution_context,
                                AgentInput {
                                    agent_ref: agent_task.agent_ref.to_string(),
                                    prompt,
                                    memory: vec![],
                                },
                            )
                            .await
                    }
                }
            }
            TaskType::SemanticQuery(semantic_task) => {
                let output = build_semantic_query_executable()
                    .execute(&execution_context, semantic_task)
                    .await?;
                Ok(output.into())
            }
            TaskType::ExecuteSQL(execute_sqltask) => build_sql_task_executable()
                .execute(&execution_context, execute_sqltask)
                .await
                .map(|output| output.into()),
            TaskType::LoopSequential(loop_sequential_task) => {
                let loop_values = match runtime_input {
                    Some(RuntimeTaskInput::Loop { values }) => values,
                    _ => {
                        return Err(OxyError::RuntimeError(
                            "Loop values not provided".to_string(),
                        ));
                    }
                };
                let mut loop_executable = build_loop_executable(
                    loop_sequential_task.tasks.clone(),
                    task.name.clone(),
                    loop_sequential_task.concurrency,
                );
                loop_executable
                    .execute(&task_execution_context, loop_values)
                    .await
                    .map(|results| {
                        results
                            .into_iter()
                            .try_collect::<OutputContainer, Vec<_>, OxyError>()
                            .map(OutputContainer::List)
                    })?
            }
            TaskType::Formatter(formatter_task) => {
                let value: Result<OutputContainer, OxyError> = execution_context
                    .renderer
                    .render(&formatter_task.template)
                    .map(|value| Output::Text(value).into());
                match value.as_ref() {
                    Ok(value) => {
                        execution_context
                            .write_kind(EventKind::Message {
                                message: format!("{}", "\nOutput:".primary()),
                            })
                            .await?;
                        execution_context
                            .write_chunk(Chunk {
                                key: None,
                                delta: Output::Text(value.to_string()),
                                finished: true,
                            })
                            .await?;
                    }
                    Err(_e) => {}
                }
                value
            }
            TaskType::Workflow(workflow_task) => {
                let (run_info, variables) = match runtime_input {
                    Some(RuntimeTaskInput::ChildRunInfo {
                        variables,
                        run_info,
                    }) => (run_info, variables),
                    _ => {
                        return Err(OxyError::RuntimeError(
                            "Workflow variables not provided".to_string(),
                        ));
                    }
                };
                match run_info {
                    Some(run_info) => {
                        let metadata = serde_json::to_string(
                            &HashMap::<String, serde_json::Value>::from_iter([
                                (
                                    "type".to_string(),
                                    serde_json::to_value("sub_workflow").unwrap(),
                                ),
                                (
                                    "workflow_id".to_string(),
                                    serde_json::to_value(run_info.get_source_id()).unwrap(),
                                ),
                                (
                                    "run_id".to_string(),
                                    serde_json::to_value(run_info.get_run_index()).unwrap(),
                                ),
                            ]),
                        )
                        .map_err(|e| {
                            OxyError::RuntimeError(format!(
                                "Failed to serialize sub workflow metadata: {e}"
                            ))
                        })?;
                        task_execution_context
                            .write_kind(EventKind::SetMetadata {
                                attributes: HashMap::from_iter([(
                                    "metadata".to_string(),
                                    metadata,
                                )]),
                            })
                            .await?;
                        WorkflowLauncherExecutable
                            .execute(
                                &task_execution_context,
                                WorkflowInput {
                                    retry: RetryStrategy::Retry {
                                        replay_id: run_info.get_replay_id(),
                                        run_index: run_info.get_run_index(),
                                    },
                                    workflow_ref: workflow_task.src.to_string_lossy().to_string(),
                                    variables,
                                },
                            )
                            .await
                    }
                    None => {
                        WorkflowLauncherExecutable
                            .execute(
                                &task_execution_context,
                                WorkflowInput {
                                    retry: RetryStrategy::NoRetry,
                                    workflow_ref: workflow_task.src.to_string_lossy().to_string(),
                                    variables,
                                },
                            )
                            .await
                    }
                }
            }
            TaskType::Conditional(_) => todo!(),
            TaskType::Unknown => Err(OxyError::RuntimeError("Unknown task type".to_string())),
        };
        tracing::warn!("Executing task finished: {}", task.name);
        task_execution_context
            .write_kind(EventKind::Finished {
                message: "".to_string(),
                attributes: Default::default(),
                error: new_value.as_ref().err().map(|e| e.to_string()),
            })
            .await?;

        new_value
    }
}

#[derive(Clone)]
pub struct TaskChainMapper;

#[async_trait::async_trait]
impl ContextMapper<TaskInput, OutputContainer> for TaskChainMapper {
    async fn map_reduce(
        &self,
        execution_context: &ExecutionContext,
        memo: OutputContainer,
        input: TaskInput,
        value: OutputContainer,
    ) -> Result<(OutputContainer, Option<ExecutionContext>), OxyError> {
        let new_value: OutputContainer =
            memo.merge(HashMap::from_iter([(input.task.name, value)]).into());
        tracing::debug!("Output Value: {:?}", new_value);
        let execution_context = execution_context.wrap_render_context(&(&new_value).into());
        Ok((new_value, Some(execution_context)))
    }
}

pub fn build_task_executable() -> Cache<Export<TaskExecutable, TaskExporter>, TaskCacheStorage> {
    ExecutableBuilder::new()
        .cache_with(TaskCacheStorage::new())
        .export_with(TaskExporter)
        .executable(TaskExecutable)
}

#[async_trait::async_trait]
impl ParamMapper<(Option<usize>, Task), TaskInput> for TaskChainMapper {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: (Option<usize>, Task),
    ) -> Result<(TaskInput, Option<ExecutionContext>), OxyError> {
        let (loop_idx, input) = input;
        let mut task_input = TaskInput {
            task: input,
            runtime_input: None,
            loop_idx,
            value: None,
        };
        let runtime_input = match task_input.task.task_type.clone() {
            TaskType::LoopSequential(loop_sequential_task) => {
                let values = match loop_sequential_task.values {
                    LoopValues::Template(ref template) => {
                        execution_context.renderer.eval_enumerate(template)?
                    }
                    LoopValues::Array(ref values) => values
                        .iter()
                        .map(minijinja::Value::from_serialize)
                        .collect(),
                };
                Some(RuntimeTaskInput::Loop { values })
            }
            TaskType::Workflow(workflow_task) => {
                let variables = workflow_task
                    .variables.as_ref()
                    .map(|vars| {
                        vars.iter()
                            .map(|(k, v)| {
                                if let Some(template) = v.as_str() {
                                    let rendered_value = execution_context
                                        .renderer
                                        .eval_expression(template)?;
                                    let json_value = serde_json::to_value(rendered_value)?;
                                    let final_value = match json_value.is_null() {
                                        true => v.clone(),
                                        false => json_value,
                                    };
                                    Ok((k.to_string(), final_value))
                                } else {
                                    Ok((k.to_string(), v.clone()))
                                }
                            })
                            .try_collect::<(String, JsonValue), HashMap<String, JsonValue>, OxyError>()
                    })
                    .transpose()?;
                let run_info = match &execution_context.checkpoint {
                    Some(checkpoint_context) => {
                        let run_info = checkpoint_context
                            .get_child_run_info(
                                &task_input.replay_id(),
                                &file_path_to_source_id(&workflow_task.src),
                            )
                            .await?;
                        Some(run_info)
                    }
                    None => None,
                };

                Some(RuntimeTaskInput::ChildRunInfo {
                    run_info,
                    variables,
                })
            }
            _ => None,
        };
        task_input.runtime_input = runtime_input;

        Ok((task_input, None))
    }
}
