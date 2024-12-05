use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use arrow::array::RecordBatch;
use arrow::util::pretty::pretty_format_batches;
use backon::ExponentialBuilder;
use backon::Retryable;
use minijinja::value::Enumerator;
use minijinja::{Environment, Value};

use crate::config::model::AgentStep;
use crate::config::model::Config;
use crate::config::model::ExecuteSQLStep;
use crate::config::model::FileFormat;
use crate::config::model::LoopValues;
use crate::config::model::Step;
use crate::config::model::StepType;
use crate::config::model::Warehouse;
use crate::config::model::Workflow;
use crate::connector::Connector;
use crate::utils::print_colored_sql;
use crate::workflow::context::Output;
use crate::StyledText;
use crate::{
    ai::{agent::LLMAgent, from_config},
    utils::list_file_stems,
};

use super::context::ContextBuilder;
use super::table::J2Table;

#[derive(Default)]
pub struct WorkflowExecutor {
    agents: HashMap<String, Box<dyn LLMAgent + Send + Sync>>,
    warehouses: HashMap<String, Warehouse>,
    data_path: PathBuf,
}

impl WorkflowExecutor {
    pub async fn init(&mut self, config: &Config) -> anyhow::Result<()> {
        let agent_names = list_file_stems(config.project_path.join("agents").to_str().unwrap())?;
        for agent_name in agent_names {
            let agent_config = config.load_config(Some(&agent_name))?;
            let agent = from_config(&agent_name, config, &agent_config, &FileFormat::Json).await?;
            self.agents.insert(agent_name, agent);
        }
        for warehouse in &config.warehouses {
            self.warehouses
                .insert(warehouse.name.clone(), warehouse.clone());
        }
        self.data_path = config.project_path.join("data");
        Ok(())
    }

    async fn execute_agent(
        &self,
        agent_step: &AgentStep,
        context: &Value,
    ) -> anyhow::Result<String> {
        let agent = self
            .agents
            .get(&agent_step.agent_ref)
            .unwrap_or_else(|| panic!("Agent {} not found", agent_step.agent_ref));
        let prompt = render_template(&agent_step.prompt, context);
        log::info!("Prompt: {}", &prompt);
        let step_output = (|| async { agent.request(&prompt).await })
            .retry(ExponentialBuilder::default().with_max_times(agent_step.retry))
            // Notify when retrying
            .notify(|err: &anyhow::Error, duration: std::time::Duration| {
                println!("\n\x1b[93mRetrying after {:?} ... \x1b[0m", duration);
                println!("Reason {:?}", err);
            })
            .await?;
        Ok(step_output)
    }

    async fn execute_sql(
        &self,
        execute_sql_step: &ExecuteSQLStep,
        context: &Value,
    ) -> anyhow::Result<Vec<RecordBatch>> {
        let wh_config = self.warehouses.get(&execute_sql_step.warehouse);
        log::info!("SQL Context: {:?}", context);
        match wh_config {
            Some(wh) => {
                let mut variables = HashMap::new();
                if let Some(vars) = &execute_sql_step.variables {
                    for (key, value) in vars {
                        let rendered_value = render_template(value, context);
                        variables.insert(key.clone(), rendered_value);
                    }
                }

                let rendered_sql_file = render_template(&execute_sql_step.sql_file, context);
                let query_file = self.data_path.join(&rendered_sql_file);
                let query = match fs::read_to_string(&query_file) {
                    Ok(query) => {
                        if !variables.is_empty() {
                            render_template(&query, &Value::from_serialize(&variables))
                        } else {
                            query
                        }
                    }
                    Err(e) => {
                        return Err(anyhow::anyhow!(
                            "Error reading query file {}: {}",
                            &query_file.display(),
                            e
                        ));
                    }
                };

                print_colored_sql(&query);
                let results = Connector::new(wh).run_query_and_load(&query).await?;
                let batches_display = pretty_format_batches(&results)?;
                println!("\n\x1b[1;32mResults:\x1b[0m");
                println!("{}", batches_display);
                Ok(results)
            }
            None => Err(anyhow::anyhow!(
                "Warehouse {} not found",
                execute_sql_step.warehouse
            )),
        }
    }

    async fn execute_steps(
        &self,
        steps: &Vec<Step>,
        execution_context: &mut ContextBuilder,
    ) -> anyhow::Result<()> {
        for (i, step) in steps.iter().enumerate() {
            if i == 0 {
                println!("⏳Starting {}", step.name.text());
            } else {
                println!("\n⏳Starting {}", step.name.text());
            }
            let template_context = execution_context.build_j2_context();

            match &step.step_type {
                StepType::Agent(agent_step) => {
                    let step_output = self.execute_agent(agent_step, &template_context).await?;
                    execution_context.add_output(step.name.clone(), Output::Single(step_output));
                }
                StepType::ExecuteSQL(execute_sql_step) => {
                    let step_output = self
                        .execute_sql(execute_sql_step, &template_context)
                        .await?;
                    execution_context
                        .add_output(step.name.clone(), Output::Table(J2Table::new(step_output)));
                }
                StepType::LoopSequential(loop_step) => {
                    execution_context.enter_loop_scope(step.name.to_string());
                    match loop_step.values {
                        LoopValues::Template(ref template) => {
                            let variable = eval_expression(template, &template_context)?;
                            log::info!("Loop values: {} {:?}", template, variable);
                            let value_or_none = variable.as_object();
                            if value_or_none.is_none() {
                                return Err(anyhow::anyhow!(
                                    "Values {} did not resolve to an array",
                                    template,
                                ));
                            }
                            let value = value_or_none.unwrap();

                            match value.enumerate() {
                                Enumerator::Seq(length) => {
                                    for idx in 0..length {
                                        let value =
                                            value.get_value(&Value::from(idx)).unwrap_or_default();
                                        execution_context.update_value(value.to_string());
                                        Box::pin(
                                            self.execute_steps(&loop_step.steps, execution_context),
                                        )
                                        .await?;
                                    }
                                }
                                _ => {
                                    return Err(anyhow::anyhow!(
                                        "Values {} did not resolve to an array",
                                        template,
                                    ));
                                }
                            }
                        }
                        LoopValues::Array(ref values) => {
                            for step_value in values.iter() {
                                execution_context.update_value(step_value.to_string());
                                Box::pin(self.execute_steps(&loop_step.steps, execution_context))
                                    .await?;
                            }
                        }
                    }

                    execution_context.escape_scope();
                }
                StepType::Formatter(formatter_step) => {
                    let step_output = render_template(&formatter_step.template, &template_context);
                    println!("{}", "\nOutput:".primary());
                    println!("{}", step_output);
                    execution_context.add_output(step.name.clone(), Output::Single(step_output));
                }
                StepType::Unknown => {
                    println!("Encountered unknown step type. Skipping.");
                }
            }
        }
        Ok(())
    }

    pub async fn execute(&self, workflow: &Workflow) -> anyhow::Result<Output> {
        println!("\n⏳Running workflow: {}", workflow.name.text());
        let mut execution_context = ContextBuilder::new();
        self.execute_steps(&workflow.steps, &mut execution_context)
            .await?;
        let results = execution_context.get_outputs();
        log::info!("\n\x1b[1;32mWorkflow output:\n{:?}\x1b[0m", results);
        Ok(results.clone())
    }
}

fn render_template(template: &str, context: &Value) -> String {
    let mut env = Environment::new();
    env.add_template(template, template).unwrap();
    let tmpl = env.get_template(template).unwrap();
    tmpl.render(context).unwrap()
}

fn eval_expression(template: &str, context: &Value) -> anyhow::Result<Value> {
    let mut env = Environment::new();
    env.add_template(template, template)?;
    let tmpl = env.get_template(template)?;
    let variables = tmpl.undeclared_variables(true);
    if variables.len() != 1 {
        return Err(anyhow::anyhow!(
            "Expected one variable in expression, found {}",
            variables.len()
        ));
    }
    let variable = variables.iter().next().unwrap();
    let expression = env.compile_expression(variable)?;
    let value = expression.eval(context)?;
    log::info!(
        "Evaluated expression: {} -> {:?}",
        template,
        value.as_object().unwrap().repr()
    );
    Ok(value)
}
