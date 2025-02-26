use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use minijinja::value::Kwargs;
use minijinja::{context, Value};

use crate::config::model::ExecuteSQLTask;
use crate::config::model::FileFormat;
use crate::config::model::FormatterTask;
use crate::config::model::LoopSequentialTask;
use crate::config::model::LoopValues;
use crate::config::model::Task;
use crate::config::model::TaskType;
use crate::config::model::Workflow;
use crate::config::model::SQL;
use crate::config::model::{AgentTask, WorkflowTask};
use crate::connector::Connector;
use crate::errors::OnyxError;
use crate::execute::agent::build_agent;
use crate::execute::agent::AgentInput;
use crate::execute::core::arrow_table::ArrowTable;
use crate::execute::core::cache::Cacheable;
use crate::execute::core::event::Dispatcher;
use crate::execute::core::value::ContextValue;
use crate::execute::core::write::Write;
use crate::execute::core::ExecutionContext;
use crate::execute::core::{run, Executable};
use crate::execute::workflow::WorkflowEvent;
use crate::execute::workflow::WorkflowInput;
use crate::execute::workflow::{LoopInput, WorkflowExporter, WorkflowReceiver};

use super::cache::AgentCache;
use super::cache::FileCache;

pub struct WorkflowExecutor {
    workflow: Workflow,
}

impl WorkflowExecutor {
    pub fn new(workflow: Workflow) -> Self {
        Self { workflow }
    }
}

#[async_trait::async_trait]
impl Executable<WorkflowInput, WorkflowEvent> for WorkflowExecutor {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
        _input: WorkflowInput,
    ) -> Result<(), OnyxError> {
        execution_context
            .notify(WorkflowEvent::Started {
                name: self.workflow.name.clone(),
            })
            .await?;
        self.workflow
            .tasks
            .execute(execution_context, ContextValue::None)
            .await?;
        execution_context.notify(WorkflowEvent::Finished).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<WorkflowInput, WorkflowEvent> for AgentTask {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
        _input: WorkflowInput,
    ) -> Result<(), OnyxError> {
        let config = execution_context.config.clone();
        let agent_file = config.project_path.join(&self.agent_ref);
        let prompt = execution_context.renderer.render(&self.prompt)?;
        let export_file_path = match &self.export {
            Some(export) => match execution_context.renderer.render(&export.path) {
                Ok(path) => Some(execution_context.config.project_path.join(path)),
                Err(e) => {
                    return Err(e);
                }
            },
            None => None,
        };
        let mut agent_executor = execution_context.child_executor();
        let (agent, agent_config, global_context) = build_agent(
            Some(&agent_file),
            &FileFormat::Json,
            Some(prompt.clone()),
            &config,
        )?;
        let task = self.clone();
        let map_agent_event = move |event| WorkflowEvent::Agent {
            orig: event,
            task: task.clone(),
            export_file_path: export_file_path.clone(),
        };
        agent_executor
            .execute(
                &agent,
                AgentInput {
                    prompt: Some(prompt),
                    system_instructions: agent_config.system_instructions.clone(),
                },
                map_agent_event,
                global_context,
                Default::default(),
                &agent_config,
            )
            .await?;
        agent_executor.finish();
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<WorkflowInput, WorkflowEvent> for ExecuteSQLTask {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
        _input: WorkflowInput,
    ) -> Result<(), OnyxError> {
        let wh = execution_context.config.find_database(&self.database)?;

        let mut variables = HashMap::new();
        if let Some(vars) = &self.variables {
            for (key, value) in vars {
                let rendered_value = execution_context.renderer.render(value)?;
                variables.insert(key.clone(), rendered_value);
            }
        }

        let query = match &self.sql {
            SQL::Query { sql_query } => {
                let query = execution_context.renderer.render(sql_query)?;
                if !variables.is_empty() {
                    execution_context
                        .renderer
                        .render_once(&query, Value::from_serialize(&variables))?
                } else {
                    query
                }
            }
            SQL::File { sql_file } => {
                let rendered_sql_file = execution_context.renderer.render(sql_file)?;
                let query_file = execution_context
                    .config
                    .project_path
                    .join(&rendered_sql_file);
                match fs::read_to_string(&query_file) {
                    Ok(query) => {
                        let context = if variables.is_empty() {
                            execution_context.renderer.get_context()
                        } else {
                            context! {
                                ..execution_context.renderer.get_context(),
                                ..Value::from_serialize(&variables)
                            }
                        };

                        execution_context.renderer.render_once(&query, context)?
                    }
                    Err(e) => {
                        return Err(OnyxError::RuntimeError(format!(
                            "Error reading query file {}: {}",
                            &query_file.display(),
                            e
                        )));
                    }
                }
            }
        };

        let (datasets, schema) = Connector::new(&wh, &execution_context.config)
            .run_query_and_load(&query)
            .await?;
        let mut export_file_path = PathBuf::new();
        if let Some(export) = &self.export {
            let relative_export_file_path = execution_context.renderer.render(&export.path)?;
            export_file_path = execution_context
                .config
                .project_path
                .join(relative_export_file_path);
        }

        execution_context
            .notify(WorkflowEvent::ExecuteSQL {
                task: self.clone(),
                query,
                datasets: datasets.clone(),
                schema,
                export_file_path,
            })
            .await?;
        execution_context.write(ContextValue::Table(ArrowTable::new(datasets)));
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<WorkflowInput, WorkflowEvent> for FormatterTask {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
        _input: WorkflowInput,
    ) -> Result<(), OnyxError> {
        let task_output = execution_context.renderer.render(&self.template)?;

        let mut export_file_path = PathBuf::new();
        if let Some(export) = &self.export {
            let relative_export_file_path = execution_context.renderer.render(&export.path)?;
            export_file_path = execution_context
                .config
                .project_path
                .join(relative_export_file_path);
        }

        execution_context
            .notify(WorkflowEvent::Formatter {
                task: self.clone(),
                output: task_output.clone(),
                export_file_path,
            })
            .await?;
        execution_context.write(ContextValue::Text(task_output));
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<WorkflowInput, WorkflowEvent> for WorkflowTask {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
        _input: WorkflowInput,
    ) -> Result<(), OnyxError> {
        let workflow = execution_context.config.load_workflow(&self.src)?;
        let dispatcher =
            Dispatcher::new(vec![Box::new(WorkflowReceiver), Box::new(WorkflowExporter)]);
        let executor = WorkflowExecutor::new(workflow.clone());
        let default_variables = workflow.variables.clone();
        let variables = if let Some(vars) = &self.variables {
            vars.clone()
        } else {
            HashMap::new()
        };
        let ctx = context! {
            ..Value::from_serialize(&variables),
            ..Value::from_serialize(&default_variables),
        };

        let output = run(
            &executor,
            WorkflowInput,
            execution_context.config.clone(),
            ctx,
            Some(&workflow),
            dispatcher,
        )
        .await?;

        execution_context
            .notify(WorkflowEvent::SubWorkflow { step: self.clone() })
            .await?;

        execution_context.write(output);
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<LoopInput, WorkflowEvent> for LoopSequentialTask {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
        input: LoopInput,
    ) -> Result<(), OnyxError> {
        let LoopInput { name } = input;
        let values = match &self.values {
            LoopValues::Template(ref template) => {
                execution_context.renderer.eval_enumerate(template)?
            }
            LoopValues::Array(ref values) => values.clone(),
        };

        let mut loop_executor = execution_context.loop_executor();
        let mut values: Vec<ContextValue> = values
            .iter()
            .map(|v| ContextValue::Text(v.to_string()))
            .collect();
        let res = loop_executor
            .params(
                &mut values,
                &self.tasks,
                |param| {
                    Value::from(Kwargs::from_iter([(
                        name.clone(),
                        Value::from(Kwargs::from_iter([(
                            "value",
                            Value::from_object(param.clone()),
                        )])),
                    )]))
                },
                self.concurrency,
                Some(|| {}),
            )
            .await;
        loop_executor.finish()?;
        res
    }
}

impl Cacheable<(), WorkflowEvent> for Task {
    fn cache_key(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
        _input: &(),
    ) -> Option<String> {
        match &self.cache {
            Some(cache) => {
                if !cache.enabled {
                    return None;
                }

                if let Ok(cache_key) = execution_context.renderer.render(&cache.path) {
                    let file_key = execution_context.config.project_path.join(&cache_key);
                    Some(file_key.to_string_lossy().to_string())
                } else {
                    None
                }
            }
            None => None,
        }
    }

    fn hit_event(&self, key: &str) -> Option<WorkflowEvent> {
        Some(WorkflowEvent::CacheHit {
            path: key.to_string(),
        })
    }

    fn write_event(&self, key: &str) -> Option<WorkflowEvent> {
        Some(WorkflowEvent::CacheWrite {
            path: key.to_string(),
        })
    }

    fn write_event_failed(&self, key: &str, err: OnyxError) -> Option<WorkflowEvent> {
        Some(WorkflowEvent::CacheWriteFailed {
            path: key.to_string(),
            err,
        })
    }
}

#[async_trait::async_trait]
impl Executable<(), WorkflowEvent> for Task {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
        _input: (),
    ) -> Result<(), OnyxError> {
        match &self.task_type {
            TaskType::Agent(agent) => {
                agent.execute(execution_context, WorkflowInput).await?;
            }
            TaskType::ExecuteSQL(execute_sql) => {
                execute_sql
                    .execute(execution_context, WorkflowInput)
                    .await?;
            }
            TaskType::Formatter(formatter) => {
                formatter.execute(execution_context, WorkflowInput).await?;
            }
            TaskType::Workflow(workflow) => {
                workflow.execute(execution_context, WorkflowInput).await?;
            }
            TaskType::LoopSequential(loop_sequential) => {
                loop_sequential
                    .execute(
                        execution_context,
                        LoopInput {
                            name: self.name.clone(),
                        },
                    )
                    .await?;
            }
            TaskType::Unknown => {
                execution_context
                    .notify(WorkflowEvent::TaskUnknown {
                        name: self.name.clone(),
                    })
                    .await?;
            }
        }
        Ok(())
    }
}

struct TaskExecutor;

#[async_trait::async_trait]
impl Executable<Task, WorkflowEvent> for TaskExecutor {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
        input: Task,
    ) -> Result<(), OnyxError> {
        execution_context
            .notify(WorkflowEvent::TaskStarted {
                name: input.name.clone(),
            })
            .await?;
        let mut cache_executor = execution_context.cache_executor();
        let res = match &input.task_type {
            TaskType::Agent(_agent) => {
                cache_executor
                    .execute(&input, &input, (), &AgentCache::new(input.name.to_string()))
                    .await
            }
            _ => cache_executor.execute(&input, &input, (), &FileCache).await,
        };
        cache_executor.finish();
        res
    }
}

#[async_trait::async_trait]
impl Executable<ContextValue, WorkflowEvent> for Vec<Task> {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
        input: ContextValue,
    ) -> Result<(), OnyxError> {
        let mut map_executor = execution_context.map_executor();
        map_executor.prefill("value", input);
        let res = map_executor
            .entries(
                self.iter()
                    .map(|s| (s.name.clone(), TaskExecutor, s.clone()))
                    .collect::<Vec<(_, _, _)>>(),
            )
            .await;
        map_executor.finish();
        res
    }
}
