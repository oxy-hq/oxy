use itertools::Itertools;

use crate::integrations::eval::builders::types::EvalRecord;
use oxy::{
    config::model::EvalKind,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, utils::ConsistencyMapper},
        types::{RelevantContextGetter, TargetOutput},
    },
    utils::asyncify,
};
use oxy_agent::types::AgentInput;
use oxy_shared::errors::OxyError;
use oxy_workflow::builders::WorkflowInput;

use super::{target::TargetExecutable, types::EvalTarget};

#[derive(Clone, Debug)]
pub(super) struct GeneratorExecutable {
    concurrency: usize,
}

impl GeneratorExecutable {
    pub fn new(concurrency: usize) -> Self {
        Self { concurrency }
    }
}

#[async_trait::async_trait]
impl Executable<(EvalKind, EvalTarget, Option<String>)> for GeneratorExecutable {
    /// (successful pairs, errored pairs: (error_message, expected_output))
    type Response = (
        Vec<(TargetOutput, TargetOutput)>,
        Vec<(String, TargetOutput)>,
    );

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        (eval_kind, eval_target, task_ref): (EvalKind, EvalTarget, Option<String>),
    ) -> Result<Self::Response, OxyError> {
        match &eval_kind {
            EvalKind::Consistency(consistency) => {
                let mut consistency_executable = ExecutableBuilder::new()
                    .map(ConsistencyMapper {
                        sample_size: consistency.n,
                    })
                    .concurrency(self.concurrency)
                    .executable(TargetExecutable::new(task_ref, RelevantContextGetter::Id));
                let results = consistency_executable
                    .execute(execution_context, eval_target)
                    .await?;
                let errors = results
                    .iter()
                    .filter_map(|res| match res {
                        Ok(_) => None,
                        // Consistency errors have no associated expected output
                        Err(err) => Some((err.to_string(), TargetOutput::default())),
                    })
                    .collect::<Vec<_>>();
                let outputs = results
                    .into_iter()
                    .filter_map(|res| res.ok())
                    .flatten()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .circular_tuple_windows::<(_, _)>()
                    .collect::<Vec<_>>();
                Ok((outputs, errors))
            }

            EvalKind::Custom(custom) => {
                let config_manager = &execution_context.project.config_manager;
                let dataset_path = config_manager.resolve_file(&custom.dataset).await?;

                let records = asyncify(move || {
                    let rdr = std::fs::File::open(dataset_path).map_err(|err| {
                        OxyError::RuntimeError(format!("Failed to open file: {err}"))
                    })?;
                    let records: Vec<EvalRecord> = serde_yaml::from_reader(rdr).map_err(|err| {
                        OxyError::SerializerError(format!(
                            "Failed to deserialize EvalRecord: {err}"
                        ))
                    })?;
                    Ok(records)
                })
                .await?;
                let relevant_context_getter = if custom.is_context_id {
                    RelevantContextGetter::Id
                } else {
                    RelevantContextGetter::Content
                };
                let mut target_executable = ExecutableBuilder::new()
                    .concurrency(self.concurrency)
                    .executable(TargetExecutable::new(task_ref, relevant_context_getter));
                let inputs = records
                    .iter()
                    .map(|record| record.as_target(&eval_target, &custom.workflow_variable_name))
                    .collect::<Vec<_>>();
                let results = target_executable
                    .execute(execution_context, inputs)
                    .await?
                    .into_iter()
                    .zip(records.iter())
                    .map(|(res, record)| {
                        res.map(|outputs| {
                            outputs
                                .into_iter()
                                .map(|output| (output, Into::<TargetOutput>::into(record.clone())))
                                .collect::<Vec<_>>()
                        })
                    })
                    .collect::<Vec<_>>();
                let errors = results
                    .iter()
                    .zip(records.iter())
                    .filter_map(|(res, record)| match res {
                        Ok(_) => None,
                        Err(err) => {
                            Some((err.to_string(), Into::<TargetOutput>::into(record.clone())))
                        }
                    })
                    .collect::<Vec<_>>();
                let outputs = results
                    .into_iter()
                    .filter_map(|res| res.ok())
                    .flatten()
                    .collect::<Vec<_>>();

                Ok((outputs, errors))
            }

            EvalKind::TestCase(test_case_eval) => {
                let runs = test_case_eval.runs;

                // Flatten all cases × runs into a single concurrent batch
                let mut all_targets = Vec::new();
                let mut expected_outputs = Vec::new();

                for case in &test_case_eval.cases {
                    let expected = TargetOutput {
                        output: case.expected.clone(),
                        task_description: Some(case.prompt.clone()),
                        relevant_contexts: vec![],
                        references: vec![],
                        duration_ms: 0.0,
                        input_tokens: 0,
                        output_tokens: 0,
                    };

                    for _ in 0..runs {
                        let target = match &eval_target {
                            EvalTarget::Agent(agent_input) => EvalTarget::Agent(AgentInput {
                                agent_ref: agent_input.agent_ref.clone(),
                                prompt: case.prompt.clone(),
                                memory: vec![],
                                variables: None,
                                a2a_task_id: None,
                                a2a_thread_id: None,
                                a2a_context_id: None,
                                sandbox_info: None,
                            }),
                            EvalTarget::Workflow(workflow_input) => {
                                EvalTarget::Workflow(WorkflowInput {
                                    workflow_ref: workflow_input.workflow_ref.clone(),
                                    retry: oxy::checkpoint::types::RetryStrategy::NoRetry {
                                        variables: None,
                                    },
                                })
                            }
                            EvalTarget::Agentic(agentic_input) => {
                                EvalTarget::Agentic(super::types::AgenticInput {
                                    config_path: agentic_input.config_path.clone(),
                                    prompt: case.prompt.clone(),
                                })
                            }
                        };
                        all_targets.push(target);
                        expected_outputs.push(expected.clone());
                    }
                }

                // Execute all runs concurrently in a single batch
                let mut target_executable = ExecutableBuilder::new()
                    .concurrency(self.concurrency)
                    .executable(TargetExecutable::new(task_ref, RelevantContextGetter::Id));
                let results = target_executable
                    .execute(execution_context, all_targets)
                    .await?;

                // Pair results back with their expected outputs
                let mut all_outputs = Vec::new();
                let mut all_errors = Vec::new();
                for (result, expected) in results.into_iter().zip(expected_outputs) {
                    match result {
                        Ok(actual_outputs) => {
                            for actual in actual_outputs {
                                all_outputs.push((actual, expected.clone()));
                            }
                        }
                        Err(err) => {
                            // Pair the error with its expected output so the solver
                            // can count it as a FAIL against the correct denominator.
                            all_errors.push((err.to_string(), expected));
                        }
                    }
                }

                Ok((all_outputs, all_errors))
            }
        }
    }
}
