use std::collections::HashMap;

use indexmap::IndexMap;

use crate::{
    config::{constants::LOOP_VAR_NAME, model::Task},
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, map::ParamMapper},
        types::{EventKind, Output, OutputContainer},
    },
};

use super::{
    task::{TaskChainMapper, build_task_executable},
    workflow::TasksGroupInput,
};

#[derive(Clone)]
pub(super) struct LoopMapper;

#[async_trait::async_trait]
impl ParamMapper<Vec<minijinja::Value>, Vec<(usize, minijinja::Value)>> for LoopMapper {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: Vec<minijinja::Value>,
    ) -> Result<(Vec<(usize, minijinja::Value)>, Option<ExecutionContext>), OxyError> {
        let values = input;
        let metadata = serde_json::to_string(&HashMap::<String, serde_json::Value>::from_iter([
            ("type".to_string(), serde_json::to_value("loop").unwrap()),
            ("values".to_string(), serde_json::to_value(&values).unwrap()),
        ]))
        .map_err(|e| OxyError::RuntimeError(format!("Failed to serialize loop values: {}", e)))?;
        execution_context
            .write_kind(EventKind::SetMetadata {
                attributes: HashMap::from_iter([("metadata".to_string(), metadata)]),
            })
            .await?;
        Ok((values.into_iter().enumerate().collect(), None))
    }
}

#[derive(Clone)]
pub(super) struct LoopItemMapper;

#[async_trait::async_trait]
impl ParamMapper<((Vec<Task>, String), (usize, minijinja::Value)), TasksGroupInput>
    for LoopItemMapper
{
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: ((Vec<Task>, String), (usize, minijinja::Value)),
    ) -> Result<(TasksGroupInput, Option<ExecutionContext>), OxyError> {
        let ((tasks, name), (loop_idx, input)) = input;

        let value = OutputContainer::Map(IndexMap::from_iter([
            (
                name.clone(),
                OutputContainer::Map(IndexMap::from_iter([(
                    LOOP_VAR_NAME.to_string(),
                    Output::Text(input.to_string()).into(),
                )])),
            ),
            (
                LOOP_VAR_NAME.to_string(),
                Output::Text(input.to_string()).into(),
            ),
        ]));
        let context_value: minijinja::Value = (&value).into();
        let execution_context = execution_context.wrap_render_context(&context_value);
        Ok((
            TasksGroupInput {
                group_ref: name,
                tasks,
                value,
                loop_idx: Some(loop_idx),
            },
            Some(execution_context),
        ))
    }
}

pub(super) fn build_loop_executable(
    tasks: Vec<Task>,
    name: String,
    concurrency: usize,
) -> impl Executable<Vec<minijinja::Value>, Response = Vec<Result<OutputContainer, OxyError>>> {
    ExecutableBuilder::new()
        .map(LoopMapper)
        .concurrency(concurrency)
        .state((tasks, name))
        .map(LoopItemMapper)
        .chain_map(TaskChainMapper)
        .checkpoint()
        .executable(build_task_executable())
}
