use itertools::Itertools;

use crate::{
    agent::AgentLauncherExecutable,
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        types::{OutputGetter, RelevantContextGetter, TargetOutput},
    },
    workflow::WorkflowLauncherExecutable,
};

use super::types::EvalTarget;

#[derive(Clone, Debug)]
pub(super) struct TargetExecutable {
    task_ref: Option<String>,
    relevant_context_getter: RelevantContextGetter,
}

impl TargetExecutable {
    pub fn new(task_ref: Option<String>, relevant_context_getter: RelevantContextGetter) -> Self {
        Self {
            task_ref,
            relevant_context_getter,
        }
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
                WorkflowLauncherExecutable
                    .execute(&execution_context, workflow_input)
                    .await
            }
            EvalTarget::Agent(agent_input) => {
                AgentLauncherExecutable
                    .execute(&execution_context, agent_input)
                    .await
            }
        }?;
        match &self.task_ref {
            Some(task_ref) => {
                let output = output_container.project_ref(task_ref)?;
                output
                    .into_iter()
                    .map(|item| {
                        OutputGetter {
                            value: &item,
                            relevant_context_getter: &self.relevant_context_getter,
                        }
                        .try_into()
                    })
                    .try_collect()
            }
            None => {
                let output = OutputGetter {
                    value: &output_container,
                    relevant_context_getter: &self.relevant_context_getter,
                }
                .try_into();
                output.map(|item| vec![item])
            }
        }
    }
}
