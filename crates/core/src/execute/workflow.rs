use std::{fs::File, io::Write, path::PathBuf, sync::Arc, sync::Mutex, time::Duration};

use arrow::{array::RecordBatch, datatypes::Schema};
use chrono::{DateTime, Utc};
use minijinja::Value;
use serde::Deserialize;
use serde::Serialize;

use crate::{
    ai::utils::{record_batches_to_markdown, record_batches_to_table},
    config::{
        model::{
            AgentTask, ExecuteSQLTask, FormatterTask, LoopValues, Task, TaskType, Workflow,
            WorkflowTask, SQL,
        },
        ConfigBuilder,
    },
    connector::load_result,
    errors::OxyError,
    execute::exporter::{export_agent_task, export_execute_sql, export_formatter},
    utils::{find_project_path, print_colored_sql, truncate_datasets, MAX_DISPLAY_ROWS},
    workflow::{executor::WorkflowExecutor, WorkflowResult},
    StyledText,
};

use super::{
    agent::{AgentEvent, AgentReceiver, ToolCall, ToolMetadata},
    consistency::{ConsistencyEvent, ConsistencyReceiver},
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
        err: OxyError,
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
        err: OxyError,
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

    // consistency
    Consistency {
        orig: ConsistencyEvent,
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
    fn register_template(&self, renderer: &mut Renderer) -> Result<(), OxyError> {
        renderer.register(&self.tasks)
    }
}

impl TemplateRegister for &Task {
    fn register_template(&self, renderer: &mut Renderer) -> Result<(), OxyError> {
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
    fn register_template(&self, renderer: &mut Renderer) -> Result<(), OxyError> {
        let mut child_register = renderer.child_register();
        child_register.entries(self)?;
        Ok(())
    }
}

pub struct WorkflowReceiver {
    consistency_receiver: ConsistencyReceiver,
    pub logger: Box<dyn WorkflowLogger>,
}

impl WorkflowReceiver {
    pub fn new(logger: Box<dyn WorkflowLogger>) -> Self {
        Self {
            consistency_receiver: ConsistencyReceiver::new(),
            logger,
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct WorkflowCLILogger;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogType {
    #[serde(rename = "success")]
    Success,
    #[serde(rename = "info")]
    Info,
    #[serde(rename = "warning")]
    Warning,
    #[serde(rename = "error")]
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogItem {
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub log_type: LogType,
}

impl LogItem {
    pub fn new(content: String, log_type: LogType) -> Self {
        Self {
            content,
            timestamp: Utc::now(),
            log_type,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkflowAPILogger {
    sender: tokio::sync::mpsc::Sender<LogItem>,
    writer: Option<Arc<Mutex<File>>>,
}

impl WorkflowAPILogger {
    pub fn new(
        sender: tokio::sync::mpsc::Sender<LogItem>,
        writer: Option<Arc<Mutex<File>>>,
    ) -> Self {
        Self { sender, writer }
    }

    pub fn log(&self, log_item: LogItem) {
        if let Some(writer) = &self.writer {
            let mut file = writer.lock().unwrap();
            let _ = writeln!(file, "{}", serde_json::to_string(&log_item).unwrap());
        }
        let _ = self.sender.try_send(log_item);
    }
}

pub trait WorkflowLogger: Send + Sync {
    fn log(&self, text: &str);
    fn log_execution_result(&self, query: &str, schema: &Arc<Schema>, datasets: &Vec<RecordBatch>);
    fn log_task_started(&self, name: &str);
    fn log_event(&self, event: WorkflowEvent);
    fn clone(&self) -> Box<dyn WorkflowLogger>;
    fn log_agent_finished(&self, output: &str);
    fn log_agent_tool_call(&self, tool_call: &ToolCall);
}

impl WorkflowLogger for WorkflowAPILogger {
    fn log(&self, text: &str) {
        let item = LogItem::new(text.to_string(), LogType::Info);
        self.log(item)
    }

    fn clone(&self) -> Box<dyn WorkflowLogger> {
        Box::new(WorkflowAPILogger {
            sender: self.sender.clone(),
            writer: self.writer.clone(),
        })
    }

    fn log_execution_result(&self, query: &str, schema: &Arc<Schema>, datasets: &Vec<RecordBatch>) {
        let item = LogItem::new(format!("Query: \n\n```sql\n{}\n```", query), LogType::Info);
        self.log(item);
        let batches_display = match record_batches_to_markdown(datasets, schema) {
            Ok(display) => display,
            Err(e) => {
                let err_log =
                    LogItem::new(format!("Error displaying results: {}", e), LogType::Error);
                self.log(err_log);
                return;
            }
        };

        let result_log = LogItem::new(format!("Results:\n\n{}", batches_display), LogType::Info);
        self.log(result_log)
    }

    fn log_task_started(&self, name: &str) {
        let item = LogItem::new(format!("Starting {}", name), LogType::Info);
        self.log(item)
    }

    fn log_event(&self, event: WorkflowEvent) {
        match event {
            WorkflowEvent::Started { name } => {
                let item = LogItem::new(format!("Running workflow: {}", name), LogType::Info);
                self.log(item);
            }
            WorkflowEvent::Finished => {
                let item = LogItem::new(
                    "Workflow executed successfully".to_string(),
                    LogType::Success,
                );
                self.log(item);
            }
            WorkflowEvent::TaskStarted { name } => {
                let item = LogItem::new(format!("Starting {}", name), LogType::Info);
                self.log(item)
            }
            WorkflowEvent::TaskUnknown { name } => {
                let item = LogItem::new(
                    format!("Encountered unknown task {name}. Skipping."),
                    LogType::Warning,
                );
                self.log(item)
            }
            WorkflowEvent::Retry { err, after } => {
                let item =
                    LogItem::new(format!("Retrying after {:?} ...", after), LogType::Warning);
                self.log(item)
            }
            WorkflowEvent::CacheHit { path } => {
                let item = LogItem::new("Cache detected. Using cache.".to_string(), LogType::Info);
                self.log(item)
            }
            WorkflowEvent::CacheWrite { path } => {
                let item = LogItem::new(format!("Cache written to {}", path), LogType::Info);
                self.log(item)
            }
            WorkflowEvent::CacheWriteFailed { path, err } => {
                let item = LogItem::new(
                    format!("Failed to write cache to {}: {}", path, err),
                    LogType::Error,
                );
                self.log(item)
            }
            WorkflowEvent::Export {
                export_file_path,
                task,
            } => {}
            WorkflowEvent::Agent {
                orig,
                task,
                export_file_path,
            } => {}
            WorkflowEvent::ExecuteSQL {
                task,
                query,
                datasets,
                schema,
                export_file_path,
            } => {
                self.log_execution_result(query.as_str(), &schema, &datasets);
            }
            WorkflowEvent::Formatter {
                task,
                output,
                export_file_path,
            } => {
                let item = LogItem::new(format!("Output:\n\n{}", output), LogType::Info);
                self.log(item)
            }
            WorkflowEvent::SubWorkflow { step } => {
                let item = LogItem::new(
                    "Subworkflow executed successfully".to_string(),
                    LogType::Success,
                );
                self.log(item)
            }
            WorkflowEvent::Consistency { .. } => {}
        }
    }

    fn log_agent_finished(&self, output: &str) {
        let item = LogItem::new(format!("Output:\n\n{}", output), LogType::Info);
        self.log(item);
    }

    fn log_agent_tool_call(&self, tool_call: &ToolCall) {
        match tool_call.metadata.clone() {
            Some(metadata) => match metadata {
                ToolMetadata::ExecuteSQL {
                    sql_query,
                    output_file,
                } => {
                    let sql_item = LogItem::new(
                        format!("SQL Query: \n\n```sql\n{}\n```", sql_query),
                        LogType::Info,
                    );
                    self.log(sql_item);
                    match load_result(&output_file) {
                        Ok((batches, schema)) => {
                            let (batches, truncated) = truncate_datasets(batches);
                            match record_batches_to_table(&batches, &schema) {
                                Ok(table) => {
                                    self.log(LogItem::new(
                                        format!("Result:\n\n{}", table),
                                        LogType::Info,
                                    ));
                                    if truncated {
                                        self.log(LogItem::new(format!(
                                            "Results have been truncated. Showing only the first {} rows.",
                                            MAX_DISPLAY_ROWS
                                        ), LogType::Warning));
                                    }
                                }
                                Err(e) => {
                                    self.log(LogItem::new(
                                        format!("Error in converting record batch to table: {}", e),
                                        LogType::Error,
                                    ));
                                }
                            }
                            self.log_execution_result(&sql_query, &schema, &batches);
                        }
                        Err(e) => {
                            self.log(LogItem::new(
                                format!("Error loading result: {}", e),
                                LogType::Error,
                            ));
                        }
                    }
                }
            },
            None => todo!(),
        }
    }
}

impl WorkflowLogger for WorkflowCLILogger {
    fn log(&self, text: &str) {
        println!("{}", text);
    }

    fn clone(&self) -> Box<dyn WorkflowLogger> {
        Box::new(WorkflowCLILogger)
    }

    fn log_execution_result(&self, query: &str, schema: &Arc<Schema>, datasets: &Vec<RecordBatch>) {
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

    fn log_task_started(&self, name: &str) {
        println!("\n⏳Starting {}", name.text());
    }

    fn log_event(&self, event: WorkflowEvent) {
        match event {
            WorkflowEvent::Started { name } => {
                println!("\n\n⏳Running workflow: {}", name.text());
            }
            WorkflowEvent::TaskStarted { name } => {
                println!("\n\n⏳Starting {}", name.text());
            }
            WorkflowEvent::ExecuteSQL {
                task,
                query,
                datasets,
                schema,
                export_file_path,
            } => {
                print_colored_sql(query.as_str());

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
            WorkflowEvent::Finished => {
                println!("{}", "\n✅Workflow executed successfully".success());
            }
            WorkflowEvent::TaskUnknown { name } => {
                println!(
                    "{}",
                    format!("Encountered unknown task {name}. Skipping.").warning()
                );
            }
            WorkflowEvent::Retry { err, after } => {
                println!("{}", format!("\nRetrying after {:?} ...", after).warning());
                println!("Reason {:?}", err);
            }
            WorkflowEvent::CacheHit { path } => {
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
            WorkflowEvent::Export {
                export_file_path,
                task,
            } => {}
            WorkflowEvent::Agent {
                orig,
                task,
                export_file_path,
            } => {}
            WorkflowEvent::Formatter {
                task,
                output,
                export_file_path,
            } => {
                println!("{}", "\nOutput:".primary());
                println!("{}", output);
            }
            WorkflowEvent::SubWorkflow { step } => {
                println!("\n⏳Subworkflow executed successfully");
            }
            WorkflowEvent::Consistency { .. } => {}
        }
    }

    fn log_agent_finished(&self, output: &str) {
        println!("{}", "\nOutput:".primary());
        println!("{}", output);
    }

    fn log_agent_tool_call(&self, tool_call: &ToolCall) {
        match &tool_call.metadata {
            Some(ToolMetadata::ExecuteSQL {
                sql_query,
                output_file,
            }) => {
                print_colored_sql(sql_query);
                match load_result(output_file) {
                    Ok((batches, schema)) => {
                        let (batches, truncated) = truncate_datasets(batches);
                        match record_batches_to_table(&batches, &schema) {
                            Ok(table) => {
                                println!("{}", "\nResult:".primary());
                                println!("{}", table);
                                if truncated {
                                    println!("{}", format!(
                                                        "Results have been truncated. Showing only the first {} rows.",
                                                        MAX_DISPLAY_ROWS
                                                    ).warning());
                                }
                            }
                            Err(e) => {
                                eprintln!(
                                    "{}",
                                    format!("Error in converting record batch to table: {}", e)
                                        .error()
                                );
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("{}", format!("Error loading result: {}", e).error());
                    }
                }
            }
            None => {
                log::debug!("Unhandled tool call: {:?}", &tool_call);
            }
        }
    }
}

impl Handler for WorkflowReceiver {
    type Event = WorkflowEvent;

    fn handle(&self, event: &Self::Event) {
        self.logger.log_event(event.clone());
        match event {
            WorkflowEvent::Started { name } => {}
            WorkflowEvent::ExecuteSQL {
                task: _,
                query,
                datasets,
                schema,
                export_file_path: _,
            } => {}
            WorkflowEvent::Formatter {
                task: _,
                output,
                export_file_path: _,
            } => {}
            WorkflowEvent::TaskStarted { name } => {}
            WorkflowEvent::CacheHit { .. } => {}
            WorkflowEvent::CacheWrite { path } => {}
            WorkflowEvent::CacheWriteFailed { path, err } => {}
            WorkflowEvent::TaskUnknown { name } => {}
            WorkflowEvent::Finished => {}
            WorkflowEvent::Retry { err, after } => {}
            WorkflowEvent::Agent { orig, .. } => {
                AgentReceiver {
                    logger: self.logger.clone(),
                }
                .handle(orig);
            }
            WorkflowEvent::SubWorkflow { step } => {}
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

pub struct PassthroughHandler {
    sender: tokio::sync::mpsc::Sender<WorkflowEvent>,
}

impl PassthroughHandler {
    pub fn new(sender: tokio::sync::mpsc::Sender<WorkflowEvent>) -> Self {
        Self { sender }
    }
}

impl Handler for PassthroughHandler {
    type Event = WorkflowEvent;

    fn handle(&self, event: &Self::Event) {
        let _ = self.sender.try_send(event.clone());
    }
}

pub async fn run_workflow(workflow_path: &PathBuf) -> Result<WorkflowResult, OxyError> {
    let config = ConfigBuilder::new()
        .with_project_path(find_project_path()?)?
        .build()
        .await?;
    let workflow = config.resolve_workflow(workflow_path).await?;

    let dispatcher = Dispatcher::new(vec![
        Box::new(WorkflowReceiver::new(Box::new(WorkflowCLILogger))),
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
