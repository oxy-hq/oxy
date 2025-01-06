use std::collections::HashMap;
use std::fs;

use backon::ExponentialBuilder;
use backon::Retryable;
use minijinja::value::Enumerator;
use minijinja::Value;

use crate::ai::utils::record_batches_to_table;
use crate::config::load_config;
use crate::config::model::AgentStep;
use crate::config::model::ExecuteSQLStep;
use crate::config::model::FileFormat;
use crate::config::model::FormatterStep;
use crate::config::model::LoopSequentialStep;
use crate::config::model::LoopValues;
use crate::config::model::ProjectPath;
use crate::config::model::Step;
use crate::config::model::StepType;
use crate::config::model::Workflow;
use crate::connector::Connector;
use crate::errors::OnyxError;
use crate::execute::agent::run_agent;
use crate::execute::core::arrow_table::ArrowTable;
use crate::execute::core::value::ContextValue;
use crate::execute::core::Executable;
use crate::execute::core::ExecutionContext;
use crate::execute::core::Write;
use crate::utils::print_colored_sql;
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
impl Executable for WorkflowExecutor {
    async fn execute(&self, execution_context: &mut ExecutionContext<'_>) -> Result<(), OnyxError> {
        println!("\n⏳Running workflow: {}", self.workflow.name.text());
        self.workflow.steps.execute(execution_context).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable for AgentStep {
    async fn execute(&self, execution_context: &mut ExecutionContext<'_>) -> Result<(), OnyxError> {
        let agent_file = ProjectPath::get_path(&self.agent_ref);
        let context = execution_context.get_context();
        let prompt = execution_context
            .renderer
            .render_async(&self.prompt, context)
            .await?;
        let step_output =
            (|| async { run_agent(Some(&agent_file), &FileFormat::Json, &prompt).await })
                .retry(ExponentialBuilder::default().with_max_times(self.retry))
                // Notify when retrying
                .notify(|err: &OnyxError, duration: std::time::Duration| {
                    println!("\n\x1b[93mRetrying after {:?} ... \x1b[0m", duration);
                    println!("Reason {:?}", err);
                })
                .await?;
        execution_context.write(ContextValue::Text(step_output.output.to_string()));
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable for ExecuteSQLStep {
    async fn execute(&self, execution_context: &mut ExecutionContext<'_>) -> Result<(), OnyxError> {
        let context = execution_context.get_context();
        let config = load_config()?;
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
        let query_file = ProjectPath::get_path(&rendered_sql_file);
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

        print_colored_sql(&query);
        let (datasets, schema) = Connector::new(wh).run_query_and_load(&query).await?;
        let batches_display = record_batches_to_table(&datasets, &schema).map_err(|e| {
            OnyxError::ConfigurationError(format!("Error displaying results: {}", e))
        })?;
        println!("\n\x1b[1;32mResults:\x1b[0m");
        println!("{}", batches_display);
        execution_context.write(ContextValue::Table(ArrowTable::new(datasets)));
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable for FormatterStep {
    async fn execute(&self, execution_context: &mut ExecutionContext<'_>) -> Result<(), OnyxError> {
        let context = execution_context.get_context();
        let step_output = execution_context
            .renderer
            .render_async(&self.template, context)
            .await?;
        println!("{}", "\nOutput:".primary());
        println!("{}", step_output);
        execution_context.write(ContextValue::Text(step_output));
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable for LoopSequentialStep {
    async fn execute(&self, execution_context: &mut ExecutionContext<'_>) -> Result<(), OnyxError> {
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
impl Executable for Step {
    async fn execute(&self, execution_context: &mut ExecutionContext<'_>) -> Result<(), OnyxError> {
        println!("\n⏳Starting {}", self.name.text());
        match &self.step_type {
            StepType::Agent(agent) => {
                agent.execute(execution_context).await?;
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
                println!("Encountered unknown step type. Skipping.");
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Executable for Vec<Step> {
    async fn execute(&self, execution_context: &mut ExecutionContext<'_>) -> Result<(), OnyxError> {
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
