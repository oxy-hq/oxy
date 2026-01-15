use std::collections::HashMap;

use indexmap::IndexMap;

use oxy::{
    config::{constants::LOOP_VAR_NAME, model::Task},
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, map::ParamMapper},
        types::{EventKind, Output, OutputContainer},
    },
    observability::events::workflow as workflow_events,
};
use oxy_shared::errors::OxyError;

use crate::{
    task_builder::{TaskChainMapper, build_task_executable},
    workflow_builder::TasksGroupInput,
};

#[derive(Clone)]
pub(super) struct LoopMapper;

#[async_trait::async_trait]
impl ParamMapper<Vec<minijinja::Value>, Vec<(usize, minijinja::Value)>> for LoopMapper {
    #[tracing::instrument(skip_all, err, fields(
        otel.name = workflow_events::task::loop_task::NAME_MAP,
        oxy.span_type = workflow_events::task::loop_task::TYPE,
        oxy.loop.iterations_count = input.len(),
    ))]
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: Vec<minijinja::Value>,
    ) -> Result<(Vec<(usize, minijinja::Value)>, Option<ExecutionContext>), OxyError> {
        workflow_events::task::loop_task::map_input(input.len());

        let values = input;
        let metadata = serde_json::to_string(&HashMap::<String, serde_json::Value>::from_iter([
            ("type".to_string(), serde_json::to_value("loop").unwrap()),
            ("values".to_string(), serde_json::to_value(&values).unwrap()),
        ]))
        .map_err(|e| OxyError::RuntimeError(format!("Failed to serialize loop values: {e}")))?;
        execution_context
            .write_kind(EventKind::SetMetadata {
                attributes: HashMap::from_iter([("metadata".to_string(), metadata)]),
            })
            .await?;

        let result: Vec<(usize, minijinja::Value)> = values.into_iter().enumerate().collect();
        workflow_events::task::loop_task::map_output(result.len());

        Ok((result, None))
    }
}

#[derive(Clone)]
pub(super) struct LoopItemMapper;

#[async_trait::async_trait]
impl ParamMapper<((Vec<Task>, String), (usize, minijinja::Value)), TasksGroupInput>
    for LoopItemMapper
{
    #[tracing::instrument(skip_all, err, fields(
        otel.name = workflow_events::task::loop_task::NAME_ITEM_MAP,
        oxy.span_type = workflow_events::task::loop_task::TYPE,
        oxy.loop.iteration_index = tracing::field::Empty,
        oxy.loop.task_name = tracing::field::Empty,
    ))]
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: ((Vec<Task>, String), (usize, minijinja::Value)),
    ) -> Result<(TasksGroupInput, Option<ExecutionContext>), OxyError> {
        let span = tracing::Span::current();
        span.record("oxy.loop.iteration_index", input.1.0);
        span.record("oxy.loop.task_name", input.0.1.as_str());

        workflow_events::task::loop_task::item_map_input(
            input.1.0,
            &input.0.1,
            &input.1.1.to_string(),
        );

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

        workflow_events::task::loop_task::item_map_output(loop_idx, &name);

        Ok((
            TasksGroupInput {
                group_ref: name,
                tasks,
                value,
                loop_idx: Some(loop_idx),
                workflow_consistency_prompt: None, // Loops inherit from parent
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
        .chain_map(TaskChainMapper {
            workflow_consistency_prompt: None, // Will be read from renderer context
        })
        .checkpoint()
        .executable(build_task_executable())
}
