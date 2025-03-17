use indoc::indoc;
use itertools::Itertools;
use minijinja::Value;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
};
use tokio::sync::mpsc::{Receiver, Sender};

use crate::{
    ai::setup_eval_agent,
    errors::OxyError,
    execute::workflow::{WorkflowEvent, WorkflowInput},
    theme::StyledText,
};

use super::core::{
    event::Handler, value::ContextValue, write::Write, Executable, ExecutionContext,
};

use tqdm::{pbar, Pbar};

#[derive(Debug, Clone)]
pub enum ConsistencyEvent {
    StartedProgress { total: usize, title: String },
    Progress { progress: usize },
    EndProgress,
    LowConsistencyDetected { consistency: f64 },
}

pub struct ConsistencyReceiver {
    output_progress: Arc<RwLock<Option<Arc<Mutex<Pbar>>>>>,
}

impl Default for ConsistencyReceiver {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsistencyReceiver {
    pub fn new() -> Self {
        Self {
            output_progress: Arc::new(RwLock::new(None)),
        }
    }
}

impl Handler for ConsistencyReceiver {
    type Event = ConsistencyEvent;

    fn handle(&self, event: &Self::Event) {
        match &event {
            ConsistencyEvent::StartedProgress { total, title } => {
                println!("{}", title.primary());
                match self.output_progress.write() {
                    Ok(mut progress) => {
                        *progress = Some(Arc::new(Mutex::new(pbar(Some(*total)))));
                    }
                    Err(err) => {
                        log::error!("Failed to acquire write lock for progress bar: {}", err)
                    }
                }
            }

            ConsistencyEvent::Progress { progress } => match self.output_progress.read() {
                Ok(progress_guard) => match progress_guard.as_ref() {
                    Some(output_progress) => match output_progress.lock() {
                        Ok(mut progress_bar) => {
                            if let Err(err) = progress_bar.update(*progress) {
                                log::error!("Failed to update progress bar: {}", err);
                            }
                        }
                        Err(err) => println!("Failed to acquire progress bar lock: {}", err),
                    },
                    None => log::error!("{}", "Progress bar is not initialized"),
                },
                Err(err) => log::error!("Failed to acquire read lock for progress bar: {}", err),
            },
            ConsistencyEvent::EndProgress => match self.output_progress.write() {
                Ok(mut progress) => {
                    *progress = None;
                }
                Err(err) => log::error!("Failed to acquire write lock for progress bar: {}", err),
            },
            ConsistencyEvent::LowConsistencyDetected { consistency } => {
                println!(
                    "{}",
                    format!(
                        "Warning: results for this step are not consistent. Try testing this step in isolation and reworking the prompt. Consistency: {}%.",
                        consistency * 100.0
                    )
                );
            }
        }
    }
}

#[derive(Clone)]
pub struct ConsistencyAdapterState {
    result: ContextValue,
}

pub struct ConsistencyAdapter<'state> {
    state: &'state mut ConsistencyAdapterState,
}

impl Write for ConsistencyAdapter<'_> {
    fn write(&mut self, value: ContextValue) {
        self.state.result = value;
    }
}

pub struct ConsistencyExecutor {
    consistency_state: ConsistencyAdapterState,
    collected_events: Vec<WorkflowEvent>,
}

impl Default for ConsistencyExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsistencyExecutor {
    pub fn new() -> Self {
        Self {
            consistency_state: ConsistencyAdapterState {
                result: Default::default(),
            },
            collected_events: Vec::new(),
        }
    }

    pub async fn execute(
        &mut self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
        entry: &dyn Executable<WorkflowInput, WorkflowEvent>,
        task_description: String,
        n: usize,
    ) -> Result<(), OxyError> {
        let (event_sender, mut event_receiver) = tokio::sync::mpsc::channel(100);

        let outputs = self
            .generate_outputs(execution_context, entry, n, event_sender)
            .await?;

        let (prompts, comparison_pairs) =
            self.prepare_comparisons(&outputs, &task_description, execution_context)?;

        let consistency_counts = self
            .evaluate_outputs(execution_context, prompts, &comparison_pairs)
            .await?;

        self.process_results(
            execution_context,
            outputs,
            consistency_counts,
            &mut event_receiver,
        )
        .await?;

        Ok(())
    }

    async fn generate_outputs(
        &mut self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
        entry: &dyn Executable<WorkflowInput, WorkflowEvent>,
        sample_size: usize,
        event_sender: Sender<(usize, WorkflowEvent)>,
    ) -> Result<Vec<(usize, ContextValue)>, OxyError> {
        let mut consistency_adapter = ConsistencyAdapter {
            state: &mut self.consistency_state,
        };

        let parts = execution_context.clone_parts();
        let mut context = ExecutionContext::from_parts(parts, &mut consistency_adapter);

        execution_context
            .notify(WorkflowEvent::Consistency {
                orig: ConsistencyEvent::StartedProgress {
                    total: sample_size,
                    title: "🔄Generating outputs".to_string(),
                },
            })
            .await?;

        let outputs = {
            let mut loop_executor = context.loop_executor();
            let mut inputs = (0..sample_size)
                .map(|_| WorkflowInput)
                .collect::<Vec<WorkflowInput>>();

            let progress_tracker = || {
                let _ = execution_context
                    .get_sender()
                    .try_send(WorkflowEvent::Consistency {
                        orig: ConsistencyEvent::Progress { progress: 1 },
                    });
            };

            let res = loop_executor
                .params(
                    &mut inputs,
                    entry,
                    |_| Value::UNDEFINED,
                    sample_size,
                    Some(progress_tracker),
                    Some(event_sender),
                )
                .await;

            let outputs = loop_executor.eject_results()?;

            execution_context
                .notify(WorkflowEvent::Consistency {
                    orig: ConsistencyEvent::EndProgress,
                })
                .await?;

            if sample_size > outputs.len() {
                if let Err(err) = res {
                    return Err(OxyError::RuntimeError(format!(
                        "Failed to generate {} outputs, {:?}",
                        sample_size - outputs.len(),
                        err.to_string()
                    )));
                }
            }

            outputs
        };

        Ok(outputs)
    }

    fn prepare_comparisons(
        &mut self,
        outputs: &[(usize, ContextValue)],
        task_description: &str,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
    ) -> Result<(Vec<String>, Vec<(usize, usize)>), OxyError> {
        let prompts = outputs
            .iter()
            .map(|(_, v)| format!("{v}"))
            .array_combinations::<2>()
            .map(|submissions| {
                let context = minijinja::context! {
                    submission_1 => submissions[0],
                    submission_2 => submissions[1],
                    task_description => task_description,
                };
                execution_context.renderer.render_once(PROMPT, context)
            })
            .collect::<Result<Vec<String>, _>>()?;

        let comparison_pairs: Vec<(usize, usize)> = outputs
            .iter()
            .enumerate()
            .array_combinations::<2>()
            .map(|[(i1, _), (i2, _)]| (i1, i2))
            .collect();

        Ok((prompts, comparison_pairs))
    }

    async fn evaluate_outputs(
        &mut self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
        prompts: Vec<String>,
        comparison_pairs: &[(usize, usize)],
    ) -> Result<HashMap<usize, i32>, OxyError> {
        execution_context
            .notify(WorkflowEvent::Consistency {
                orig: ConsistencyEvent::StartedProgress {
                    total: prompts.len(),
                    title: "🔄Evaluating records".to_string(),
                },
            })
            .await?;

        let model_ref = execution_context
            .config
            .default_model()
            .ok_or_else(|| OxyError::ConfigurationError("No default model found".to_string()))?;
        let agent = setup_eval_agent(PROMPT, model_ref)?;

        let consistency_counts = Arc::new(Mutex::new(HashMap::<usize, i32>::new()));
        let context_sender = execution_context.get_sender();

        let futures = prompts
            .into_iter()
            .enumerate()
            .map(|(i, system_instruction)| (i, agent.simple_request(system_instruction)))
            .map(|(i, item)| {
                let comparison_pairs = comparison_pairs.to_owned();
                let consistency_counts = consistency_counts.clone();
                let context_sender = context_sender.clone();

                async move {
                    let output = item.await;
                    let _ = context_sender.try_send(WorkflowEvent::Consistency {
                        orig: ConsistencyEvent::Progress { progress: 1 },
                    });

                    let result = parse_consistency_response(&output.unwrap());

                    if result == "A" {
                        let (i1, i2) = comparison_pairs[i];
                        if let Ok(mut counts) = consistency_counts.lock() {
                            *counts.entry(i1).or_insert(0) += 1;
                            *counts.entry(i2).or_insert(0) += 1;
                        }
                    }
                }
            });

        futures::future::join_all(futures).await;

        execution_context
            .notify(WorkflowEvent::Consistency {
                orig: ConsistencyEvent::EndProgress,
            })
            .await?;

        let consistency_counts = consistency_counts
            .lock()
            .map_err(|_| {
                OxyError::RuntimeError("Failed to acquire consistency counts lock".to_string())
            })?
            .clone();

        Ok(consistency_counts)
    }

    async fn process_results(
        &mut self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
        outputs: Vec<(usize, ContextValue)>,
        consistency_counts: HashMap<usize, i32>,
        event_receiver: &mut Receiver<(usize, WorkflowEvent)>,
    ) -> Result<(), OxyError> {
        let total_comparisons = outputs.len() * (outputs.len() - 1) / 2;
        let mut highest_consistency = 0.0;
        let mut most_consistent_output = None;

        for (i, output) in outputs.iter().enumerate() {
            let consistency_score =
                *consistency_counts.get(&i).unwrap_or(&0) as f64 / total_comparisons as f64;
            if consistency_score > highest_consistency {
                highest_consistency = consistency_score;
                most_consistent_output = Some(output.clone());
            }
        }

        while let Ok(event) = event_receiver.try_recv() {
            if let Some((index, _)) = most_consistent_output {
                if index == event.0 {
                    self.collected_events.push(event.1);
                }
            }
        }

        if highest_consistency < 0.25 {
            execution_context
                .notify(WorkflowEvent::Consistency {
                    orig: ConsistencyEvent::LowConsistencyDetected {
                        consistency: highest_consistency,
                    },
                })
                .await?;
        }

        if let Some(output) = most_consistent_output {
            self.consistency_state.result = output.1;
        }

        execution_context.write(self.consistency_state.result.clone());

        for event in self.collected_events.clone() {
            execution_context.notify(event).await?;
        }

        Ok(())
    }
}

const PROMPT: &str = indoc! {"
    You are comparing a pair of submitted answers on a given question. Here is the data:
    [BEGIN DATA]
    ************
    [Question]: {{ task_description }}
    ************
    [Submission 1]: {{submission_1}}
    ************
    [Submission 2]: {{submission_2}}
    ************
    [END DATA]

    Compare the factual content of the submitted answers. Ignore any differences in style, grammar, punctuation. Answer the question by selecting one of the following options:
    A. The submitted answers are either a superset or contains each other and is fully consistent with it.
    B. There is a disagreement between the submitted answers.

    - First, highlight the disagreements between the two submissions.
    Following is the syntax to highlight the differences:

    (1) <factual_content>
    +++ <submission_1_factual_content_diff>
    --- <submission_2_factual_content_diff>

    [BEGIN EXAMPLE]
    Here are the key differences between the two submissions:
    (1) Capital of France
    +++ Paris
    --- France
    [END EXAMPLE]

    - Then reason about the highlighted differences. The submitted answers may either be a subset or superset of each other, or it may conflict. Determine which case applies.
    - At the end, print only a single choice from AB (without quotes or brackets or punctuation) on its own line corresponding to the correct answer. e.g A

    Reasoning:
"};

fn parse_consistency_response(response: &str) -> String {
    for line in response.lines().rev() {
        let trimmed = line.trim();
        if trimmed == "A" || trimmed == "B" {
            return trimmed.to_string();
        }
    }
    "B".to_string()
}
