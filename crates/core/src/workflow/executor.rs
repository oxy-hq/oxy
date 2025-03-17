use std::collections::HashMap;
use std::fs;

use minijinja::value::Kwargs;
use minijinja::{Value, context};

use crate::config::model::ExecuteSQLTask;
use crate::config::model::FileFormat;
use crate::config::model::FormatterTask;
use crate::config::model::LoopSequentialTask;
use crate::config::model::LoopValues;
use crate::config::model::SQL;
use crate::config::model::Task;
use crate::config::model::TaskType;
use crate::config::model::Workflow;
use crate::config::model::{AgentTask, WorkflowTask};
use crate::connector::Connector;
use crate::errors::OxyError;
use crate::execute::agent::AgentInput;
use crate::execute::agent::build_agent;
use crate::execute::consistency::ConsistencyExecutor;
use crate::execute::core::ExecutionContext;
use crate::execute::core::arrow_table::ArrowTable;
use crate::execute::core::cache::Cacheable;
use crate::execute::core::event::Dispatcher;
use crate::execute::core::value::ContextValue;
use crate::execute::core::write::Write;
use crate::execute::core::{Executable, run};
use crate::execute::workflow::WorkflowEvent;
use crate::execute::workflow::WorkflowInput;
use crate::execute::workflow::{LoopInput, PassthroughHandler};

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
    ) -> Result<(), OxyError> {
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
    ) -> Result<(), OxyError> {
        let config = execution_context.config.clone();
        let prompt = execution_context.renderer.render(&self.prompt)?;
        let export_file_path = match &self.export {
            Some(export) => match execution_context.renderer.render(&export.path) {
                Ok(path) => Some(execution_context.config.resolve_file(path).await?),
                Err(e) => {
                    return Err(e);
                }
            },
            None => None,
        };
        let mut agent_executor = execution_context.child_executor();
        let (agent, agent_config, global_context) = build_agent(
            &self.agent_ref,
            &FileFormat::Json,
            Some(prompt.clone()),
            config,
        )
        .await?;
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
    ) -> Result<(), OxyError> {
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
                    .resolve_file(&rendered_sql_file)
                    .await?;
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
                        return Err(OxyError::RuntimeError(format!(
                            "Error reading query file {}: {}",
                            &query_file, e
                        )));
                    }
                }
            }
        };

        let (datasets, schema) =
            Connector::from_database(&self.database, execution_context.config.as_ref())
                .await?
                .run_query_and_load(&query)
                .await?;
        let mut export_file_path = String::new();
        if let Some(export) = &self.export {
            let relative_export_file_path = execution_context.renderer.render(&export.path)?;
            export_file_path = execution_context
                .config
                .resolve_file(relative_export_file_path)
                .await?;
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
    ) -> Result<(), OxyError> {
        let task_output = execution_context.renderer.render(&self.template)?;

        let mut export_file_path = String::new();
        if let Some(export) = &self.export {
            let relative_export_file_path = execution_context.renderer.render(&export.path)?;
            export_file_path = execution_context
                .config
                .resolve_file(relative_export_file_path)
                .await?;
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
    ) -> Result<(), OxyError> {
        let workflow = execution_context.config.resolve_workflow(&self.src).await?;
        let dispatcher = Dispatcher::new(vec![Box::new(PassthroughHandler::new(
            execution_context.get_sender(),
        ))]);
        let executor = WorkflowExecutor::new(workflow.clone());
        let default_variables = workflow.variables.clone();

        // render variables before passing them to the sub workflow
        let mut variables = HashMap::new();
        if let Some(vars) = &self.variables {
            let ctx = execution_context.renderer.get_context();
            let mut renderer = execution_context.renderer.clone();
            for (key, value) in vars {
                let rendered_value = renderer.render_once(value, ctx.clone())?;
                let rendered_key = renderer.render_once(key, ctx.clone())?;
                variables.insert(rendered_key, rendered_value);
            }
        }

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
    ) -> Result<(), OxyError> {
        let LoopInput { name } = input;
        let values = match &self.values {
            LoopValues::Template(template) => {
                execution_context.renderer.eval_enumerate(template)?
            }
            LoopValues::Array(values) => values.clone(),
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
                None,
            )
            .await;
        loop_executor.finish()?;
        res
    }
}

#[async_trait::async_trait]
impl Cacheable<(), WorkflowEvent> for Task {
    async fn cache_key(
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
                    let file_key = execution_context
                        .config
                        .resolve_file(&cache_key)
                        .await
                        .ok()?;
                    Some(file_key)
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

    fn write_event_failed(&self, key: &str, err: OxyError) -> Option<WorkflowEvent> {
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
    ) -> Result<(), OxyError> {
        match &self.task_type {
            TaskType::Agent(agent) => {
                if agent.consistency_run > 1 {
                    let mut consistency_executor = ConsistencyExecutor::new();
                    consistency_executor
                        .execute(
                            execution_context,
                            agent,
                            agent.prompt.clone(),
                            agent.consistency_run,
                        )
                        .await?;
                } else {
                    agent.execute(execution_context, WorkflowInput).await?;
                }
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
    ) -> Result<(), OxyError> {
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
    ) -> Result<(), OxyError> {
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
