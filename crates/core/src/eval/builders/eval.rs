use itertools::Itertools;

use crate::{
    agent::types::AgentInput,
    config::{
        constants::EVAL_SOURCE,
        model::{EvalConfig, EvalKind, Task, TaskType},
    },
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, map::ParamMapper},
        types::EventKind,
    },
    theme::StyledText,
    workflow::WorkflowInput,
};

use super::{
    EvalInput, EvalResult,
    generator::GeneratorExecutable,
    solver::SolverExecutable,
    types::{EvalTarget, MetricKind},
};

#[derive(Clone, Debug)]
struct EvalMapper;

impl EvalMapper {
    pub fn last_task_ref_internal(&self, tasks: &[Task]) -> Vec<String> {
        let mut task_ref = vec![];
        if let Some(task) = tasks.last() {
            task_ref.push(task.name.clone());
            if let TaskType::LoopSequential(loop_values) = &task.task_type {
                task_ref.extend(self.last_task_ref_internal(&loop_values.tasks))
            }
        }
        task_ref
    }

    pub fn last_task_ref(&self, tasks: &[Task]) -> Result<String, OxyError> {
        let task_ref = self.last_task_ref_internal(tasks);
        if task_ref.is_empty() {
            return Err(OxyError::ConfigurationError(
                "No tasks found in the workflow".to_string(),
            ));
        }
        Ok(task_ref.join("."))
    }
}

#[async_trait::async_trait]
impl ParamMapper<EvalInput, Vec<(usize, EvalConfig, EvalTarget)>> for EvalMapper {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: EvalInput,
    ) -> Result<
        (
            Vec<(usize, EvalConfig, EvalTarget)>,
            Option<ExecutionContext>,
        ),
        OxyError,
    > {
        let EvalInput { target_ref, index } = input;
        let mapped_input = match &target_ref {
            workflow_ref if workflow_ref.ends_with("workflow.yml") => {
                let workflow = execution_context
                    .config
                    .resolve_workflow(&target_ref)
                    .await?;
                Ok(workflow
                    .tests
                    .iter()
                    .enumerate()
                    .filter(|(idx, _)| index.is_none_or(|i| *idx == i))
                    .map(|(idx, test)| {
                        let task_ref = test
                            .task_ref
                            .clone()
                            .or_else(|| self.last_task_ref(&workflow.tasks).ok());
                        (
                            idx,
                            EvalConfig {
                                task_ref,
                                ..test.clone()
                            },
                            EvalTarget::Workflow(WorkflowInput {
                                workflow_ref: workflow_ref.to_string(),
                                restore_from_checkpoint: false,
                                variables: None,
                            }),
                        )
                    })
                    .collect())
            }
            agent_ref if agent_ref.ends_with("agent.yml") => {
                let agent = execution_context.config.resolve_agent(&target_ref).await?;
                agent
                    .tests
                    .iter()
                    .enumerate()
                    .filter(|(idx, _)| index.is_none_or(|i| *idx == i))
                    .map(|(idx, test)| {
                        Ok((
                            idx,
                            test.clone(),
                            EvalTarget::Agent(AgentInput {
                                agent_ref: agent_ref.to_string(),
                                prompt: match &test.kind {
                                    EvalKind::Consistency(consistency) => {
                                        consistency.task_description.clone().ok_or(
                                            OxyError::ConfigurationError(
                                                "Task description is required for agent consistency evaluation"
                                                    .to_string(),
                                            ),
                                        )?
                                    }
                                    _ => "".to_string(),
                                },
                            }),
                        ))
                    })
                    .try_collect()
            }
            _ => {
                return Err(OxyError::ConfigurationError(format!(
                    "Invalid file extension: {}. Expected .workflow.yml",
                    target_ref
                )));
            }
        };
        mapped_input.map(|input| (input, None))
    }
}

#[derive(Clone, Debug)]
pub struct EvalExecutable;

#[async_trait::async_trait]
impl Executable<(usize, EvalConfig, EvalTarget)> for EvalExecutable {
    type Response = EvalResult;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: (usize, EvalConfig, EvalTarget),
    ) -> Result<Self::Response, OxyError> {
        let (idx, eval, target) = input;
        let eval_context =
            execution_context.with_child_source(format!("eval-{}", idx), EVAL_SOURCE.to_string());
        eval_context
            .write_kind(EventKind::Started {
                name: format!("{}::Test{}", target, idx),
            })
            .await?;

        eval_context
            .write_kind(EventKind::Message {
                message: "🔄Generating outputs".to_string(),
            })
            .await?;
        let (outputs, errors) = GeneratorExecutable::new(eval.concurrency)
            .execute(
                &eval_context,
                (eval.kind.clone(), target, eval.task_ref.clone()),
            )
            .await?;

        eval_context
            .write_kind(EventKind::Message {
                message: "🔄Evaluating records".to_string(),
            })
            .await?;
        let mut solver_executable = ExecutableBuilder::new()
            .concurrency(eval.concurrency)
            .executable(SolverExecutable::new(eval.concurrency));
        let metrics = solver_executable
            .execute(
                execution_context,
                eval.metrics
                    .into_iter()
                    .map(|solver| (solver, outputs.clone()))
                    .collect::<Vec<_>>(),
            )
            .await?
            .into_iter()
            .try_collect::<MetricKind, Vec<_>, OxyError>()?;

        let result = EvalResult::new(errors, metrics);
        eval_context
            .write_kind(EventKind::Message {
                message: format!("{}", result).primary().to_string(),
            })
            .await?;
        eval_context
            .write_kind(EventKind::Finished {
                message: format!("{:?}", result),
            })
            .await?;
        Ok(result)
    }
}

pub(crate) fn build_eval_executable()
-> impl Executable<EvalInput, Response = Vec<Result<EvalResult, OxyError>>> {
    ExecutableBuilder::new()
        .map(EvalMapper)
        .concurrency(10)
        .executable(EvalExecutable)
}
