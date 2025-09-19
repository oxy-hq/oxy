use std::{collections::HashMap, path::PathBuf};

use itertools::Itertools;
use schemars::schema::SchemaObject;
use serde_json::Value;

use crate::{
    adapters::checkpoint::RunInfo,
    config::model::{Task, Variable, Variables, Workflow},
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::{
            ExecutableBuilder, chain::IntoChain, checkpoint::CheckpointRootId, map::ParamMapper,
        },
        renderer::Renderer,
        types::OutputContainer,
    },
};

use super::task::{TaskChainMapper, build_task_executable};

#[derive(Debug, Clone, Hash)]
pub(super) struct TasksGroupInput {
    pub group_ref: String,
    pub tasks: Vec<Task>,
    pub value: OutputContainer,
    pub loop_idx: Option<usize>,
}

impl IntoChain<(Option<usize>, Task), OutputContainer> for TasksGroupInput {
    fn into_chain(self) -> (Vec<(Option<usize>, Task)>, OutputContainer) {
        let tasks = self
            .tasks
            .into_iter()
            .map(|task| (self.loop_idx, task))
            .collect();
        let value = self.value;
        (tasks, value)
    }
}

#[derive(Debug, Clone)]
pub(super) struct WorkflowRunInput {
    pub run_info: RunInfo,
    pub tasks_group: TasksGroupInput,
}

impl IntoChain<(Option<usize>, Task), OutputContainer> for WorkflowRunInput {
    fn into_chain(self) -> (Vec<(Option<usize>, Task)>, OutputContainer) {
        let tasks = self
            .tasks_group
            .tasks
            .into_iter()
            .map(|task| (self.tasks_group.loop_idx, task))
            .collect();
        let value = self.tasks_group.value;
        (tasks, value)
    }
}

impl CheckpointRootId for WorkflowRunInput {
    fn run_info(&self) -> RunInfo {
        self.run_info.clone()
    }
}

#[derive(Clone)]
pub(super) struct WorkflowMapper;

impl WorkflowMapper {
    async fn resolve_workflow_variables_schema(
        &self,
        execution_context: &ExecutionContext,
        workflow_ref: String,
    ) -> Result<Workflow, OxyError> {
        let config_manager = &execution_context.project.config_manager;
        let temp_workflow = config_manager
            .resolve_workflow_with_raw_variables(workflow_ref)
            .await?;
        let variables = temp_workflow
            .variables
            .map(|variables| {
                variables
                    .into_iter()
                    .map(|(k, v)| {
                        if let Some(template) = v.as_str() {
                            let rendered_value =
                                execution_context.renderer.eval_expression(template)?;
                            let json_value = serde_json::to_value(rendered_value)?;
                            let final_value = match json_value.is_null() {
                                true => v,
                                false => json_value,
                            };
                            let variable: Variable = serde_json::from_value(final_value)?;
                            Ok((k, variable.into()))
                        } else {
                            let variable: Variable = serde_json::from_value(v)?;
                            Ok((k, variable.into()))
                        }
                    })
                    .try_collect::<(String, SchemaObject), HashMap<String, SchemaObject>, OxyError>(
                    )
            })
            .transpose()?
            .map(|variables| Variables { variables });

        Ok(Workflow {
            name: temp_workflow.name,
            tasks: temp_workflow.tasks,
            tests: temp_workflow.tests,
            variables,
            description: temp_workflow.description,
            retrieval: temp_workflow.retrieval,
        })
    }
}

#[async_trait::async_trait]
impl ParamMapper<(String, Option<HashMap<String, Value>>, RunInfo), WorkflowRunInput>
    for WorkflowMapper
{
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: (String, Option<HashMap<String, Value>>, RunInfo),
    ) -> Result<(WorkflowRunInput, Option<ExecutionContext>), OxyError> {
        // Extract the workflow reference and variables from the input
        let (workflow_ref, variables, run_info) = input;
        let workflow = self
            .resolve_workflow_variables_schema(execution_context, workflow_ref.clone())
            .await?;

        // Validate the workflow variables against the schema
        let variables_schema = workflow.variables.clone().unwrap_or_default();
        let variables = variables_schema.resolve_params(variables)?;
        let json_schema: serde_json::Value = (&variables_schema).into();
        let instance = serde_json::to_value(&variables).map_err(|err| {
            OxyError::ArgumentError(format!(
                "Failed to serialize workflow variables for workflow '{}': {}\nVariables: {:#?}",
                workflow.name, err, variables
            ))
        })?;

        jsonschema::validate(&json_schema, &instance).map_err(|err| {
            OxyError::ArgumentError(format!(
                "Workflow variable validation failed for workflow '{}': {}\nSchema: {:#?}\nInstance: {:#?}",
                workflow.name, err, json_schema, instance
            ))
        })?;

        // Create the OutputContainer and Renderer
        let value: OutputContainer = variables
            .into_iter()
            .map(|(k, v)| (k, OutputContainer::Variable(v)))
            .collect::<HashMap<String, OutputContainer>>()
            .into();
        let renderer = Renderer::from_template((&value).into(), &workflow)?;
        let execution_context: ExecutionContext = execution_context.wrap_renderer(renderer);
        Ok((
            WorkflowRunInput {
                run_info,
                tasks_group: TasksGroupInput {
                    group_ref: PathBuf::from(workflow_ref)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    tasks: workflow.tasks,
                    value,
                    loop_idx: None,
                },
            },
            Some(execution_context),
        ))
    }
}

pub(super) fn build_workflow_executable()
-> impl Executable<(String, Option<HashMap<String, Value>>, RunInfo), Response = OutputContainer> {
    ExecutableBuilder::new()
        .map(WorkflowMapper)
        .checkpoint_root()
        .chain_map(TaskChainMapper)
        .checkpoint()
        .executable(build_task_executable())
}

pub(super) fn build_tasks_executable() -> impl Executable<Vec<Task>, Response = OutputContainer> {
    ExecutableBuilder::new()
        .map(TasksMapper)
        .chain_map(TaskChainMapper)
        .executable(build_task_executable())
}

#[derive(Clone)]
pub(super) struct TasksMapper;

#[async_trait::async_trait]
impl ParamMapper<Vec<Task>, TasksGroupInput> for TasksMapper {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: Vec<Task>,
    ) -> Result<(TasksGroupInput, Option<ExecutionContext>), OxyError> {
        let value: OutputContainer = OutputContainer::default();
        let renderer = Renderer::from_template((&value).into(), &input)?;
        let execution_context: ExecutionContext = execution_context.wrap_renderer(renderer);
        Ok((
            TasksGroupInput {
                group_ref: "tasks_group".to_string(),
                tasks: input,
                value,
                loop_idx: None,
            },
            Some(execution_context),
        ))
    }
}
