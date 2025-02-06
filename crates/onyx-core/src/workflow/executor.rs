use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use backon::ExponentialBuilder;
use backon::Retryable;
use minijinja::value::Enumerator;
use minijinja::Value;

use crate::config::load_config;
use crate::config::model::AgentStep;
use crate::config::model::ExecuteSQLStep;
use crate::config::model::FileFormat;
use crate::config::model::FormatterStep;
use crate::config::model::LoopSequentialStep;
use crate::config::model::LoopValues;
use crate::config::model::Step;
use crate::config::model::StepType;
use crate::config::model::Workflow;
use crate::connector::Connector;
use crate::errors::OnyxError;
use crate::execute::agent::run_agent;
use crate::execute::agent::AgentEvent;
use crate::execute::core::arrow_table::ArrowTable;
use crate::execute::core::event::Handler;
use crate::execute::core::value::ContextValue;
use crate::execute::core::write::Write;
use crate::execute::core::Executable;
use crate::execute::core::ExecutionContext;
use crate::execute::workflow::WorkflowEvent;
use crate::workflow::cache::get_agent_cache;
use crate::StyledText;

pub struct WorkflowExecutor {
    workflow: Workflow,
}

impl WorkflowExecutor {
    pub fn new(workflow: Workflow) -> Self {
        Self { workflow }
    }
}

#[derive(Debug, Default)]
pub struct EventsCollector {
    events: Mutex<Vec<AgentEvent>>,
}

impl Handler for EventsCollector {
    type Event = AgentEvent;
    fn handle(&self, event: &Self::Event) {
        self.events.lock().unwrap().push(event.clone());
    }
}

#[async_trait::async_trait]
impl Executable<WorkflowEvent> for WorkflowExecutor {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
    ) -> Result<(), OnyxError> {
        execution_context.notify(WorkflowEvent::Started {
            name: self.workflow.name.clone(),
        });
        self.workflow.steps.execute(execution_context).await?;
        execution_context.notify(WorkflowEvent::Finished);
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<WorkflowEvent> for AgentStep {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
    ) -> Result<(), OnyxError> {
        let agent_file = execution_context.config.project_path.join(&self.agent_ref);
        let context = execution_context.get_context();
        let prompt = execution_context
            .renderer
            .render_async(&self.prompt, Value::from_serialize(&context))
            .await?;
        let collector = EventsCollector::default();

        let step_output = (|| async {
            run_agent(
                Some(&agent_file),
                &FileFormat::Json,
                &prompt,
                Some(&collector),
            )
            .await
        })
        .retry(ExponentialBuilder::default().with_max_times(self.retry))
        // Notify when retrying
        .notify(|err: &OnyxError, duration: std::time::Duration| {
            execution_context.notify(WorkflowEvent::Retry {
                err: err.clone(),
                after: duration,
            });
        })
        .await?;
        let mut tool_calls = vec![];
        for event in collector.events.lock().unwrap().iter_mut() {
            match event {
                AgentEvent::ToolCall(tool) => {
                    tool_calls.push(std::mem::take(tool));
                }
                _ => {}
            }
        }

        let mut export_file_path = PathBuf::new();
        if let Some(export) = &self.export {
            let export_file_path_str = execution_context
                .renderer
                .render_async(&export.path, Value::from_serialize(&context))
                .await?;
            export_file_path = execution_context
                .config
                .project_path
                .join(export_file_path_str);
        }

        execution_context.notify(WorkflowEvent::AgentToolCalls {
            calls: tool_calls,
            step: self.clone(),
            export_file_path,
        });

        let Some(cache) = &self.cache else {
            execution_context.write(step_output.output);
            return Ok(());
        };

        if cache.enabled {
            let cache_file_path_str = execution_context
                .renderer
                .render_async(&cache.path, Value::from_serialize(&context))
                .await?;

            let cache_file_path = execution_context
                .config
                .project_path
                .join(cache_file_path_str);

            execution_context.notify(WorkflowEvent::CacheAgentResult {
                result: step_output.clone(),
                file_path: cache_file_path,
            });
        }
        execution_context.write(step_output.output);
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<WorkflowEvent> for ExecuteSQLStep {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
    ) -> Result<(), OnyxError> {
        let context = execution_context.get_context();
        let config = load_config(None)?;
        let wh = &config.find_warehouse(&self.warehouse)?;
        log::info!("SQL Context: {:?}", context);

        let mut variables = HashMap::new();
        if let Some(vars) = &self.variables {
            for (key, value) in vars {
                let rendered_value = execution_context
                    .renderer
                    .render_async(value, Value::from_serialize(&context))
                    .await?;
                variables.insert(key.clone(), rendered_value);
            }
        }

        let rendered_sql_file = execution_context
            .renderer
            .render_async(&self.sql_file, Value::from_serialize(&context))
            .await?;
        let query_file = config.project_path.join(&rendered_sql_file);
        let query = match fs::read_to_string(&query_file) {
            Ok(query) => {
                if !variables.is_empty() {
                    execution_context
                        .renderer
                        .render_temp_async(&query, Value::from_serialize(&variables))
                        .await?
                } else {
                    query
                }
            }
            Err(e) => {
                return Err(OnyxError::RuntimeError(format!(
                    "Error reading query file {}: {}",
                    &query_file.display(),
                    e
                )));
            }
        };

        let (datasets, schema) = Connector::new(wh, &config)
            .run_query_and_load(&query)
            .await?;
        let mut export_file_path = PathBuf::new();
        if let Some(export) = &self.export {
            let export_file_path_str = execution_context
                .renderer
                .render_async(&export.path, Value::from_serialize(&context))
                .await?;
            export_file_path = execution_context
                .config
                .project_path
                .join(export_file_path_str);
        }

        execution_context.notify(WorkflowEvent::ExecuteSQL {
            step: self.clone(),
            query,
            datasets: datasets.clone(),
            schema,
            export_file_path,
        });
        execution_context.write(ContextValue::Table(ArrowTable::new(datasets)));
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<WorkflowEvent> for FormatterStep {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
    ) -> Result<(), OnyxError> {
        let context = execution_context.get_context();
        let step_output = execution_context
            .renderer
            .render_async(&self.template, Value::from_serialize(&context))
            .await?;

        let mut export_file_path = PathBuf::new();
        if let Some(export) = &self.export {
            let export_file_path_str = execution_context
                .renderer
                .render_async(&export.path, Value::from_serialize(&context))
                .await?;
            export_file_path = execution_context
                .config
                .project_path
                .join(export_file_path_str);
        }

        execution_context.notify(WorkflowEvent::Formatter {
            step: self.clone(),
            output: step_output.clone(),
            export_file_path,
        });
        execution_context.write(ContextValue::Text(step_output));
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<WorkflowEvent> for LoopSequentialStep {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
    ) -> Result<(), OnyxError> {
        let context = execution_context.get_context();
        let values = match &self.values {
            LoopValues::Template(ref template) => {
                let rendered = execution_context
                    .renderer
                    .eval_expression(template, &context)?;
                let value_or_none = rendered.as_object();
                if value_or_none.is_none() {
                    return Err(OnyxError::RuntimeError(format!(
                        "Values {} did not resolve to an array",
                        template,
                    )));
                }
                let rendered_value = value_or_none.unwrap();

                match rendered_value.enumerate() {
                    Enumerator::Seq(length) => {
                        let mut values = Vec::new();
                        for idx in 0..length {
                            let value = rendered_value
                                .get_value(&Value::from(idx))
                                .unwrap_or_default();
                            values.push(value.to_string());
                        }
                        values
                    }
                    _ => {
                        return Err(OnyxError::RuntimeError(format!(
                            "Values {} did not resolve to an array",
                            template,
                        )));
                    }
                }
            }
            LoopValues::Array(ref values) => values.clone(),
        };

        let mut loop_executor = execution_context.loop_executor();
        let values: Vec<ContextValue> = values
            .iter()
            .map(|v| ContextValue::Text(v.to_string()))
            .collect();
        loop_executor.params(&values, &self.steps).await?;
        loop_executor.finish();

        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<WorkflowEvent> for Step {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
    ) -> Result<(), OnyxError> {
        execution_context.notify(WorkflowEvent::StepStarted {
            name: self.name.clone(),
        });
        match &self.step_type {
            StepType::Agent(agent) => {
                let cache_result = get_agent_cache(
                    &execution_context.config.project_path.clone(),
                    agent.cache.clone(),
                    execution_context,
                )
                .await?;
                if let Some(result) = cache_result {
                    execution_context.write(result.output);
                    println!("{}", "Cache detected. Using cache.".primary());
                } else {
                    agent.execute(execution_context).await?;
                }
            }
            StepType::ExecuteSQL(execute_sql) => {
                execute_sql.execute(execution_context).await?;
            }
            StepType::Formatter(formatter) => {
                formatter.execute(execution_context).await?;
            }
            StepType::LoopSequential(loop_sequential) => {
                loop_sequential.execute(execution_context).await?;
            }
            StepType::Unknown => {
                execution_context.notify(WorkflowEvent::StepUnknown {
                    name: self.name.clone(),
                });
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<WorkflowEvent> for Vec<Step> {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
    ) -> Result<(), OnyxError> {
        let mut map_executor = execution_context.map_executor();
        map_executor
            .entries(
                self.iter()
                    .map(|s| (s.name.clone(), s.clone()))
                    .collect::<Vec<(String, Step)>>(),
            )
            .await?;
        map_executor.finish();
        Ok(())
    }
}
