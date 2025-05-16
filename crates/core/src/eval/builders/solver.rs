use itertools::Itertools;
use minijinja::{Value, context};
use rapidfuzz::distance::levenshtein::normalized_distance;

use crate::{
    agent::{OneShotInput, build_openai_executable},
    config::{
        constants::{EVAL_METRICS_POSTFIX, EVAL_SOURCE},
        model::{DistanceMethod, SolverKind},
    },
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, map::ParamMapper},
        types::TargetOutput,
    },
};

use super::types::{MetricKind, RecallRecord, Record};

#[derive(Clone, Debug)]
pub struct LLMSolverMapper {
    prompt_template: String,
    task_description: Option<String>,
}

#[async_trait::async_trait]
impl ParamMapper<(TargetOutput, TargetOutput), OneShotInput> for LLMSolverMapper {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: (TargetOutput, TargetOutput),
    ) -> Result<(OneShotInput, Option<ExecutionContext>), OxyError> {
        let (submission_1, submission_2) = input;
        let task_description = match &self.task_description {
            Some(task_description) => task_description,
            None => &submission_1
                .task_description
                .ok_or(OxyError::ConfigurationError(
                    "Task description is required for non-agent task consistency evaluation"
                        .to_string(),
                ))?,
        };
        let context = context! {
            submission_1 => submission_1.output,
            submission_2 => submission_2.output,
            task_description => Value::from_safe_string(task_description.to_string()),
        };
        let system_instructions = execution_context
            .renderer
            .render_once(&self.prompt_template, context)
            .map_err(|_| {
                OxyError::RuntimeError("Failed to render consistency evaluation prompt".to_string())
            })?;
        Ok((
            OneShotInput {
                system_instructions,
                user_input: None,
            },
            None,
        ))
    }
}

#[derive(Clone, Debug)]
pub(super) struct SolverExecutable {
    concurrency: usize,
}

impl SolverExecutable {
    pub fn new(concurrency: usize) -> Self {
        Self { concurrency }
    }
}

#[async_trait::async_trait]
impl Executable<(SolverKind, Vec<(TargetOutput, TargetOutput)>)> for SolverExecutable {
    type Response = MetricKind;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        (solver_kind, outputs): (SolverKind, Vec<(TargetOutput, TargetOutput)>),
    ) -> Result<Self::Response, OxyError> {
        let metric_context = execution_context.with_child_source(
            format!("{}-{}", execution_context.source.id, EVAL_METRICS_POSTFIX),
            EVAL_SOURCE.to_string(),
        );

        match solver_kind {
            SolverKind::Similarity(llm_solver) => {
                let model_ref = match &llm_solver.model_ref {
                    Some(model_ref) => model_ref,
                    None => match execution_context.config.default_model() {
                        Some(model_ref) => model_ref,
                        None => {
                            return Err(OxyError::ConfigurationError(
                                "No default model found".to_string(),
                            ));
                        }
                    },
                };
                let model = execution_context.config.resolve_model(model_ref)?;
                let agent = build_openai_executable(model);
                let mut eval_executable = ExecutableBuilder::new()
                    .concurrency(self.concurrency)
                    .map(LLMSolverMapper {
                        prompt_template: llm_solver.prompt.to_string(),
                        task_description: None,
                    })
                    .executable(agent);
                let results = eval_executable.execute(&metric_context, outputs).await?;
                let metric = results
                    .into_iter()
                    .map(|res| {
                        let output = res?;
                        let mut record = Record::try_from(output.content)?;
                        record.fill_score(&llm_solver.scores);
                        Ok(record)
                    })
                    .try_collect::<Record, MetricKind, OxyError>()?;
                Ok(metric)
            }
            SolverKind::ContextRecall(recall) => {
                let metric = outputs
                    .into_iter()
                    .map(|(submission_1, submission_2)| match recall.distance {
                        DistanceMethod::Levenshtein => {
                            let scores = submission_2
                                .relevant_contexts
                                .iter()
                                .filter_map(|reference_context| {
                                    submission_1
                                        .relevant_contexts
                                        .iter()
                                        .map(|retrieved_context| {
                                            let distance = normalized_distance(
                                                retrieved_context.chars(),
                                                reference_context.chars(),
                                            );

                                            1.0 - distance
                                        })
                                        .max_by(|a, b| a.partial_cmp(b).unwrap())
                                })
                                .collect::<Vec<_>>();
                            let score = if scores.is_empty() {
                                f32::NAN
                            } else {
                                scores.iter().sum::<f64>() as f32 / scores.len() as f32
                            };
                            RecallRecord {
                                score,
                                pass: score > recall.threshold,
                                retrieved_contexts: submission_1.relevant_contexts,
                                reference_contexts: submission_2.relevant_contexts,
                            }
                        }
                    })
                    .collect::<MetricKind>();
                Ok(metric)
            }
        }
    }
}
