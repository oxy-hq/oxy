use itertools::Itertools;
use minijinja::{Value, context};
use rapidfuzz::distance::levenshtein::normalized_distance;

use oxy::{
    adapters::openai::{IntoOpenAIConfig, OpenAIClient},
    config::{
        constants::{EVAL_METRICS_POSTFIX, EVAL_SOURCE},
        model::{DistanceMethod, SolverKind},
    },
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, map::ParamMapper},
        types::TargetOutput,
    },
};
use oxy_agent::agent::openai::{OneShotInput, SimpleMapper, build_openai_executable};
use oxy_shared::errors::OxyError;

use super::{
    correctness_solver::{CorrectnessSolverMapper, parse_correctness_record},
    types::{Correctness, MetricKind, RecallRecord, Record},
};

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
                memory: vec![],
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
impl
    Executable<(
        SolverKind,
        Vec<(TargetOutput, TargetOutput)>,
        Vec<(String, TargetOutput)>,
    )> for SolverExecutable
{
    type Response = MetricKind;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        (solver_kind, outputs, errors_with_expected): (
            SolverKind,
            Vec<(TargetOutput, TargetOutput)>,
            Vec<(String, TargetOutput)>,
        ),
    ) -> Result<Self::Response, OxyError> {
        let metric_context = execution_context.with_child_source(
            format!("{}-{}", execution_context.source.id, EVAL_METRICS_POSTFIX),
            EVAL_SOURCE.to_string(),
        );

        let config_manager = &execution_context.workspace.config_manager;
        let secret_manager = &execution_context.workspace.secrets_manager;

        match solver_kind {
            SolverKind::Similarity(llm_solver) => {
                let model_ref = match &llm_solver.model_ref {
                    Some(model_ref) => model_ref,
                    None => match config_manager.default_model() {
                        Some(model_ref) => model_ref,
                        None => {
                            return Err(OxyError::ConfigurationError(
                                "No default model found".to_string(),
                            ));
                        }
                    },
                };
                let model = config_manager.resolve_model(model_ref)?;
                let client =
                    OpenAIClient::with_config(model.into_openai_config(secret_manager).await?);
                let agent = build_openai_executable(
                    client,
                    model.model_name().to_string(),
                    vec![],
                    None,
                    None,
                    false,
                );
                let mut eval_executable = ExecutableBuilder::new()
                    .concurrency(self.concurrency)
                    .map(LLMSolverMapper {
                        prompt_template: llm_solver.prompt.to_string(),
                        task_description: None,
                    })
                    .map(SimpleMapper)
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
            SolverKind::Correctness(correctness_solver) => {
                let model_ref = match &correctness_solver.model_ref {
                    Some(model_ref) => model_ref,
                    None => match config_manager.default_model() {
                        Some(model_ref) => model_ref,
                        None => {
                            return Err(OxyError::ConfigurationError(
                                "No default model found".to_string(),
                            ));
                        }
                    },
                };
                let model = config_manager.resolve_model(model_ref)?;
                let client =
                    OpenAIClient::with_config(model.into_openai_config(secret_manager).await?);
                let agent = build_openai_executable(
                    client,
                    model.model_name().to_string(),
                    vec![],
                    None,
                    None,
                    false,
                );
                // Capture context from both actual and expected TargetOutputs
                let run_context: Vec<_> = outputs
                    .iter()
                    .map(|(actual, expected)| {
                        (
                            expected.task_description.clone(),
                            expected.output.clone(),
                            actual.output.clone(),
                            actual.references.clone(),
                            actual.duration_ms,
                            actual.input_tokens,
                            actual.output_tokens,
                        )
                    })
                    .collect();

                let mut eval_executable = ExecutableBuilder::new()
                    .concurrency(self.concurrency)
                    .map(CorrectnessSolverMapper {
                        prompt_template: correctness_solver.prompt.to_string(),
                    })
                    .map(SimpleMapper)
                    .executable(agent);
                let results = eval_executable.execute(&metric_context, outputs).await?;
                let mut records = results
                    .into_iter()
                    .zip(run_context)
                    .map(
                        |(
                            res,
                            (
                                prompt,
                                expected,
                                actual_output,
                                references,
                                duration_ms,
                                input_tokens,
                                output_tokens,
                            ),
                        )| {
                            let output = res?;
                            let mut record = parse_correctness_record(output.content)?;
                            record.prompt = prompt;
                            record.expected = Some(expected);
                            record.actual_output = Some(actual_output);
                            record.references = references;
                            record.duration_ms = duration_ms;
                            record.input_tokens = input_tokens;
                            record.output_tokens = output_tokens;
                            Ok(record)
                        },
                    )
                    .collect::<Result<Vec<Record>, OxyError>>()?;

                // Errored runs count as FAILs so the denominator is correct
                for (error_msg, expected) in &errors_with_expected {
                    records.push(Record {
                        cot: format!("Run failed with error: {error_msg}"),
                        choice: "FAIL".to_string(),
                        score: 0.0,
                        prompt: expected.task_description.clone(),
                        expected: Some(expected.output.clone()),
                        actual_output: Some(format!("[ERROR] {error_msg}")),
                        references: vec![],
                        duration_ms: 0.0,
                        input_tokens: 0,
                        output_tokens: 0,
                    });
                }

                Ok(MetricKind::Correctness(Correctness::from_records(records)))
            }
        }
    }
}
