use crate::{
    adapters::checkpoint::types::RetryStrategy,
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        types::{Chunk, Output, OutputContainer, Prompt},
    },
    observability::events,
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

    #[tracing::instrument(skip_all, err, fields(
        otel.name = events::tool::WORKFLOW_EXECUTE,
        oxy.span_type = events::tool::TOOL_CALL_TYPE,
    ))]
    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: WorkflowToolInput,
    ) -> Result<Self::Response, OxyError> {
        events::tool::tool_call_input(&input);
        execution_context
            .write_chunk(Chunk {
                key: None,
                delta: Output::Prompt(Prompt::new("Running workflow...".to_string())),
                finished: true,
            })
            .await?;
        let result = WorkflowLauncherExecutable
            .execute(
                execution_context,
                WorkflowInput {
                    retry: RetryStrategy::NoRetry {
                        variables: input.variables.map(|v| v.into_iter().collect()),
                    },
                    workflow_ref: input.workflow_config.workflow_ref.clone(),
                },
            )
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to run workflow: {e}")))?;
        let output = match input.workflow_config.output_task_ref {
            Some(task_ref) => result
                .project_ref(&task_ref)?
                .first()
                .ok_or(OxyError::RuntimeError(format!(
                    "Workflow output task {task_ref} not found"
                )))?
                .to_owned()
                .clone(),
            None => result,
        };

        events::tool::tool_call_output(&output);
        Ok(output)
    }
}
