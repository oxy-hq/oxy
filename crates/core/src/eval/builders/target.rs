use std::collections::HashMap;

use itertools::Itertools;

use crate::{
    agent::AgentLauncher,
    config::constants::{AGENT_SOURCE_PROMPT, WORKFLOW_SOURCE},
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        types::{OutputContainer, TargetOutput},
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
                let workflow_context = execution_context.with_child_source(
                    workflow_input.workflow_ref.to_string(),
                    WORKFLOW_SOURCE.to_string(),
                );
                WorkflowLauncher::new()
                    .with_external_context(&workflow_context)
                    .await?
                    .launch(workflow_input, execution_context.writer.clone())
                    .await
            }
            EvalTarget::Agent(agent_input) => {
                let prompt = agent_input.prompt.clone();
                let output = AgentLauncher::new()
                    .with_external_context(execution_context)?
                    .launch(agent_input, execution_context.writer.clone())
                    .await?;
                Ok(OutputContainer::Metadata {
                    output,
                    metadata: HashMap::from_iter([(AGENT_SOURCE_PROMPT.to_string(), prompt)]),
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
