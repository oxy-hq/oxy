use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use minijinja::value::Kwargs;
use minijinja::Value;

use crate::config::model::AgentStep;
use crate::config::model::ExecuteSQLStep;
use crate::config::model::FileFormat;
use crate::config::model::FormatterStep;
use crate::config::model::LoopSequentialStep;
use crate::config::model::LoopValues;
use crate::config::model::Step;
use crate::config::model::StepType;
use crate::config::model::Workflow;
use crate::config::model::SQL;
use crate::connector::Connector;
use crate::errors::OnyxError;
use crate::execute::agent::build_agent;
use crate::execute::agent::AgentInput;
use crate::execute::core::arrow_table::ArrowTable;
use crate::execute::core::value::ContextValue;
use crate::execute::core::write::Write;
use crate::execute::core::Executable;
use crate::execute::core::ExecutionContext;
use crate::execute::workflow::LoopInput;
use crate::execute::workflow::WorkflowEvent;
use crate::execute::workflow::WorkflowInput;
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
            .steps
            .execute(execution_context, ContextValue::None)
            .await?;
        execution_context.notify(WorkflowEvent::Finished).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<WorkflowInput, WorkflowEvent> for AgentStep {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
        _input: WorkflowInput,
    ) -> Result<(), OnyxError> {
        let agent_file = execution_context.config.project_path.join(&self.agent_ref);
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
        let (agent, agent_config, global_context, _) =
            build_agent(Some(&agent_file), &FileFormat::Json, Some(prompt.clone()))?;
        let step = self.clone();
        let map_agent_event = move |event| WorkflowEvent::Agent {
            orig: event,
            step: step.clone(),
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
        let step_output = agent_executor.finish();

        if let Some(cache) = &self.cache {
            if cache.enabled {
                let cache_file_path_str =
                    execution_context.renderer.render_async(&cache.path).await?;

                let cache_file_path = execution_context
                    .config
                    .project_path
                    .join(cache_file_path_str);

                execution_context
                    .notify(WorkflowEvent::CacheAgentResult {
                        result: step_output,
                        file_path: cache_file_path,
                    })
                    .await?;
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<WorkflowInput, WorkflowEvent> for ExecuteSQLStep {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
        _input: WorkflowInput,
    ) -> Result<(), OnyxError> {
        let wh = execution_context.config.find_warehouse(&self.warehouse)?;

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
                        if !variables.is_empty() {
                            execution_context
                                .renderer
                                .render_once(&query, Value::from_serialize(&variables))?
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
                step: self.clone(),
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
impl Executable<WorkflowInput, WorkflowEvent> for FormatterStep {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
        _input: WorkflowInput,
    ) -> Result<(), OnyxError> {
        let step_output = execution_context.renderer.render(&self.template)?;

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
                step: self.clone(),
                output: step_output.clone(),
                export_file_path,
            })
            .await?;
        execution_context.write(ContextValue::Text(step_output));
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<LoopInput, WorkflowEvent> for LoopSequentialStep {
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
        loop_executor
            .params(
                &mut values,
                &self.steps,
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
            .await?;
        loop_executor.finish()?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<Step, WorkflowEvent> for Step {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
        _input: Step,
    ) -> Result<(), OnyxError> {
        execution_context
            .notify(WorkflowEvent::StepStarted {
                name: self.name.clone(),
            })
            .await?;
        match &self.step_type {
            StepType::Agent(agent) => {
                let cache_result = match &agent.cache {
                    Some(cache) => match cache.enabled {
                        true => {
                            let cached_file_path =
                                execution_context.renderer.render(&cache.path)?;
                            get_agent_cache(
                                &execution_context.config.project_path.clone(),
                                &cached_file_path,
                            )
                        }
                        false => None,
                    },
                    None => None,
                };

                if let Some(cache_result) = cache_result {
                    execution_context.write(cache_result);
                    println!("{}", "Cache detected. Using cache.".primary());
                } else {
                    agent.execute(execution_context, WorkflowInput).await?;
                }
            }
            StepType::ExecuteSQL(execute_sql) => {
                execute_sql
                    .execute(execution_context, WorkflowInput)
                    .await?;
            }
            StepType::Formatter(formatter) => {
                formatter.execute(execution_context, WorkflowInput).await?;
            }
            StepType::LoopSequential(loop_sequential) => {
                loop_sequential
                    .execute(
                        execution_context,
                        LoopInput {
                            name: self.name.clone(),
                        },
                    )
                    .await?;
            }
            StepType::Unknown => {
                execution_context
                    .notify(WorkflowEvent::StepUnknown {
                        name: self.name.clone(),
                    })
                    .await?;
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable<ContextValue, WorkflowEvent> for Vec<Step> {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, WorkflowEvent>,
        input: ContextValue,
    ) -> Result<(), OnyxError> {
        let mut map_executor = execution_context.map_executor();
        map_executor.prefill("value", input);
        map_executor
            .entries(
                self.iter()
                    .map(|s| (s.name.clone(), s.clone(), s.clone()))
                    .collect::<Vec<(String, Step, Step)>>(),
            )
            .await?;
        map_executor.finish();
        Ok(())
    }
}
