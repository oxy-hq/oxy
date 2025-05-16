use crate::{
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        types::{Chunk, Output, OutputContainer, Prompt},
    },
    workflow::{WorkflowInput, WorkflowLauncherExecutable},
};

use super::types::WorkflowInput as WorkflowToolInput;

#[derive(Debug, Clone)]
pub struct WorkflowExecutable;

impl WorkflowExecutable {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl Executable<WorkflowToolInput> for WorkflowExecutable {
    type Response = OutputContainer;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: WorkflowToolInput,
    ) -> Result<Self::Response, OxyError> {
        execution_context
            .write_chunk(Chunk {
                key: None,
                delta: Output::Prompt(Prompt::new("Running workflow...".to_string())),
                finished: true,
            })
            .await?;
        let output = WorkflowLauncherExecutable
            .execute(
                execution_context,
                WorkflowInput {
                    restore_from_checkpoint: false,
                    workflow_ref: input.workflow_config.workflow_ref.clone(),
                    variables: input.variables,
                },
            )
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to run workflow: {}", e)))?;
        let output = match input.workflow_config.output_task_ref {
            Some(task_ref) => output
                .project_ref(&task_ref)?
                .first()
                .ok_or(OxyError::RuntimeError(format!(
                    "Workflow output task {} not found",
                    task_ref
                )))?
                .to_owned()
                .clone(),
            None => output,
        };
        Ok(output)
    }
}
