use std::collections::HashMap;

use itertools::Itertools;
use serde_json::Value as JsonValue;

use crate::{
    adapters::checkpoint::CheckpointManager,
    agent::{AgentLauncherExecutable, types::AgentInput},
    config::model::{Task, TaskType},
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, cache::Cache, chain::ContextMapper, export::Export},
        types::{EventKind, Output, OutputContainer},
    },
    theme::StyledText,
};

use super::{
    WorkflowInput, WorkflowLauncherExecutable, cache::TaskCacheStorage, consistency::AgentPicker,
    export::TaskExporter, loop_concurrency::build_loop_executable, sql::build_sql_task_executable,
};

#[derive(Clone)]
pub(super) struct TaskExecutable {
    checkpoint_manager: CheckpointManager,
}

impl TaskExecutable {
    pub fn new(checkpoint_manager: CheckpointManager) -> Self {
        Self { checkpoint_manager }
    }
}

#[derive(Debug, Clone, Hash)]
pub(super) struct TaskInput {
    pub task: Task,
    pub value: Option<OutputContainer>,
}

#[async_trait::async_trait]
impl Executable<TaskInput> for TaskExecutable {
    type Response = OutputContainer;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: TaskInput,
    ) -> Result<Self::Response, OxyError> {
        let TaskInput { task, value } = input;
        let execution_context = execution_context.with_child_source(
            format!("{}-{}", &task.name, fxhash::hash(&value)),
            (&task.kind()).to_string(),
        );
        execution_context
            .write_kind(EventKind::Started {
                name: task.name.to_string(),
            })
            .await?;
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
                        let (output, score) = executable
                            .execute(
                                &execution_context,
                                AgentInput {
                                    agent_ref: agent_task.agent_ref.to_string(),
                                    prompt,
                                },
                            )
                            .await?;
                        let value = output.try_get_metadata()?;
                        Ok(OutputContainer::Consistency { value, score })
                    }
                    _ => {
                        AgentLauncherExecutable
                            .execute(
                                &execution_context,
                                AgentInput {
                                    agent_ref: agent_task.agent_ref.to_string(),
                                    prompt,
                                },
                            )
                            .await
                    }
                }
            }
            TaskType::ExecuteSQL(execute_sqltask) => {
                let output = build_sql_task_executable()
                    .execute(&execution_context, execute_sqltask)
                    .await?;
                Ok(output.into())
            }
            TaskType::LoopSequential(loop_sequential_task) => {
                let mut loop_executable = build_loop_executable(
                    loop_sequential_task.tasks.clone(),
                    task.name.clone(),
                    loop_sequential_task.concurrency,
                    self.checkpoint_manager.clone(),
                );
                let values = loop_executable
                    .execute(&execution_context, loop_sequential_task.values)
                    .await?;
                let mut results = vec![];
                for value in values {
                    results.push(value?);
                }
                Ok(OutputContainer::List(results))
            }
            TaskType::Formatter(formatter_task) => {
                let value = execution_context
                    .renderer
                    .render(&formatter_task.template)?;
                execution_context
                    .write_kind(EventKind::Message {
                        message: format!("{}\n{}", "\nOutput:".primary(), value.clone()),
                    })
                    .await?;
                Ok(Output::Text(value).into())
            }
            TaskType::Workflow(workflow_task) => {
                let variables = workflow_task
                    .variables
                    .map(|vars| {
                        vars.into_iter()
                            .map(|(k, v)| {
                                if let Some(template) = v.as_str() {
                                    let rendered_value = execution_context
                                        .renderer
                                        .eval_expression(template)?;
                                    let json_value = serde_json::to_value(rendered_value)?;
                                    let final_value = match json_value.is_null() {
                                        true => v,
                                        false => json_value,
                                    };
                                    Ok((k, final_value))
                                } else {
                                    Ok((k, v))
                                }
                            })
                            .try_collect::<(String, JsonValue), HashMap<String, JsonValue>, OxyError>()
                    })
                    .transpose()?;
                WorkflowLauncherExecutable
                    .execute(
                        &execution_context,
                        WorkflowInput {
                            restore_from_checkpoint: false,
                            workflow_ref: workflow_task.src.to_string_lossy().to_string(),
                            variables,
                        },
                    )
                    .await
            }
            TaskType::Conditional(_) => todo!(),
            TaskType::Unknown => Err(OxyError::RuntimeError("Unknown task type".to_string())),
        }?;
        execution_context
            .write_kind(EventKind::Finished {
                message: "".to_string(),
            })
            .await?;

        Ok(new_value)
    }
}

#[derive(Clone)]
pub struct TaskChainMapper;

#[async_trait::async_trait]
impl ContextMapper<TaskInput, OutputContainer> for TaskChainMapper {
    async fn map(
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

pub fn build_task_executable(
    checkpoint_manager: CheckpointManager,
) -> Cache<Export<TaskExecutable, TaskExporter>, TaskCacheStorage> {
    ExecutableBuilder::new()
        .cache_with(TaskCacheStorage::new())
        .export_with(TaskExporter)
        .executable(TaskExecutable::new(checkpoint_manager))
}
