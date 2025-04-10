use std::collections::HashMap;

use itertools::Itertools;
use minijinja::{Value, context};
use tokio::task::JoinHandle;

use crate::{
    agent::{OpenAIExecutableResponse, build_openai_executable, types::AgentInput},
    config::{
        constants::EVAL_SOURCE,
        model::{Consistency, EvalKind, Task, TaskType},
    },
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::{
            ExecutableBuilder, concurrency::ConcurrencyControl, map::ParamMapper,
            utils::ConsistencyMapper,
        },
        types::{EventKind, Output, TargetOutput},
        writer::OrderedWriter,
    },
    theme::StyledText,
    workflow::WorkflowInput,
};

use super::target::TargetExecutable;

pub struct EvalInput {
    pub target_ref: String,
    pub quiet: bool,
}

#[derive(Clone, Debug)]
pub(super) enum EvalTarget {
    Workflow(WorkflowInput),
    Agent(AgentInput),
}

impl std::fmt::Display for EvalTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvalTarget::Workflow(workflow_input) => write!(f, "{}", workflow_input.workflow_ref),
            EvalTarget::Agent(agent_input) => write!(f, "{}", agent_input.agent_ref),
        }
    }
}

#[derive(Clone, Debug)]
enum MetricKind {
    Accuracy(f32),
}

impl std::fmt::Display for MetricKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetricKind::Accuracy(accuracy) => write!(f, "Accuracy: {:.2}%", accuracy * 100.0),
        }
    }
}

#[derive(Debug)]
pub struct Record {
    pub cot: String,
    pub choice: String,
    pub score: f32,
}

impl std::fmt::Display for Record {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let reason = self
            .cot
            .replace("---", &format!("{}", "---".error()))
            .replace("+++", &format!("{}", "+++".success()));

        if self.choice.trim() == "B" {
            writeln!(f, "{}", "Inconsistent result detected.".warning())?;
        }
        writeln!(f, "{}", &reason)
    }
}

impl Record {
    pub fn fill_score(&mut self, scores: &HashMap<String, f32>) {
        if let Some(score) = scores.get(&self.choice) {
            self.score = *score;
        }
    }
}

impl TryFrom<Output> for Record {
    type Error = OxyError;

    fn try_from(value: Output) -> Result<Self, Self::Error> {
        let record = Record {
            cot: String::new(),
            choice: String::new(),
            score: 0.0,
        };
        let response = match value {
            Output::Text(text) => text,
            _ => {
                return Err(OxyError::RuntimeError(
                    "Unsupported output type".to_string(),
                ));
            }
        };
        let record = response.trim().lines().fold(record, |mut record, part| {
            record.cot.push_str(&record.choice);
            record.cot.push('\n');
            record.choice = part.trim().to_string();
            record
        });
        Ok(record)
    }
}

#[derive(Debug)]
pub struct Metric {
    errors: Vec<String>,
    records: Vec<Record>,
    kind: MetricKind,
}

impl Metric {
    pub fn set_errors(&mut self, errors: Vec<String>) {
        self.errors = errors;
    }
}

impl std::fmt::Display for Metric {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.errors.is_empty() {
            writeln!(
                f,
                "{}",
                format!("\nFailed to generate {} outputs:\n", self.errors.len()).warning()
            )?;
            writeln!(f, "**********\n")?;
            for error in &self.errors {
                writeln!(f, "{}", error)?;
                writeln!(f, "**********\n")?;
            }
            writeln!(f)?;
        }
        let mut is_header_printed = false;
        for record in &self.records {
            if record.score < 1.0 {
                if !is_header_printed {
                    writeln!(f, "{}\n", "FAILURES:".error())?;
                    writeln!(f, "**********\n")?;
                    is_header_printed = true;
                }
                write!(f, "{}", record)?;
                writeln!(f, "**********\n")?;
            }
        }
        Ok(())
    }
}

impl FromIterator<Record> for Metric {
    fn from_iter<T: IntoIterator<Item = Record>>(iter: T) -> Self {
        let records = iter.into_iter().collect::<Vec<_>>();
        let accuracy = records.iter().map(|r| r.score).sum::<f32>() / records.len() as f32;
        Metric {
            errors: vec![],
            records,
            kind: MetricKind::Accuracy(accuracy),
        }
    }
}

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
impl ParamMapper<EvalInput, Vec<(usize, EvalKind, EvalTarget)>> for EvalMapper {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: EvalInput,
    ) -> Result<(Vec<(usize, EvalKind, EvalTarget)>, Option<ExecutionContext>), OxyError> {
        let EvalInput { target_ref, .. } = input;
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
                    .map(|(idx, test)| {
                        (
                            idx,
                            match test {
                                EvalKind::Consistency(consistency) => {
                                    let task_ref = consistency
                                        .task_ref
                                        .clone()
                                        .or_else(|| self.last_task_ref(&workflow.tasks).ok());
                                    EvalKind::Consistency(Consistency {
                                        task_ref,
                                        ..consistency.clone()
                                    })
                                }
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
                    .map(|(idx, test)| {
                        Ok((
                            idx,
                            test.clone(),
                            EvalTarget::Agent(AgentInput {
                                agent_ref: agent_ref.to_string(),
                                prompt: match test {
                                    EvalKind::Consistency(consistency) => consistency.task_description.clone().ok_or(OxyError::ConfigurationError(
                                        "Task description is required for agent consistency evaluation".to_string(),
                                    ))?
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
pub struct EvalConsistencyMapper {
    prompt_template: String,
    task_description: Option<String>,
}

#[async_trait::async_trait]
impl ParamMapper<(TargetOutput, TargetOutput), String> for EvalConsistencyMapper {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: (TargetOutput, TargetOutput),
    ) -> Result<(String, Option<ExecutionContext>), OxyError> {
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
        let prompt = execution_context
            .renderer
            .render_once(&self.prompt_template, context)
            .map_err(|_| {
                OxyError::RuntimeError("Failed to render consistency evaluation prompt".to_string())
            })?;
        Ok((prompt, None))
    }
}

#[derive(Clone, Debug)]
pub struct AgentMetricControl {
    scores: HashMap<String, f32>,
}

#[async_trait::async_trait]
impl ConcurrencyControl<OpenAIExecutableResponse> for AgentMetricControl {
    type Response = Metric;

    async fn handle(
        &self,
        execution_context: &ExecutionContext,
        results_handle: JoinHandle<
            Result<Vec<Result<OpenAIExecutableResponse, OxyError>>, OxyError>,
        >,
        ordered_writer: OrderedWriter,
    ) -> Result<Self::Response, OxyError> {
        let results = {
            let sender = execution_context.writer.clone();
            let events_handle =
                tokio::spawn(async move { ordered_writer.write_sender(sender).await });
            let results = results_handle.await??;
            events_handle.await??;
            results
        };
        let metric = results
            .into_iter()
            .map(|res| {
                let output = res?;
                let mut record = Record::try_from(output.content)?;
                record.fill_score(&self.scores);
                Ok(record)
            })
            .try_collect::<Record, Metric, OxyError>()?;
        Ok(metric)
    }
}

#[derive(Clone, Debug)]
pub struct EvalConsistencyControl {
    consistency: Consistency,
}

#[async_trait::async_trait]
impl ConcurrencyControl<Vec<TargetOutput>> for EvalConsistencyControl {
    type Response = Metric;

    async fn handle(
        &self,
        execution_context: &ExecutionContext,
        results_handle: JoinHandle<Result<Vec<Result<Vec<TargetOutput>, OxyError>>, OxyError>>,
        ordered_writer: OrderedWriter,
    ) -> Result<Self::Response, OxyError> {
        let results = {
            let sender = execution_context.writer.clone();
            let events_handle =
                tokio::spawn(async move { ordered_writer.write_sender(sender).await });
            let results = results_handle.await??;
            events_handle.await??;
            results
        };
        let model_ref = match &self.consistency.model_ref {
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
            .concurrency_control(
                self.consistency.concurrency,
                AgentMetricControl {
                    scores: self.consistency.scores.clone(),
                },
            )
            .map(EvalConsistencyMapper {
                prompt_template: self.consistency.prompt.to_string(),
                task_description: self.consistency.task_description.clone(),
            })
            .executable(agent);
        let errors = results
            .iter()
            .filter(|res| res.is_err())
            .map(|res| res.as_ref().err().unwrap().to_string())
            .collect::<Vec<_>>();
        let outputs = results
            .into_iter()
            .flatten()
            .flatten()
            .collect::<Vec<_>>()
            .into_iter()
            .circular_tuple_windows::<(_, _)>()
            .collect::<Vec<_>>();
        let metric_context = execution_context.with_child_source(
            format!("{}-metrics", execution_context.source.id),
            EVAL_SOURCE.to_string(),
        );
        metric_context
            .write_kind(EventKind::Message {
                message: "🔄Evaluating records".to_string(),
            })
            .await?;
        let mut metric = eval_executable.execute(&metric_context, outputs).await?;
        metric.set_errors(errors);
        Ok(metric)
    }
}

#[derive(Clone, Debug)]
pub struct EvalExecutable;

#[async_trait::async_trait]
impl Executable<(usize, EvalKind, EvalTarget)> for EvalExecutable {
    type Response = Metric;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: (usize, EvalKind, EvalTarget),
    ) -> Result<Self::Response, OxyError> {
        let (idx, eval, target) = input;
        let eval_context =
            execution_context.with_child_source(format!("eval-{}", idx), EVAL_SOURCE.to_string());
        eval_context
            .write_kind(EventKind::Started {
                name: format!("{}::Test{}", target, idx),
            })
            .await?;
        let metric = match &eval {
            EvalKind::Consistency(consistency) => {
                let mut consistency_executable = ExecutableBuilder::new()
                    .map(ConsistencyMapper {
                        sample_size: consistency.n,
                    })
                    .concurrency_control(
                        consistency.concurrency,
                        EvalConsistencyControl {
                            consistency: consistency.clone(),
                        },
                    )
                    .executable(TargetExecutable::new(consistency.task_ref.clone()));
                eval_context
                    .write_kind(EventKind::Message {
                        message: "🔄Generating outputs".to_string(),
                    })
                    .await?;
                consistency_executable.execute(&eval_context, target).await
            }
        }?;
        eval_context
            .write_kind(EventKind::Message {
                message: format!("✅Eval finished with metrics: {}", metric.kind)
                    .primary()
                    .to_string(),
            })
            .await?;
        eval_context
            .write_kind(EventKind::Finished {
                message: format!("{}", metric),
            })
            .await?;
        Ok(metric)
    }
}

pub fn build_eval_executable()
-> impl Executable<EvalInput, Response = Vec<Result<Metric, OxyError>>> {
    ExecutableBuilder::new()
        .map(EvalMapper)
        .concurrency(10)
        .executable(EvalExecutable)
}
