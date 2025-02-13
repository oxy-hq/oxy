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
        load_config,
        model::{Consistency, Eval, FileFormat, Step, StepType, Workflow},
    },
    errors::OnyxError,
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
    pub fn last_step_ref_internal(&self, steps: &[Step]) -> Vec<String> {
        let mut task_ref = vec![];
        if let Some(step) = steps.last() {
            task_ref.push(step.name.clone());
            if let StepType::LoopSequential(loop_values) = &step.step_type {
                task_ref.extend(self.last_step_ref_internal(&loop_values.steps))
            }
        }
        task_ref
    }

    pub fn last_step_ref(&self) -> Result<String, OnyxError> {
        let task_ref = self.last_step_ref_internal(&self.workflow.steps);
        if task_ref.is_empty() {
            return Err(OnyxError::ConfigurationError(
                "No steps found in the workflow".to_string(),
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
    fn register_template(&self, renderer: &mut Renderer) -> Result<(), OnyxError> {
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
    ) -> Result<(), OnyxError> {
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
    ) -> Result<(), OnyxError> {
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
    ) -> Result<(), OnyxError> {
        let workflow = &self.workflow;
        let mut child_executor = execution_context.child_executor();
        let map_event = |event| EvalEvent::Workflow(event);
        let workflow_executor = WorkflowExecutor::new(workflow.clone());
        child_executor
            .execute(
                &workflow_executor,
                self.input.clone(),
                map_event,
                Value::UNDEFINED,
                Value::UNDEFINED,
                workflow,
            )
            .await?;
        child_executor.finish();
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<(), EvalEvent> for TargetAgent {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, EvalEvent>,
        _input: (),
    ) -> Result<(), OnyxError> {
        if self.input.prompt.is_none() {
            return Err(OnyxError::ConfigurationError(
                "Task description is required".to_string(),
            ));
        }
        let agent_file = execution_context.config.project_path.join(&self.agent_ref);
        let mut agent_executor = execution_context.child_executor();
        let (agent, agent_config, global_context, _) = build_agent(
            Some(&agent_file),
            &FileFormat::Json,
            self.input.prompt.clone(),
        )?;

        let map_event = |event| EvalEvent::Agent(event);
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
    ) -> Result<(), OnyxError> {
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
    ) -> Result<(), OnyxError> {
        execution_context
            .notify(EvalEvent::GeneratingOutputs)
            .await?;
        let sample_size = self.n;
        let mut outputs = {
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
            loop_executor
                .params(
                    &mut inputs,
                    &input,
                    |_| Value::UNDEFINED,
                    self.concurrency,
                    Some(progress_tracker),
                )
                .await?;
            loop_executor.eject()?
        };
        if let Target::Workflow(workflow) = input {
            let task_ref = match &self.task_ref {
                Some(task_ref) => task_ref.to_string(),
                None => workflow.last_step_ref()?,
            };
            outputs = Array(outputs).nested_project(&task_ref);
        }
        log::info!("Outputs: {:?}", outputs);

        let model_ref = match &self.model_ref {
            Some(model_ref) => model_ref.to_string(),
            None => match execution_context.config.default_model() {
                Some(model_ref) => model_ref,
                None => {
                    return Err(OnyxError::ConfigurationError(
                        "No default model found".to_string(),
                    ));
                }
            },
        };
        let agent = setup_eval_agent(&self.prompt, &model_ref)?;
        let agent_ref = &agent;
        let task_description = match &self.task_description {
            Some(task_description) => task_description.to_string(),
            None => match outputs.last() {
                Some(output) => match output {
                    ContextValue::Agent(agent) => agent.prompt.clone(),
                    _ => {
                        return Err(OnyxError::ConfigurationError(
                            "No task description found".to_string(),
                        ));
                    }
                },
                None => {
                    return Err(OnyxError::ConfigurationError(
                        "No task description found".to_string(),
                    ));
                }
            },
        };
        execution_context
            .notify(EvalEvent::EvaluatingRecords)
            .await?;

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
                .try_collect::<String, Vec<String>, OnyxError>()?;
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
    verbose: bool,
}

impl EvalReceiver {
    pub fn new(verbose: bool) -> Self {
        EvalReceiver { verbose }
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
            EvalEvent::EvaluatingRecords => {
                println!("🔄Evaluating records");
            }
            EvalEvent::Finished { metrics, records } => {
                println!(
                    "{}",
                    format!("✅Eval finished with metrics: {:?}", metrics).primary()
                );
                if self.verbose {
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
                            println!("Choice {}:\n\n{}", &record.choice.warning(), &reason);
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

pub async fn run_eval(path: PathBuf, verbose: bool) -> Result<(), OnyxError> {
    let config = load_config(None)?;
    let eval_inputs = match path.to_str().unwrap_or_default() {
        workflow_path if workflow_path.ends_with(".workflow.yml") => {
            let workflow = config.load_workflow(&path)?;
            config.validate_workflow(&workflow).map_err(|e| {
                OnyxError::ConfigurationError(format!("Invalid workflow configuration: {}", e))
            })?;
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
            let (agent, _) = config.load_agent_config(Some(&path))?;
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
            return Err(OnyxError::ConfigurationError(format!(
                "Invalid file extension: {}. Expected .workflow.yml",
                path.display()
            )));
        }
    };
    let executor = EvalExecutor;
    run(
        &executor,
        eval_inputs,
        config,
        Value::UNDEFINED,
        None,
        EvalReceiver { verbose },
    )
    .await?;
    Ok(())
}
