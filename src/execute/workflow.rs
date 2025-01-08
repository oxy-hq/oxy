use std::{path::PathBuf, time::Duration};

use minijinja::Value;

use crate::{
    config::{
        load_config,
        model::{AgentStep, LoopValues, Step, StepType, Workflow},
    },
    errors::OnyxError,
    utils::print_colored_sql,
    workflow::{executor::WorkflowExecutor, WorkflowResult},
    StyledText,
};

use super::{
    agent::ToolCall,
    core::{
        event::{Dispatcher, Handler},
        write::OutputCollector,
        Executable, ExecutionContext,
    },
    renderer::{Renderer, TemplateRegister},
};

#[derive(Debug, Clone)]
pub enum WorkflowEvent {
    // workflow
    Started {
        name: String,
    },
    Finished,

    // step
    StepStarted {
        name: String,
    },
    StepUnknown {
        name: String,
    },
    Retry {
        err: OnyxError,
        after: Duration,
    },

    // agent
    AgentToolCalls {
        calls: Vec<ToolCall>,
        step: AgentStep,
    },

    // sql
    ExecuteSQL {
        query: String,
        output: String,
    },

    // formatter
    Formatter {
        output: String,
    },
}

impl TemplateRegister for Workflow {
    fn register_template(&self, renderer: &mut Renderer) -> Result<(), OnyxError> {
        renderer.register(&self.steps)
    }
}

impl TemplateRegister for &Step {
    fn register_template(&self, renderer: &mut Renderer) -> Result<(), OnyxError> {
        let mut register = renderer.struct_register();
        match &self.step_type {
            StepType::Agent(agent) => {
                register.field(&agent.prompt.as_str())?;
            }
            StepType::ExecuteSQL(execute_sql) => {
                register.field(&execute_sql.sql_file.as_str())?;
                match &execute_sql.variables {
                    Some(variables) => {
                        register.fields(
                            variables
                                .iter()
                                .map(|(_key, value)| value.as_str())
                                .collect::<Vec<&str>>(),
                        )?;
                    }
                    None => {}
                }
            }
            StepType::Formatter(formatter) => {
                register.field(&formatter.template.as_str())?;
            }
            StepType::LoopSequential(loop_sequential) => {
                match &loop_sequential.values {
                    LoopValues::Template(template) => {
                        register.field(&template.as_str())?;
                    }
                    _ => {}
                }
                register.field(&loop_sequential.steps)?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl TemplateRegister for Vec<Step> {
    fn register_template(&self, renderer: &mut Renderer) -> Result<(), OnyxError> {
        let mut list_register = renderer.list_register();
        list_register.items(self)?;
        Ok(())
    }
}

pub struct WorkflowReceiver;

impl Handler for WorkflowReceiver {
    type Event = WorkflowEvent;

    fn handle(&self, event: &Self::Event) {
        match event {
            WorkflowEvent::Started { name } => {
                println!("\n⏳Running workflow: {}", name.text());
            }
            WorkflowEvent::ExecuteSQL { query, output } => {
                print_colored_sql(&query);
                println!("{}", "\nResults:".primary());
                println!("{}", output);
            }
            WorkflowEvent::Formatter { output } => {
                println!("{}", "\nOutput:".primary());
                println!("{}", output);
            }
            WorkflowEvent::StepStarted { name } => {
                println!("\n⏳Starting {}", name.text());
            }
            WorkflowEvent::StepUnknown { name } => {
                println!(
                    "{}",
                    format!("Encountered unknown step {name}. Skipping.").warning()
                );
            }
            WorkflowEvent::Finished => {
                println!("{}", "\n✅Workflow executed successfully".success());
            }
            WorkflowEvent::Retry { err, after } => {
                println!("{}", format!("\nRetrying after {:?} ...", after).warning());
                println!("Reason {:?}", err);
            }
            _ => {
                log::debug!("Unhandled event: {:?}", event);
            }
        }
    }
}

pub struct WorkflowExporter;

impl Handler for WorkflowExporter {
    type Event = WorkflowEvent;

    fn handle(&self, event: &Self::Event) {
        match event {
            WorkflowEvent::AgentToolCalls { calls: _, step: _ } => {
                // @TODO: Implement export logic for agent step
                log::debug!("Agent tool calls: {:?}", event);
            }
            _ => {
                log::debug!("Unhandled event: {:?}", event);
            }
        }
    }
}

pub async fn run_workflow(workflow_path: &PathBuf) -> Result<WorkflowResult, OnyxError> {
    let config = load_config()?;
    let workflow = config.load_workflow(workflow_path)?;
    config.validate_workflow(&workflow).map_err(|e| {
        OnyxError::ConfigurationError(format!("Invalid workflow configuration: {}", e))
    })?;

    let mut renderer = Renderer::new();
    renderer.register(&workflow)?;
    let dispatcher = Dispatcher::new(vec![Box::new(WorkflowReceiver), Box::new(WorkflowExporter)]);
    let mut output_collector = OutputCollector::new(&dispatcher);
    let mut execution_context = ExecutionContext::new(
        Value::UNDEFINED,
        &mut renderer,
        &Value::UNDEFINED,
        &mut output_collector,
    );
    let executor = WorkflowExecutor::new(workflow);
    executor.execute(&mut execution_context).await?;
    let output = output_collector.output.unwrap_or_default();
    Ok(WorkflowResult { output })
}
