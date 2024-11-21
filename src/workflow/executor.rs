use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use arrow::util::pretty::pretty_format_batches;
use backon::ExponentialBuilder;
use backon::Retryable;
use minijinja::{Environment, Value};

use crate::config::model::AgentStep;
use crate::config::model::Config;
use crate::config::model::ExecuteSQLStep;
use crate::config::model::StepType;
use crate::config::model::Warehouse;
use crate::config::model::Workflow;
use crate::connector::Connector;
use crate::utils::print_colored_sql;
use crate::StyledText;
use crate::{
    ai::{agent::LLMAgent, from_config},
    utils::{list_file_stems, truncate_with_ellipsis},
};

#[derive(Default)]
pub struct WorkflowExecutor {
    agents: HashMap<String, Box<dyn LLMAgent + Send + Sync>>,
    warehouses: HashMap<String, Warehouse>,
    data_path: PathBuf,
}

impl WorkflowExecutor {
    pub async fn init(&mut self, config: &Config) -> anyhow::Result<()> {
        let agent_names = list_file_stems(
            config
                .defaults
                .project_path
                .join("agents")
                .to_str()
                .unwrap(),
        )?;
        for agent_name in agent_names {
            let agent_config = config.load_config(Some(&agent_name))?;
            let agent = from_config(&agent_name, config, &agent_config).await;
            self.agents.insert(agent_name, agent);
        }
        for warehouse in &config.warehouses {
            self.warehouses
                .insert(warehouse.name.clone(), warehouse.clone());
        }
        self.data_path = config.defaults.project_path.join("data");
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
            .expect(format!("Agent {} not found", agent_step.agent_ref).as_str());
        let mut env = Environment::new();
        env.add_template("step_instruct", &agent_step.prompt)
            .unwrap();
        let tmpl = env.get_template("step_instruct").unwrap();
        let prompt = tmpl.render(context).unwrap();
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
        _context: &Value,
    ) -> anyhow::Result<String> {
        let wh_config = self.warehouses.get(&execute_sql_step.warehouse);
        match wh_config {
            Some(wh) => {
                let query_file = self.data_path.join(&execute_sql_step.sql_file);
                let query = match fs::read_to_string(&query_file) {
                    Ok(query) => query,
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
                Ok(batches_display.to_string())
            }
            None => {
                return Err(anyhow::anyhow!(
                    "Warehouse {} not found",
                    execute_sql_step.warehouse
                ));
            }
        }
    }

    pub async fn execute(&self, workflow: &Workflow) -> anyhow::Result<String> {
        println!("\n⏳Running workflow: {}", workflow.name.text());
        let mut step_output: Option<String> = None;
        let mut results = HashMap::<String, String>::default();
        for (i, step) in workflow.steps.iter().enumerate() {
            if i == 0 {
                println!("⏳Starting {}", step.name.text());
            } else {
                println!("\n⏳Starting {}", step.name.text());
            }
            let template_context = Value::from(results.clone());
            match &step.step_type {
                StepType::Agent(agent_step) => {
                    step_output = Some(self.execute_agent(&agent_step, &template_context).await?);
                }
                StepType::ExecuteSQL(execute_sql_step) => {
                    step_output = Some(
                        self.execute_sql(&execute_sql_step, &template_context)
                            .await?,
                    );
                }
            }
            results.insert(
                step.name.clone(),
                truncate_with_ellipsis(&step_output.clone().unwrap_or_default(), 10000),
            );
        }
        let workflow_result = &step_output.unwrap_or_default();
        log::info!("\n\x1b[1;32mWorkflow output:\n{}\x1b[0m", &workflow_result);

        Ok(workflow_result.to_string())
    }
}
