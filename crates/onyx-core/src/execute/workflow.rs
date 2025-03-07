use std::{path::PathBuf, sync::Arc, time::Duration};

use arrow::{array::RecordBatch, datatypes::Schema};
use minijinja::Value;

use crate::{
    ai::utils::record_batches_to_table,
    config::{
        model::{
            AgentTask, ExecuteSQLTask, FormatterTask, LoopValues, Task, TaskType, Workflow,
            WorkflowTask, SQL,
        },
        ConfigBuilder,
    },
    errors::OnyxError,
    execute::exporter::{export_agent_task, export_execute_sql, export_formatter},
    utils::{find_project_path, print_colored_sql},
    workflow::{executor::WorkflowExecutor, WorkflowResult},
    StyledText,
};

use super::{
    agent::{AgentEvent, AgentReceiver},
    consensus::{ConsensusEvent, ConsensusReceiver},
    core::{
        event::{Dispatcher, Handler},
        run,
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

    // task
    TaskStarted {
        name: String,
    },
    TaskUnknown {
        name: String,
    },
    Retry {
        err: OnyxError,
        after: Duration,
    },
    CacheHit {
        path: String,
    },
    CacheWrite {
        path: String,
    },
    CacheWriteFailed {
        path: String,
        err: OnyxError,
    },

    // export
    Export {
        export_file_path: PathBuf,
        task: TaskType,
    },

    // agent
    Agent {
        orig: AgentEvent,
        task: AgentTask,
        export_file_path: Option<String>,
    },

    // consensus
    Consensus {
        orig: ConsensusEvent,
    },

    // sql
    ExecuteSQL {
        task: ExecuteSQLTask,
        query: String,
        datasets: Vec<RecordBatch>,
        schema: Arc<Schema>,
        export_file_path: String,
    },

    // formatter
    Formatter {
        task: FormatterTask,
        output: String,
        export_file_path: String,
    },
    SubWorkflow {
        step: WorkflowTask,
    },
}

impl TemplateRegister for Workflow {
    fn register_template(&self, renderer: &mut Renderer) -> Result<(), OnyxError> {
        renderer.register(&self.tasks)
    }
}

impl TemplateRegister for &Task {
    fn register_template(&self, renderer: &mut Renderer) -> Result<(), OnyxError> {
        let mut register = renderer.child_register();

        if let Some(cache) = &self.cache {
            register.entry(&cache.path.as_str())?;
        }

        match &self.task_type {
            TaskType::Agent(agent) => {
                register.entry(&agent.prompt.as_str())?;
                if let Some(export) = &agent.export {
                    register.entry(&export.path.as_str())?;
                }
            }
            TaskType::ExecuteSQL(execute_sql) => {
                let sql = match &execute_sql.sql {
                    SQL::Query { sql_query } => sql_query,
                    SQL::File { sql_file } => sql_file,
                };
                register.entry(&sql.as_str())?;
                if let Some(variables) = &execute_sql.variables {
                    register.entries(
                        variables
                            .iter()
                            .map(|(_key, value)| value.as_str())
                            .collect::<Vec<&str>>(),
                    )?;
                }
                if let Some(export) = &execute_sql.export {
                    register.entry(&export.path.as_str())?;
                }
            }
            TaskType::Formatter(formatter) => {
                register.entry(&formatter.template.as_str())?;
                if let Some(export) = &formatter.export {
                    register.entry(&export.path.as_str())?;
                }
            }
            TaskType::LoopSequential(loop_sequential) => {
                if let LoopValues::Template(template) = &loop_sequential.values {
                    register.entry(&template.as_str())?;
                }
                register.entry(&loop_sequential.tasks)?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl TemplateRegister for Vec<Task> {
    fn register_template(&self, renderer: &mut Renderer) -> Result<(), OnyxError> {
        let mut child_register = renderer.child_register();
        child_register.entries(self)?;
        Ok(())
    }
}

pub struct WorkflowReceiver {
    consensus_receiver: ConsensusReceiver,
}

impl WorkflowReceiver {
    pub fn new() -> Self {
        Self {
            consensus_receiver: ConsensusReceiver::new(),
        }
    }
}

impl Handler for WorkflowReceiver {
    type Event = WorkflowEvent;

    fn handle(&self, event: &Self::Event) {
        match event {
            WorkflowEvent::Started { name } => {
                println!("\n⏳Running workflow: {}", name.text());
            }
            WorkflowEvent::ExecuteSQL {
                task: _,
                query,
                datasets,
                schema,
                export_file_path: _,
            } => {
                print_colored_sql(query);

                let batches_display = match record_batches_to_table(datasets, schema) {
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
                task: _,
                output,
                export_file_path: _,
            } => {
                println!("{}", "\nOutput:".primary());
                println!("{}", output);
            }
            WorkflowEvent::TaskStarted { name } => {
                println!("\n⏳Starting {}", name.text());
            }
            WorkflowEvent::CacheHit { .. } => {
                println!("{}", "Cache detected. Using cache.".primary());
            }
            WorkflowEvent::CacheWrite { path } => {
                println!("{}", format!("Cache written to {}", path).primary());
            }
            WorkflowEvent::CacheWriteFailed { path, err } => {
                println!(
                    "{}",
                    format!("Failed to write cache to {}: {}", path, err).error()
                );
            }
            WorkflowEvent::TaskUnknown { name } => {
                println!(
                    "{}",
                    format!("Encountered unknown task {name}. Skipping.").warning()
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
            WorkflowEvent::Consensus { orig, .. } => {
                self.consensus_receiver.handle(orig);
            }
            WorkflowEvent::SubWorkflow { step } => {
                println!("\n⏳Subworkflow executed successfully");
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
                task,
                export_file_path,
            } => {
                log::debug!("Agent tool calls: {:?}", event);
                if let AgentEvent::ToolCall(tool_call) = event {
                    if let Some(export_file_path) = export_file_path {
                        export_agent_task(task, &[tool_call], export_file_path);
                    }
                }
            }
            WorkflowEvent::ExecuteSQL {
                task,
                query,
                datasets,
                schema,
                export_file_path,
            } => {
                if let Some(export) = &task.export {
                    export_execute_sql(export, "", query, schema, datasets, export_file_path);
                }
                log::debug!("ExecuteSQL tool calls: {:?}", event);
            }
            WorkflowEvent::Formatter {
                task,
                output,
                export_file_path,
            } => {
                if task.export.is_some() {
                    export_formatter(output, export_file_path);
                }
                log::debug!("Formatter tool calls: {:?}", event);
            }
            WorkflowEvent::SubWorkflow { step } => {
                log::debug!("SubWorkflow tool calls: {:?}", step);
            }
            _ => {
                log::debug!("Unhandled event: {:?}", event);
            }
        }
    }
}

pub async fn run_workflow(workflow_path: &PathBuf) -> Result<WorkflowResult, OnyxError> {
    let config = ConfigBuilder::new()
        .with_project_path(find_project_path()?)?
        .build()
        .await?;
    let workflow = config.resolve_workflow(workflow_path).await?;
    let dispatcher = Dispatcher::new(vec![
        Box::new(WorkflowReceiver::new()),
        Box::new(WorkflowExporter),
    ]);
    let executor = WorkflowExecutor::new(workflow.clone());
    let ctx = Value::from_serialize(&workflow.variables);
    let output = run(
        &executor,
        WorkflowInput,
        Arc::new(config),
        ctx,
        Some(&workflow),
        dispatcher,
    )
    .await?;
    Ok(WorkflowResult { output })
}
