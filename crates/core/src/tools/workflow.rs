use crate::config::ConfigManager;
use crate::config::model::Workflow;
use crate::execute::core::run;
use crate::execute::core::value::ContextValue;
use crate::execute::workflow::{WorkflowCLILogger, WorkflowInput};
use crate::{
    config::ConfigBuilder,
    execute::{core::event::Dispatcher, workflow::WorkflowReceiver},
    utils::find_project_path,
    workflow::executor::WorkflowExecutor,
};
use crate::{
    errors::OxyError,
    execute::{Executable, ExecutionContext, types::Output},
    tools::tool::Tool,
};
use minijinja::Value;

use super::types::{WorkflowInput as WorkflowToolInput, WorkflowParams};

#[derive(Debug, Clone)]
pub struct WorkflowExecutable;

impl WorkflowExecutable {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for WorkflowExecutable {
    type Param = WorkflowParams;
    type Output = String;

    fn serialize_output(&self, output: &Self::Output) -> Result<String, OxyError> {
        Ok(output.to_string())
    }
}

#[async_trait::async_trait]
impl Executable<WorkflowToolInput> for WorkflowExecutable {
    type Response = Output;

    async fn execute(
        &mut self,
        _execution_context: &ExecutionContext,
        input: WorkflowToolInput,
    ) -> Result<Self::Response, OxyError> {
        let (workflow, config) = self
            .setup_workflow(&input.workflow_config.workflow_ref)
            .await?;
        let output = self.execute_workflow(workflow, config).await?;
        let task_output =
            self.extract_task_output(output, &input.workflow_config.output_task_ref)?;

        Ok(Output::Text(task_output.to_string()))
    }
}

impl WorkflowExecutable {
    async fn setup_workflow(
        &self,
        workflow_ref: &str,
    ) -> anyhow::Result<(Workflow, ConfigManager)> {
        let project_path = find_project_path()?;
        let config = ConfigBuilder::new()
            .with_project_path(project_path)
            .unwrap()
            .build()
            .await?;

        let workflow = config
            .resolve_workflow(workflow_ref)
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

    fn extract_task_output(
        &self,
        output: ContextValue,
        output_task_ref: &str,
    ) -> anyhow::Result<String> {
        let template = format!("{{{{ {} }}}}", output_task_ref);
        let env = minijinja::Environment::new();
        let tmpl = env.template_from_str(&template)?;

        tmpl.render(&Value::from_object(output))
            .map_err(anyhow::Error::from)
    }
}
