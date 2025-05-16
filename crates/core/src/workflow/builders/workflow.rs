use std::collections::HashMap;

use serde_json::Value;

use crate::{
    adapters::checkpoint::CheckpointManager,
    config::model::Task,
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, checkpoint::ShouldRestore, map::ParamMapper},
        renderer::Renderer,
        types::OutputContainer,
    },
};

use super::task::{TaskChainMapper, TaskInput, build_task_executable};

#[derive(Clone)]
pub(super) struct WorkflowMapper;

#[async_trait::async_trait]
impl ParamMapper<(String, Option<HashMap<String, Value>>), (Vec<TaskInput>, OutputContainer)>
    for WorkflowMapper
{
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: (String, Option<HashMap<String, Value>>),
    ) -> Result<((Vec<TaskInput>, OutputContainer), Option<ExecutionContext>), OxyError> {
        // Extract the workflow reference and variables from the input
        let (workflow_ref, variables) = input;
        let workflow = execution_context
            .config
            .resolve_workflow(workflow_ref)
            .await?;

        // Validate the workflow variables against the schema
        let variables_schema = workflow.variables.clone().unwrap_or_default();
        let variables = variables_schema.resolve_params(variables)?;
        let json_schema: serde_json::Value = variables_schema.into();
        let instance = serde_json::to_value(&variables)
            .map_err(|err| OxyError::ArgumentError(err.to_string()))?;

        jsonschema::validate(&json_schema, &instance)
            .map_err(|err| OxyError::ArgumentError(err.to_string()))?;

        // Create the OutputContainer and Renderer
        let value: OutputContainer = variables
            .into_iter()
            .map(|(k, v)| (k, OutputContainer::Variable(v)))
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
) -> impl Executable<(String, Option<HashMap<String, Value>>), Response = OutputContainer>
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
