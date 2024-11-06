use std::collections::HashMap;

use backon::ExponentialBuilder;
use backon::Retryable;
use minijinja::{Environment, Value};

use crate::{
    ai::{agent::LLMAgent, from_config},
    utils::{list_file_stems, truncate_with_ellipsis},
    yaml_parsers::{config_parser::Config, workflow_parser::Workflow},
};

#[derive(Default)]
pub struct WorkflowExecutor {
    agents: HashMap<String, Box<dyn LLMAgent + Send + Sync>>,
}

impl WorkflowExecutor {
    pub async fn load_agents(&mut self, config: &Config) -> anyhow::Result<()> {
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
            let agent = from_config(&agent_name, &config, &agent_config).await;
            self.agents.insert(agent_name, agent);
        }
        Ok(())
    }

    pub async fn execute(&self, workflow: &Workflow) -> anyhow::Result<String> {
        println!("\n\x1b[1;32mRunning workflow: {}\x1b[0m", workflow.name);
        let mut workflow_output: Option<String> = None;
        let mut results = HashMap::<String, String>::default();
        for step in &workflow.steps {
            println!("\n\x1b[1;32mStarting {}\x1b[0m", step.name);
            let agent = self
                .agents
                .get(&step.agent_ref)
                .expect(format!("Agent {} not found", step.agent_ref).as_str());
            let template_context = Value::from(results.clone());
            let mut env = Environment::new();
            env.add_template("step_instruct", &step.prompt).unwrap();
            let tmpl = env.get_template("step_instruct").unwrap();
            let prompt = tmpl.render(template_context).unwrap();
            log::info!("Prompt: {}", &prompt);
            let step_output = (|| async { agent.request(&prompt).await })
                .retry(ExponentialBuilder::default().with_max_times(step.retry))
                // Notify when retrying
                .notify(|err: &anyhow::Error, dur: std::time::Duration| {
                    println!("\n\x1b[93mRetrying {} after {:?} ... \x1b[0m", step.name, dur);
                    println!("Reason {:?}", err);
                })
                .await?;
            workflow_output = Some(step_output.clone());
            results.insert(
                step.name.clone(),
                truncate_with_ellipsis(&step_output, 10000),
            );
        }
        let workflow_result = workflow_output.unwrap_or_default();
        log::info!("\n\x1b[1;32mWorkflow output:\n{}\x1b[0m", &workflow_result);

        Ok(workflow_result)
    }
}
