use std::collections::HashMap;

use crate::{
    adapters::checkpoint::CheckpointManager,
    config::model::Task,
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, checkpoint::ShouldRestore, map::ParamMapper},
        renderer::Renderer,
        types::{Output, OutputContainer},
    },
};

use super::task::{TaskChainMapper, TaskInput, build_task_executable};

#[derive(Clone)]
pub(super) struct WorkflowMapper;

#[async_trait::async_trait]
impl ParamMapper<(String, Option<HashMap<String, String>>), (Vec<TaskInput>, OutputContainer)>
    for WorkflowMapper
{
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: (String, Option<HashMap<String, String>>),
    ) -> Result<((Vec<TaskInput>, OutputContainer), Option<ExecutionContext>), OxyError> {
        let (workflow_ref, variables) = input;
        let workflow = execution_context
            .config
            .resolve_workflow(workflow_ref)
            .await?;
        let variables = variables
            .clone()
            .unwrap_or(workflow.variables.clone().unwrap_or_default());
        let value: OutputContainer = variables
            .into_iter()
            .map(|(k, v)| (k, Output::Text(v).into()))
            .collect::<HashMap<String, OutputContainer>>()
            .into();
        let renderer = Renderer::from_template((&value).into(), &workflow)?;
        let execution_context: ExecutionContext = execution_context.wrap_renderer(renderer);
        Ok((
            (
                workflow
                    .tasks
                    .into_iter()
                    .map(|task| TaskInput { task, value: None })
                    .collect(),
                value,
            ),
            Some(execution_context),
        ))
    }
}

pub(super) fn build_workflow_executable<S>(
    checkpoint_manager: CheckpointManager,
    should_restore: S,
) -> impl Executable<(String, Option<HashMap<String, String>>), Response = OutputContainer>
where
    S: ShouldRestore + Clone + Send + Sync,
{
    ExecutableBuilder::new()
        .map(WorkflowMapper)
        .checkpoint_root(checkpoint_manager.clone(), should_restore)
        .chain_map(TaskChainMapper)
        .checkpoint()
        .executable(build_task_executable(checkpoint_manager))
}

pub(super) fn build_tasks_executable<S>(
    checkpoint_manager: CheckpointManager,
    should_restore: S,
) -> impl Executable<Vec<Task>, Response = OutputContainer>
where
    S: ShouldRestore + Clone + Send + Sync,
{
    ExecutableBuilder::new()
        .map(TasksMapper)
        .checkpoint_root(checkpoint_manager.clone(), should_restore)
        .chain_map(TaskChainMapper)
        .checkpoint()
        .executable(build_task_executable(checkpoint_manager))
}

#[derive(Clone)]
pub(super) struct TasksMapper;

#[async_trait::async_trait]
impl ParamMapper<Vec<Task>, (Vec<TaskInput>, OutputContainer)> for TasksMapper {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: Vec<Task>,
    ) -> Result<((Vec<TaskInput>, OutputContainer), Option<ExecutionContext>), OxyError> {
        let value: OutputContainer = OutputContainer::default();
        let renderer = Renderer::from_template((&value).into(), &input)?;
        let execution_context: ExecutionContext = execution_context.wrap_renderer(renderer);
        Ok((
            (
                input
                    .into_iter()
                    .map(|task| TaskInput { task, value: None })
                    .collect(),
                value,
            ),
            Some(execution_context),
        ))
    }
}
