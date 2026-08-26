use futures::stream::StreamExt;
use itertools::Itertools;

use crate::integrations::eval::builders::types::EvalRecord;
use oxy::{
    config::model::EvalKind,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, utils::ConsistencyMapper},
        types::{EventKind, ProgressType, RelevantContextGetter, TargetOutput, Usage},
    },
    utils::asyncify,
};
use oxy_shared::errors::OxyError;

use super::{target::TargetExecutable, target_agentic::run_target, types::AgenticInput};

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
impl Executable<(EvalKind, AgenticInput, Option<String>)> for GeneratorExecutable {
    /// (successful pairs, errored pairs: (error_message, expected_output))
    type Response = (
        Vec<(TargetOutput, TargetOutput)>,
        Vec<(String, TargetOutput)>,
    );

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        (eval_kind, eval_target, task_ref): (EvalKind, AgenticInput, Option<String>),
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
                let config_manager = &execution_context.workspace.config_manager;
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
                    .map(|record| record.as_target(&eval_target))
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
                        all_targets.push(AgenticInput {
                            config_path: eval_target.config_path.clone(),
                            prompt: case.prompt.clone(),
                        });
                        expected_outputs.push(expected.clone());
                    }
                }

                // Execute all runs concurrently on the agentic path, with no old
                // executor pipeline. `run_target` is scaffolding-free (see
                // `target_agentic::run_target`), so the two side-effects the old
                // `ExecutableBuilder`/`ConcurrencyWrapper` produced are re-emitted
                // here at the call site, where the `ExecutionContext` lives:
                //   - progress events (Started/Updated/Finished) — drive the CLI bar
                //     and the Test Dashboard SSE stream (keyed on this eval source);
                //   - per-run `EventKind::Usage` — folded into run-level `TokenStats`
                //     by `EvalEventsHandler` (per-case tokens ride on `TargetOutput`).
                // `buffered` preserves input order so results still line up with
                // `expected_outputs` below. `task_ref` is always `None` here and the
                // agentic target yields empty relevant-context, so the getter is moot.
                let total = all_targets.len();
                // Telemetry writes are best-effort: a failed progress/usage emit
                // (only reachable on a closed receiver) must not cancel in-flight
                // target runs or skip `Finished`, so log rather than propagate.
                if let Err(e) = execution_context
                    .write_progress(ProgressType::Started(Some(total)))
                    .await
                {
                    tracing::warn!("eval: failed to emit target progress Started: {e}");
                }
                let workspace = execution_context.workspace.clone();
                let mut stream = futures::stream::iter(all_targets)
                    .map(|input| {
                        let workspace = workspace.clone();
                        async move { run_target(&workspace, input).await }
                    })
                    .buffered(self.concurrency.max(1));
                let mut results: Vec<Result<Vec<TargetOutput>, OxyError>> =
                    Vec::with_capacity(total);
                // `Updated(1)` fires in input order as `buffered` yields (not at real
                // completion) — deliberate: it keeps the `case n/m` label monotonic.
                while let Some(result) = stream.next().await {
                    if let Ok(outputs) = &result {
                        for out in outputs {
                            if let Err(e) = execution_context
                                .write_kind(EventKind::Usage {
                                    usage: Usage::new(out.input_tokens, out.output_tokens),
                                })
                                .await
                            {
                                tracing::warn!("eval: failed to emit target token usage: {e}");
                            }
                        }
                    }
                    if let Err(e) = execution_context
                        .write_progress(ProgressType::Updated(1))
                        .await
                    {
                        tracing::warn!("eval: failed to emit target progress Updated: {e}");
                    }
                    results.push(result);
                }
                if let Err(e) = execution_context
                    .write_progress(ProgressType::Finished)
                    .await
                {
                    tracing::warn!("eval: failed to emit target progress Finished: {e}");
                }

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
