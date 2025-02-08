use std::{path::PathBuf, sync::Arc, time::Duration};

use arrow::{array::RecordBatch, datatypes::Schema};
use minijinja::Value;

use crate::{
    ai::utils::record_batches_to_table,
    config::{
        load_config,
        model::{
            AgentStep, ExecuteSQLStep, FormatterStep, LoopValues, Step, StepType, Workflow, SQL,
        },
    },
    errors::OnyxError,
    execute::exporter::{export_agent_step, export_execute_sql, export_formatter},
    utils::print_colored_sql,
    workflow::{cache::write_agent_cache, executor::WorkflowExecutor, WorkflowResult},
    StyledText,
};

use super::{
    agent::{AgentEvent, AgentReceiver},
    core::{
        event::{Dispatcher, Handler},
        run,
        value::ContextValue,
    },
    renderer::{Renderer, TemplateRegister},
};

#[derive(Debug, Clone)]
pub struct WorkflowInput;

pub struct LoopInput {
    pub name: String,
}

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

    // export
    Export {
        export_file_path: PathBuf,
        step: StepType,
    },

    // agent
    Agent {
        orig: AgentEvent,
        step: AgentStep,
        export_file_path: Option<PathBuf>,
    },

    // sql
    ExecuteSQL {
        step: ExecuteSQLStep,
        query: String,
        datasets: Vec<RecordBatch>,
        schema: Arc<Schema>,
        export_file_path: PathBuf,
    },

    // formatter
    Formatter {
        step: FormatterStep,
        output: String,
        export_file_path: PathBuf,
    },

    // agent
    CacheAgentResult {
        result: ContextValue,
        file_path: PathBuf,
    },
}

impl TemplateRegister for Workflow {
    fn register_template(&self, renderer: &mut Renderer) -> Result<(), OnyxError> {
        renderer.register(&self.steps)
    }
}

impl TemplateRegister for &Step {
    fn register_template(&self, renderer: &mut Renderer) -> Result<(), OnyxError> {
        let mut register = renderer.child_register();
        match &self.step_type {
            StepType::Agent(agent) => {
                register.entry(&agent.prompt.as_str())?;
                if let Some(export) = &agent.export {
                    register.entry(&export.path.as_str())?;
                }

                if let Some(cache) = &agent.cache {
                    register.entry(&cache.path.as_str())?;
                }
            }
            StepType::ExecuteSQL(execute_sql) => {
                let sql = match &execute_sql.sql {
                    SQL::Query { sql_query } => sql_query,
                    SQL::File { sql_file } => sql_file,
                };
                register.entry(&sql.as_str())?;
                match &execute_sql.variables {
                    Some(variables) => {
                        register.entries(
                            variables
                                .iter()
                                .map(|(_key, value)| value.as_str())
                                .collect::<Vec<&str>>(),
                        )?;
                    }
                    None => {}
                }
                if let Some(export) = &execute_sql.export {
                    register.entry(&export.path.as_str())?;
                }
            }
            StepType::Formatter(formatter) => {
                register.entry(&formatter.template.as_str())?;
                if let Some(export) = &formatter.export {
                    register.entry(&export.path.as_str())?;
                }
            }
            StepType::LoopSequential(loop_sequential) => {
                if let LoopValues::Template(template) = &loop_sequential.values {
                    register.entry(&template.as_str())?;
                }
                register.entry(&loop_sequential.steps)?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl TemplateRegister for Vec<Step> {
    fn register_template(&self, renderer: &mut Renderer) -> Result<(), OnyxError> {
        let mut child_register = renderer.child_register();
        child_register.entries(self)?;
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
            WorkflowEvent::ExecuteSQL {
                step: _,
                query,
                datasets,
                schema,
                export_file_path: _,
            } => {
                print_colored_sql(&query);

                let batches_display = match record_batches_to_table(&datasets, &schema) {
                    Ok(display) => display,
                    Err(e) => {
                        println!("{}", format!("Error displaying results: {}", e).error());
                        return;
                    }
                };

                println!("{}", "\nResults:".primary());
                println!("{}", batches_display);
            }
            WorkflowEvent::Formatter {
                step: _,
                output,
                export_file_path: _,
            } => {
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
            WorkflowEvent::Agent { orig, .. } => {
                AgentReceiver.handle(orig);
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
            WorkflowEvent::Agent {
                orig: event,
                step,
                export_file_path,
            } => {
                log::debug!("Agent tool calls: {:?}", event);
                match event {
                    AgentEvent::ToolCall(tool_call) => {
                        if let Some(export_file_path) = export_file_path {
                            export_agent_step(step, &[tool_call], export_file_path);
                        }
                    }
                    _ => {}
                }
            }
            WorkflowEvent::ExecuteSQL {
                step,
                query,
                datasets,
                schema,
                export_file_path,
            } => {
                if let Some(export) = &step.export {
                    export_execute_sql(export, "", query, schema, datasets, export_file_path);
                }
                log::debug!("ExecuteSQL tool calls: {:?}", event);
            }
            WorkflowEvent::Formatter {
                step,
                output,
                export_file_path,
            } => {
                if let Some(_) = &step.export {
                    export_formatter(output, export_file_path);
                }
                log::debug!("Formatter tool calls: {:?}", event);
            }
            _ => {
                log::debug!("Unhandled event: {:?}", event);
            }
        }
    }
}

pub struct WorkflowCacheStep;

impl Handler for WorkflowCacheStep {
    type Event = WorkflowEvent;

    fn handle(&self, event: &Self::Event) {
        match event {
            WorkflowEvent::CacheAgentResult { result, file_path } => {
                write_agent_cache(file_path, result);
                log::debug!("Cache agent result: {:?}", event);
            }

            _ => {
                log::debug!("Unhandled event: {:?}", event);
            }
        }
    }
}

pub async fn run_workflow(workflow_path: &PathBuf) -> Result<WorkflowResult, OnyxError> {
    let config = load_config(None)?;
    let workflow = config.load_workflow(workflow_path)?;
    config.validate_workflow(&workflow).map_err(|e| {
        OnyxError::ConfigurationError(format!("Invalid workflow configuration: {}", e))
    })?;

    let dispatcher = Dispatcher::new(vec![
        Box::new(WorkflowReceiver),
        Box::new(WorkflowExporter),
        Box::new(WorkflowCacheStep),
    ]);
    let executor = WorkflowExecutor::new(workflow.clone());
    let output = run(
        &executor,
        WorkflowInput,
        config,
        Value::UNDEFINED,
        Some(&workflow),
        dispatcher,
    )
    .await?;
    Ok(WorkflowResult { output })
}
