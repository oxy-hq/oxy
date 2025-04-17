use std::collections::HashMap;
use std::path::PathBuf;

use crate::{
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        types::{Chunk, Output, Prompt},
    },
    service::workflow::run_workflow,
    tools::tool::Tool,
    workflow::loggers::NoopLogger,
};

use minijinja::Value;

use super::types::WorkflowInput as WorkflowToolInput;

#[derive(Debug, Clone)]
pub struct WorkflowExecutable;

impl WorkflowExecutable {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for WorkflowExecutable {
    type Param = HashMap<String, String>;
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
        execution_context: &ExecutionContext,
        input: WorkflowToolInput,
    ) -> Result<Self::Response, OxyError> {
        execution_context
            .write_chunk(Chunk {
                key: None,
                delta: Output::Prompt(Prompt::new("Running workflow...".to_string())).into(),
                finished: true,
            })
            .await?;
        let output = run_workflow(
            &PathBuf::from(input.workflow_config.workflow_ref.clone()),
            NoopLogger {},
            false,
            input.variables,
        )
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to run workflow: {}", e)))?;
        let template = format!("{{{{ {} }}}}", input.workflow_config.output_task_ref);
        let env = minijinja::Environment::new();
        let tmpl = env
            .template_from_str(&template)
            .map_err(|e| OxyError::RuntimeError(e.to_string()))?;

        let workflow_data: Value = (&output).into();
        let output = tmpl
            .render(&workflow_data)
            .map_err(|e| OxyError::RuntimeError(e.to_string()))?;

        Ok(Output::Text(output))
    }
}
