use std::{collections::HashMap, sync::Arc};

use itertools::Itertools;

use crate::{
    agent::{AgentLauncherExecutable, AgentReferencesHandler},
    config::constants::AGENT_SOURCE_PROMPT,
    errors::OxyError,
    execute::{
        Executable, ExecutionContext, execute_with_handler,
        types::{Metadata, OutputContainer, TargetOutput},
    },
    workflow::WorkflowLauncher,
};

use super::types::EvalTarget;

#[derive(Clone, Debug)]
pub(super) struct TargetExecutable {
    task_ref: Option<String>,
}

impl TargetExecutable {
    pub fn new(task_ref: Option<String>) -> Self {
        Self { task_ref }
    }
}

#[async_trait::async_trait]
impl Executable<EvalTarget> for TargetExecutable {
    type Response = Vec<TargetOutput>;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: EvalTarget,
    ) -> Result<Self::Response, OxyError> {
        let output_container = match input {
            EvalTarget::Workflow(workflow_input) => {
                WorkflowLauncher::new()
                    .with_external_context(&execution_context)
                    .await?
                    .launch(workflow_input, execution_context.writer.clone())
                    .await
            }
            EvalTarget::Agent(agent_input) => {
                let prompt = agent_input.prompt.clone();
                let agent_reference_handler =
                    AgentReferencesHandler::new(execution_context.writer.clone());
                let references = agent_reference_handler.references.clone();
                let output = execute_with_handler(
                    AgentLauncherExecutable,
                    &execution_context,
                    agent_input,
                    agent_reference_handler,
                )
                .await?;
                let references = Arc::try_unwrap(references)
                    .map_err(|_| {
                        OxyError::RuntimeError("Failed to unwrap agent references".to_string())
                    })?
                    .into_inner()?;
                Ok(OutputContainer::Metadata {
                    value: Metadata {
                        output,
                        references,
                        metadata: HashMap::from_iter([(AGENT_SOURCE_PROMPT.to_string(), prompt)]),
                    },
                })
            }
        }?;
        match &self.task_ref {
            Some(task_ref) => {
                let output = output_container.project_ref(task_ref)?;
                output.into_iter().map(|item| item.try_into()).try_collect()
            }
            None => {
                let output = (&output_container).try_into();
                output.map(|item| vec![item])
            }
        }
    }
}
