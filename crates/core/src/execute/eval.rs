use futures::future::try_join_all;
use itertools::Itertools;
use minijinja::{value::Kwargs, Value};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tqdm::pbar;

use crate::{
    ai::setup_eval_agent,
    config::{
        model::{Consistency, Eval, FileFormat, Task, TaskType, Workflow},
        ConfigBuilder,
    },
    errors::OxyError,
    utils::find_project_path,
    workflow::executor::WorkflowExecutor,
    StyledText,
};

use super::{
    agent::{build_agent, AgentEvent, AgentInput},
    core::{
        event::Handler,
        run,
        value::{Array, ContextValue},
        Executable, ExecutionContext,
    },
    renderer::{Renderer, TemplateRegister},
    workflow::{WorkflowEvent, WorkflowInput},
};

#[derive(Debug)]
pub enum EvalEvent {
    Started,
    GeneratingOutputs,
    SomeOutputsFailed {
        failed_count: u32,
        err: String,
    },
    EvaluatingRecords,
    Finished {
        metrics: Metrics,
        records: Vec<Record>,
    },
    Workflow(WorkflowEvent),
    Agent(AgentEvent),
}

#[derive(Debug)]
pub struct EvalInput {
    pub eval: Eval,
    pub target: Target,
}

#[derive(Debug)]
pub enum Target {
    Workflow(TargetWorkflow),
    Agent(TargetAgent),
}

#[derive(Debug)]
pub struct TargetWorkflow {
    pub workflow: Workflow,
    pub input: WorkflowInput,
}

impl TargetWorkflow {
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

    pub fn last_task_ref(&self) -> Result<String, OxyError> {
        let task_ref = self.last_task_ref_internal(&self.workflow.tasks);
        if task_ref.is_empty() {
            return Err(OxyError::ConfigurationError(
                "No tasks found in the workflow".to_string(),
            ));
        }
        Ok(task_ref.join("."))
    }
}

#[derive(Debug)]
pub struct TargetAgent {
    pub agent_ref: PathBuf,
    pub input: AgentInput,
}

#[derive(Debug)]
pub struct Record {
    pub cot: String,
    pub choice: String,
    pub score: f32,
}

#[derive(Debug)]
pub enum Metrics {
    Accuracy(f32),
}

pub struct EvalExecutor;

impl TemplateRegister for Eval {
    fn register_template(&self, renderer: &mut Renderer) -> Result<(), OxyError> {
        match self {
            Eval::Consistency(Consistency { prompt, .. }) => {
                renderer.register_template(prompt)?;
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<Vec<EvalInput>, EvalEvent> for EvalExecutor {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, EvalEvent>,
        input: Vec<EvalInput>,
    ) -> Result<(), OxyError> {
        let mut map_executor = execution_context.map_executor();
        map_executor
            .entries(
                input
                    .into_iter()
                    .enumerate()
                    .map(|(idx, item)| {
                        (format!("eval_{}", idx).to_string(), item.eval, item.target)
                    })
                    .collect::<Vec<_>>(),
            )
            .await?;
        map_executor.finish();
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<(), EvalEvent> for Target {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, EvalEvent>,
        input: (),
    ) -> Result<(), OxyError> {
        match self {
            Target::Workflow(workflow) => {
                workflow.execute(execution_context, input).await?;
            }
            Target::Agent(agent) => {
                agent.execute(execution_context, input).await?;
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<(), EvalEvent> for TargetWorkflow {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, EvalEvent>,
        _input: (),
    ) -> Result<(), OxyError> {
        let workflow = &self.workflow;
        let mut child_executor = execution_context.child_executor();
        let map_event = EvalEvent::Workflow;
        let workflow_executor = WorkflowExecutor::new(workflow.clone());
        let ctx = Value::from_serialize(&workflow.variables);
        let res = child_executor
            .execute(
                &workflow_executor,
                self.input.clone(),
                map_event,
                ctx,
                Value::UNDEFINED,
                workflow,
            )
            .await;
        child_executor.finish();
        res
    }
}

#[async_trait::async_trait]
impl Executable<(), EvalEvent> for TargetAgent {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, EvalEvent>,
        _input: (),
    ) -> Result<(), OxyError> {
        if self.input.prompt.is_none() {
            return Err(OxyError::ConfigurationError(
                "Task description is required".to_string(),
            ));
        }
        let config = execution_context.config.clone();
        let mut agent_executor = execution_context.child_executor();
        let (agent, agent_config, global_context) = build_agent(
            &self.agent_ref,
            &FileFormat::Json,
            self.input.prompt.clone(),
            config,
        )
        .await?;

        let map_event = EvalEvent::Agent;
        agent_executor
            .execute(
                &agent,
                self.input.clone(),
                map_event,
                global_context,
                Value::UNDEFINED,
                &agent_config,
            )
            .await?;
        agent_executor.finish();
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<Target, EvalEvent> for Eval {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, EvalEvent>,
        input: Target,
    ) -> Result<(), OxyError> {
        execution_context.notify(EvalEvent::Started).await?;
        match self {
            Eval::Consistency(consistency) => {
                consistency.execute(execution_context, input).await?;
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<Target, EvalEvent> for Consistency {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, EvalEvent>,
        input: Target,
    ) -> Result<(), OxyError> {
        execution_context
            .notify(EvalEvent::GeneratingOutputs)
            .await?;
        let sample_size = self.n;
        let outputs = {
            let mut loop_executor = execution_context.loop_executor();
            let mut inputs = (0..sample_size).map(|_| ()).collect::<Vec<_>>();
            log::info!("Inputs: {:?}", inputs.len());
            let output_progress = Arc::new(Mutex::new(pbar(Some(sample_size))));
            let progress_tracker = || {
                if let Ok(mut output_progress) = output_progress.lock() {
                    if let Err(err) = output_progress.update(1) {
                        eprintln!("{}", err);
                    }
                }
            };
            let res = loop_executor
                .params(
                    &mut inputs,
                    &input,
                    |_| Value::UNDEFINED,
                    self.concurrency,
                    Some(progress_tracker),
                    None,
                )
                .await;
            let mut outputs = loop_executor.eject()?;

            if let Target::Workflow(workflow) = input {
                let task_ref = match &self.task_ref {
                    Some(task_ref) => task_ref.to_string(),
                    None => workflow.last_task_ref()?,
                };
                outputs = Array(outputs).nested_project(&task_ref);
            }

            if sample_size > outputs.len() {
                if let Err(err) = res {
                    execution_context
                        .notify(EvalEvent::SomeOutputsFailed {
                            failed_count: sample_size as u32 - outputs.len() as u32,
                            err: err.to_string(),
                        })
                        .await?;
                }
            }

            outputs
        };

        log::info!("Outputs: {:?}", outputs);

        let model_ref = match &self.model_ref {
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
        let agent = setup_eval_agent(&self.prompt, model_ref)?;
        let agent_ref = &agent;
        let task_description = match &self.task_description {
            Some(task_description) => task_description.to_string(),
            None => match outputs.last() {
                Some(output) => match output {
                    ContextValue::Agent(agent) => agent.prompt.clone(),
                    _ => {
                        return Err(OxyError::ConfigurationError(
                            "No task description found".to_string(),
                        ));
                    }
                },
                None => {
                    return Err(OxyError::ConfigurationError(
                        "No task description found".to_string(),
                    ));
                }
            },
        };

        execution_context
            .notify(EvalEvent::EvaluatingRecords)
            .await?;
        if outputs.len() < 2 {
            return Err(OxyError::RuntimeError(
                "The number of successfully generated outputs must be greater than 2.".to_string(),
            ));
        }

        let (metrics, records) = {
            let prompts = outputs
                .into_iter()
                .map(|v| format!("{v}"))
                .tuple_windows::<(_, _)>()
                .map(|(submission_1, submission_2)| {
                    let context = Value::from(Kwargs::from_iter([
                        ("submission_1", Value::from_safe_string(submission_1)),
                        ("submission_2", Value::from_safe_string(submission_2)),
                        (
                            "task_description",
                            Value::from_safe_string(task_description.clone()),
                        ),
                    ]));
                    execution_context
                        .renderer
                        .render_once(&self.prompt, context)
                })
                .try_collect::<String, Vec<String>, OxyError>()?;
            let records_progress = Arc::new(Mutex::new(pbar(Some(prompts.len()))));
            let records_fut = prompts
                .into_iter()
                .map(move |system_instruction| agent_ref.simple_request(system_instruction))
                .map(|item| {
                    let records_progress = records_progress.clone();
                    async move {
                        let output = item.await;
                        if let Ok(mut records_progress) = records_progress.lock() {
                            if let Err(err) = records_progress.update(1) {
                                eprintln!("{}", err);
                            }
                        }
                        output.map(|r| self.parse_response(&r))
                    }
                });
            let records = try_join_all(records_fut).await?;
            (self.calculate(records.as_slice()), records)
        };

        execution_context
            .notify(EvalEvent::Finished { metrics, records })
            .await?;
        Ok(())
    }
}

impl Consistency {
    fn parse_response(&self, response: &str) -> Record {
        let record = Record {
            cot: String::new(),
            choice: String::new(),
            score: 0.0,
        };
        response.trim().lines().fold(record, |mut record, part| {
            record.cot.push_str(&record.choice);
            record.cot.push('\n');
            record.choice = part.trim().to_string();
            record.score = self.scores.get(part.trim()).unwrap_or(&0.0).to_owned();
            record
        })
    }

    fn calculate(&self, records: &[Record]) -> Metrics {
        let score = 0.0_f32;
        let accuracy = records
            .iter()
            .fold(score, |score, record| score + record.score)
            / records.len() as f32;
        Metrics::Accuracy(accuracy)
    }
}
#[derive(Debug)]
pub struct EvalReceiver {
    quiet: bool,
}

impl EvalReceiver {
    pub fn new(quiet: bool) -> Self {
        EvalReceiver { quiet }
    }
}

impl Handler for EvalReceiver {
    type Event = EvalEvent;

    fn handle(&self, event: &Self::Event) {
        match event {
            EvalEvent::Started => {
                println!("⏳Eval started");
            }
            EvalEvent::GeneratingOutputs => {
                println!("🔄Generating outputs");
            }
            EvalEvent::SomeOutputsFailed { failed_count, err } => {
                println!(
                    "{}",
                    format!("Failed to generate {} outputs:\n{}", failed_count, err).warning()
                );
            }
            EvalEvent::EvaluatingRecords => {
                println!("🔄Evaluating records");
            }
            EvalEvent::Finished { metrics, records } => {
                println!(
                    "{}",
                    format!("✅Eval finished with metrics: {:?}", metrics).primary()
                );
                if !self.quiet {
                    let mut is_header_printed = false;
                    for record in records {
                        if record.score < 1.0 {
                            if !is_header_printed {
                                println!("{}\n", "FAILURES:".error());
                                println!("**********\n");
                                is_header_printed = true;
                            }
                            let reason = record
                                .cot
                                .replace("---", &format!("{}", "---".error()))
                                .replace("+++", &format!("{}", "+++".success()));

                            if record.choice.trim() == "B" {
                                println!("{}", "Inconsistent result detected.".warning());
                            }
                            println!("{}", &reason);
                            println!("**********\n");
                        }
                    }
                }
            }
            EvalEvent::Workflow(workflow_event) => {
                log::debug!("Workflow event: {:?}", workflow_event);
            }
            EvalEvent::Agent(agent_event) => {
                log::debug!("Agent event: {:?}", agent_event);
            }
        }
    }
}

pub async fn run_eval(path: PathBuf, quiet: bool) -> Result<(), OxyError> {
    let config = ConfigBuilder::new()
        .with_project_path(find_project_path()?)?
        .build()
        .await?;
    let eval_inputs = match path.to_str().unwrap_or_default() {
        workflow_path if workflow_path.ends_with(".workflow.yml") => {
            let workflow = config.resolve_workflow(&path).await?;
            workflow
                .tests
                .clone()
                .into_iter()
                .map(|eval| EvalInput {
                    target: Target::Workflow(TargetWorkflow {
                        workflow: workflow.clone(),
                        input: WorkflowInput,
                    }),
                    eval,
                })
                .collect::<Vec<_>>()
        }
        agent_path if agent_path.ends_with(".agent.yml") => {
            let agent = config.resolve_agent(&path).await?;
            agent
                .tests
                .clone()
                .into_iter()
                .map(|eval| EvalInput {
                    target: Target::Agent(TargetAgent {
                        agent_ref: path.clone(),
                        input: match &eval {
                            Eval::Consistency(consistency) => AgentInput {
                                system_instructions: agent.system_instructions.clone(),
                                prompt: consistency.task_description.clone(),
                            },
                        },
                    }),
                    eval,
                })
                .collect::<Vec<_>>()
        }
        _ => {
            return Err(OxyError::ConfigurationError(format!(
                "Invalid file extension: {}. Expected .workflow.yml",
                path.display()
            )));
        }
    };
    let executor = EvalExecutor;
    run(
        &executor,
        eval_inputs,
        Arc::new(config),
        Value::UNDEFINED,
        None,
        EvalReceiver { quiet },
    )
    .await?;
    Ok(())
}
