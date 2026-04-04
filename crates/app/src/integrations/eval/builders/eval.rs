use itertools::Itertools;

use oxy::{
    checkpoint::types::RetryStrategy,
    config::{
        constants::EVAL_SOURCE,
        model::{
            CorrectnessSolver, EvalConfig, EvalKind, SolverKind, Task, TaskType, TestCaseEval,
            default_correctness_prompt,
        },
    },
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, map::ParamMapper},
        types::EventKind,
    },
};
use oxy_agent::types::AgentInput;
use oxy_shared::errors::OxyError;
use oxy_workflow::builders::WorkflowInput;

use super::{
    EvalInput, EvalResult,
    generator::GeneratorExecutable,
    solver::SolverExecutable,
    types::{EvalTarget, MetricKind, RunStats},
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
        let EvalInput {
            target_ref,
            index,
            tag,
        } = input;
        let mapped_input = match &target_ref {
            workflow_ref
                if workflow_ref.ends_with("procedure.yml")
                    || workflow_ref.ends_with("workflow.yml")
                    || workflow_ref.ends_with("automation.yml") =>
            {
                let config_manager = &execution_context.project.config_manager;
                let workflow = config_manager.resolve_workflow(&target_ref).await?;
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
                                retry: RetryStrategy::NoRetry { variables: None },
                            }),
                        )
                    })
                    .collect())
            }
            agent_ref if agent_ref.ends_with("agent.yml") => {
                let config_manager = &execution_context.project.config_manager;
                let agent = config_manager.resolve_agent(&target_ref).await?;
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
                                memory: vec![],
                                variables: None,
                                a2a_task_id: None,
                                a2a_thread_id: None,
                                a2a_context_id: None,
                                sandbox_info: None,
                            }),
                        ))
                    })
                    .try_collect()
            }
            test_ref if test_ref.ends_with("test.yml") => {
                let config_manager = &execution_context.project.config_manager;
                let test_config = config_manager.resolve_test(&target_ref).await?;
                let resolved_target = test_config.target.ok_or_else(|| {
                    OxyError::ConfigurationError(format!(
                        "Could not determine target for test file: {target_ref}"
                    ))
                })?;

                // Determine the eval target based on the resolved target file extension
                let eval_target = if resolved_target.ends_with("agent.yml") {
                    EvalTarget::Agent(AgentInput {
                        agent_ref: resolved_target.clone(),
                        prompt: String::new(),
                        memory: vec![],
                        variables: None,
                        a2a_task_id: None,
                        a2a_thread_id: None,
                        a2a_context_id: None,
                        sandbox_info: None,
                    })
                } else if resolved_target.ends_with("aw.yml") {
                    // Agentic workflows (.aw.yml) are agent-like: they accept a prompt
                    EvalTarget::Agent(AgentInput {
                        agent_ref: resolved_target.clone(),
                        prompt: String::new(),
                        memory: vec![],
                        variables: None,
                        a2a_task_id: None,
                        a2a_thread_id: None,
                        a2a_context_id: None,
                        sandbox_info: None,
                    })
                } else if resolved_target.ends_with("agentic.yml")
                    || resolved_target.ends_with("agentic.yaml")
                {
                    // Analytics agentic systems (.agentic.yml) accept a prompt via
                    // the headless eval path.
                    EvalTarget::Agentic(super::types::AgenticInput {
                        config_path: resolved_target.clone(),
                        prompt: String::new(),
                    })
                } else if resolved_target.ends_with("workflow.yml")
                    || resolved_target.ends_with("automation.yml")
                    || resolved_target.ends_with("procedure.yml")
                {
                    return Err(OxyError::ConfigurationError(format!(
                        "Unsupported test target: {resolved_target}. \
                         The testing framework only supports .agent.yml, .aw.yml, and .agentic.yml targets. \
                         Workflow/automation/procedure files do not accept prompts and cannot be tested with .test.yml files."
                    )));
                } else {
                    return Err(OxyError::ConfigurationError(format!(
                        "Unsupported test target: {resolved_target}. \
                         Expected .agent.yml, .aw.yml, or .agentic.yml"
                    )));
                };

                let correctness_solver = SolverKind::Correctness(CorrectnessSolver {
                    prompt: default_correctness_prompt(),
                    model_ref: test_config.settings.judge_model.clone(),
                });

                // Filter cases by index and/or tag
                let cases: Vec<_> = test_config
                    .cases
                    .into_iter()
                    .enumerate()
                    .filter(|(idx, _)| index.is_none_or(|i| *idx == i))
                    .filter(|(_, c)| {
                        tag.as_ref()
                            .is_none_or(|tag_filter| c.tags.contains(tag_filter))
                    })
                    .map(|(_, c)| c)
                    .collect();

                Ok(vec![(
                    0,
                    EvalConfig {
                        kind: EvalKind::TestCase(TestCaseEval {
                            cases,
                            runs: test_config.settings.runs,
                            judge_model: test_config.settings.judge_model,
                        }),
                        metrics: vec![correctness_solver],
                        concurrency: test_config.settings.concurrency,
                        task_ref: None,
                    },
                    eval_target,
                )])
            }
            _ => {
                return Err(OxyError::ConfigurationError(format!(
                    "Invalid file extension: {target_ref}. Expected .workflow.yml, .automation.yml, .agent.yml, or .test.yml"
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
            execution_context.with_child_source(format!("eval-{idx}"), EVAL_SOURCE.to_string());
        eval_context
            .write_kind(EventKind::Started {
                name: format!("{target}::Test{idx}"),
                attributes: Default::default(),
            })
            .await?;

        eval_context
            .write_kind(EventKind::Message {
                message: "🔄Generating outputs".to_string(),
            })
            .await?;
        let (outputs, errors_with_expected) = GeneratorExecutable::new(eval.concurrency)
            .execute(
                &eval_context,
                (eval.kind.clone(), target, eval.task_ref.clone()),
            )
            .await?;

        let answered = outputs.len();
        let total_attempted = answered + errors_with_expected.len();
        let error_strings: Vec<String> = errors_with_expected
            .iter()
            .map(|(e, _)| e.clone())
            .collect();
        let stats = RunStats {
            total_attempted,
            answered,
        };

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
                    .map(|solver| (solver, outputs.clone(), errors_with_expected.clone()))
                    .collect::<Vec<_>>(),
            )
            .await?
            .into_iter()
            .try_collect::<MetricKind, Vec<_>, OxyError>()?;

        let result = EvalResult::new(error_strings, metrics, stats);
        eval_context
            .write_kind(EventKind::Finished {
                message: format!("{result:?}"),
                attributes: Default::default(),
                error: None,
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
