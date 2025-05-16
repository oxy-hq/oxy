use indexmap::IndexMap;

use crate::{
    adapters::checkpoint::CheckpointManager,
    config::{
        constants::LOOP_VAR_NAME,
        model::{LoopValues, Task},
    },
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, map::ParamMapper},
        types::{Output, OutputContainer},
    },
};

use super::task::{TaskChainMapper, TaskInput, build_task_executable};

#[derive(Clone)]
pub(super) struct LoopMapper;

#[async_trait::async_trait]
impl ParamMapper<LoopValues, Vec<String>> for LoopMapper {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: LoopValues,
    ) -> Result<(Vec<String>, Option<ExecutionContext>), OxyError> {
        let values = match input {
            LoopValues::Template(ref template) => {
                execution_context.renderer.eval_enumerate(template)?
            }
            LoopValues::Array(ref values) => values.clone(),
        };
        Ok((values, None))
    }
}

#[derive(Clone)]
pub(super) struct LoopChainMapper;

#[async_trait::async_trait]
impl ParamMapper<((Vec<Task>, String), String), (Vec<TaskInput>, OutputContainer)>
    for LoopChainMapper
{
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: ((Vec<Task>, String), String),
    ) -> Result<((Vec<TaskInput>, OutputContainer), Option<ExecutionContext>), OxyError> {
        let ((tasks, name), input) = input;

        let value = OutputContainer::Map(IndexMap::from_iter([
            (
                name,
                OutputContainer::Map(IndexMap::from_iter([(
                    LOOP_VAR_NAME.to_string(),
                    Output::Text(input.clone()).into(),
                )])),
            ),
            (
                LOOP_VAR_NAME.to_string(),
                Output::Text(input.clone()).into(),
            ),
        ]));
        let execution_context = execution_context.wrap_render_context(&(&value).into());
        Ok((
            (
                tasks
                    .into_iter()
                    .map(|task| TaskInput {
                        task,
                        value: Some(value.clone()),
                    })
                    .collect(),
                value,
            ),
            Some(execution_context),
        ))
    }
}

pub(super) fn build_loop_executable(
    tasks: Vec<Task>,
    name: String,
    concurrency: usize,
    checkpoint_manager: CheckpointManager,
) -> impl Executable<LoopValues, Response = Vec<Result<OutputContainer, OxyError>>> {
    ExecutableBuilder::new()
        .map(LoopMapper)
        .concurrency(concurrency)
        .state((tasks, name))
        .map(LoopChainMapper)
        .chain_map(TaskChainMapper)
        .checkpoint()
        .executable(build_task_executable(checkpoint_manager))
}
