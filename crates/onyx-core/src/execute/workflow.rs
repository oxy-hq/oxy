use std::{path::PathBuf, sync::Arc, time::Duration};

use arrow::{array::RecordBatch, datatypes::Schema};
use minijinja::Value;

use crate::{
    ai::{agent::AgentResult, utils::record_batches_to_table},
    config::{
        load_config,
        model::{AgentStep, ExecuteSQLStep, FormatterStep, LoopValues, Step, StepType, Workflow},
    },
    errors::OnyxError,
    execute::exporter::{export_agent_step, export_execute_sql, export_formatter},
    utils::print_colored_sql,
    workflow::{cache::write_agent_cache, executor::WorkflowExecutor, WorkflowResult},
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
        export_file_path: PathBuf,
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
        result: AgentResult,
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
        let mut register = renderer.struct_register();
        match &self.step_type {
            StepType::Agent(agent) => {
                register.field(&agent.prompt.as_str())?;
                if let Some(export) = &agent.export {
                    register.field(&export.path.as_str())?;
                }

                if let Some(cache) = &agent.cache {
                    register.field(&cache.path.as_str())?;
                }
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
                if let Some(export) = &execute_sql.export {
                    register.field(&export.path.as_str())?;
                }
            }
            StepType::Formatter(formatter) => {
                register.field(&formatter.template.as_str())?;
                if let Some(export) = &formatter.export {
                    register.field(&export.path.as_str())?;
                }
            }
            StepType::LoopSequential(loop_sequential) => {
                if let LoopValues::Template(template) = &loop_sequential.values {
                    register.field(&template.as_str())?;
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
            WorkflowEvent::AgentToolCalls {
                calls,
                step,
                export_file_path,
            } => {
                export_agent_step(step, calls, export_file_path);
                log::debug!("Agent tool calls: {:?}", event);
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

    let mut renderer = Renderer::new();
    renderer.register(&workflow)?;
    let dispatcher = Dispatcher::new(vec![
        Box::new(WorkflowReceiver),
        Box::new(WorkflowExporter),
        Box::new(WorkflowCacheStep),
    ]);
    let mut output_collector = OutputCollector::new(&dispatcher);
    let mut execution_context = ExecutionContext::new(
        Value::UNDEFINED,
        &mut renderer,
        &Value::UNDEFINED,
        &mut output_collector,
        config.clone(),
    );
    let executor = WorkflowExecutor::new(workflow);
    executor.execute(&mut execution_context).await?;
    let output = output_collector.output.unwrap_or_default();
    Ok(WorkflowResult { output })
}
