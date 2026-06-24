use itertools::Itertools;

use oxy::{
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
use oxy_shared::errors::OxyError;

use super::{
    EvalInput, EvalResult,
    generator::GeneratorExecutable,
    solver::SolverExecutable,
    types::{AgenticInput, MetricKind, RunStats},
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
                "No tasks found in the automation".to_string(),
            ));
        }
        Ok(task_ref.join("."))
    }
}

#[async_trait::async_trait]
impl ParamMapper<EvalInput, Vec<(usize, EvalConfig, AgenticInput)>> for EvalMapper {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: EvalInput,
    ) -> Result<
        (
            Vec<(usize, EvalConfig, AgenticInput)>,
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
            test_ref if test_ref.ends_with("test.yml") => {
                let config_manager = &execution_context.workspace.config_manager;
                let test_config = config_manager.resolve_test(&target_ref).await?;
                let resolved_target = test_config.target.ok_or_else(|| {
                    OxyError::ConfigurationError(format!(
                        "Could not determine target for test file: {target_ref}"
                    ))
                })?;

                // Only `.agentic.yml` agents are runnable as eval targets. The
                // classic `.agent.yml` runtime was retired with the oxy-agent
                // crate; automations don't accept prompts
                // via the test framework.
                let eval_target = if resolved_target.ends_with("agentic.yml")
                    || resolved_target.ends_with("agentic.yaml")
                {
                    AgenticInput {
                        config_path: resolved_target.clone(),
                        prompt: String::new(),
                    }
                } else {
                    return Err(OxyError::ConfigurationError(format!(
                        "Unsupported test target: {resolved_target}. \
                         Expected .agentic.yml or .agentic.yaml — the classic \
                         .agent.yml runtime was removed."
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
                    "Invalid file extension: {target_ref}. Expected .test.yml \
                     (the inline workflow.tests / agent.tests entry points were \
                     removed with classic agent retirement)."
                )));
            }
        };
        mapped_input.map(|input| (input, None))
    }
}

#[derive(Clone, Debug)]
pub struct EvalExecutable;

#[async_trait::async_trait]
impl Executable<(usize, EvalConfig, AgenticInput)> for EvalExecutable {
    type Response = EvalResult;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: (usize, EvalConfig, AgenticInput),
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
