use std::collections::HashMap;

use super::Tool;
use crate::config::ConfigManager;
use crate::config::model::Workflow;
use crate::execute::core::run;
use crate::execute::core::value::ContextValue;
use crate::workflow::Logger::WorkflowCLILogger;
use crate::workflow::WorkflowInput;
use crate::{
    config::ConfigBuilder,
    execute::{agent::ToolCall, core::event::Dispatcher, workflow::WorkflowReceiver},
    utils::find_project_path,
    workflow::executor::WorkflowExecutor,
};
use async_trait::async_trait;
use minijinja::Value;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, Debug, JsonSchema)]
pub struct WorkflowParams {}

#[derive(Debug)]
pub struct WorkflowTool {
    pub tool_name: String,
    pub tool_description: String,
    pub workflow_ref: String,
    pub output_task_ref: String,
    pub variables: Option<HashMap<String, String>>,
}

#[async_trait]
impl Tool for WorkflowTool {
    type Input = WorkflowParams;

    fn name(&self) -> String {
        self.tool_name.to_string()
    }

    fn description(&self) -> String {
        self.tool_description.to_string()
    }

    async fn call_internal(&self, parameters: &WorkflowParams) -> anyhow::Result<ToolCall> {
        let (workflow, config) = self.setup_workflow().await?;
        let output = self.execute_workflow(workflow, config).await?;
        let task_output = self.extract_task_output(output)?;

        Ok(ToolCall {
            name: self.name(),
            output: task_output,
            metadata: None,
        })
    }
}

impl WorkflowTool {
    async fn setup_workflow(&self) -> anyhow::Result<(Workflow, ConfigManager)> {
        let project_path = find_project_path()?;
        let config = ConfigBuilder::new()
            .with_project_path(project_path)
            .unwrap()
            .build()
            .await?;

        let workflow = config
            .resolve_workflow(&self.workflow_ref)
            .await
            .map_err(anyhow::Error::from)?;

        Ok((workflow, config))
    }

    async fn execute_workflow(
        &self,
        workflow: Workflow,
        config: ConfigManager,
    ) -> anyhow::Result<ContextValue> {
        let dispatcher = Dispatcher::new(vec![Box::new(WorkflowReceiver::new(WorkflowCLILogger))]);
        let executor = WorkflowExecutor::new(workflow.clone());
        let ctx = Value::from_serialize(&workflow.variables);

        run(
            &executor,
            WorkflowInput,
            config,
            ctx,
            Some(&workflow),
            dispatcher,
        )
        .await
        .map_err(anyhow::Error::from)
    }

    fn extract_task_output(&self, output: ContextValue) -> anyhow::Result<String> {
        let template = format!("{{{{ {} }}}}", self.output_task_ref);
        let env = minijinja::Environment::new();
        let tmpl = env.template_from_str(&template)?;

        tmpl.render(&Value::from_object(output))
            .map_err(anyhow::Error::from)
    }
}
