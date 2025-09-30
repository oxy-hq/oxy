use crate::{
    agent::builders::fsm::{
        config::AgenticInput,
        control::{TransitionContext, config::OutputArtifact},
        machine::Agent,
        states::{data_app::DataAppState, qa::QAState, query::QueryState},
        subflow::state::AgenticArtifact,
    },
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::fsm::{FSM, Trigger},
    },
};

pub struct SubflowRun<S> {
    pub objective: String,
    pub src: String,
    pub _state: std::marker::PhantomData<S>,
}

pub trait CollectArtifact {
    fn get_artifacts(&self) -> &[AgenticArtifact];
    fn collect(&mut self, artifact: AgenticArtifact);
    fn collect_artifacts(&mut self, artifacts: Vec<AgenticArtifact>) {
        for artifact in artifacts {
            self.collect(artifact)
        }
    }
}

pub trait CollectArtifactDelegator {
    fn target(&self) -> &dyn CollectArtifact;
    fn target_mut(&mut self) -> &mut dyn CollectArtifact;
}

impl<T> CollectArtifact for T
where
    T: CollectArtifactDelegator,
{
    fn get_artifacts(&self) -> &[AgenticArtifact] {
        self.target().get_artifacts()
    }

    fn collect(&mut self, artifact: AgenticArtifact) {
        self.target_mut().collect(artifact)
    }

    fn collect_artifacts(&mut self, artifacts: Vec<AgenticArtifact>) {
        self.target_mut().collect_artifacts(artifacts)
    }
}

impl<S> SubflowRun<S>
where
    S: TransitionContext + Send + Sync,
{
    pub fn new(objective: String, src: String) -> Self {
        Self {
            objective,
            src,
            _state: std::marker::PhantomData,
        }
    }
}

#[async_trait::async_trait]
impl<S> Trigger for SubflowRun<S>
where
    S: TransitionContext + CollectArtifact + Send + Sync,
{
    type State = S;

    async fn run(
        &self,
        execution_context: &ExecutionContext,
        mut current_state: Self::State,
    ) -> Result<Self::State, OxyError> {
        let agentic_config = execution_context
            .project
            .config_manager
            .resolve_agentic_workflow(&self.src)
            .await?;
        match agentic_config.end.end.output_artifact {
            OutputArtifact::None => {
                let machine =
                    Agent::<QAState>::new(execution_context.project.clone(), agentic_config)
                        .await?;
                let output = FSM::new(machine)
                    .execute(
                        execution_context,
                        AgenticInput {
                            prompt: self.objective.clone(),
                            trace: current_state.get_messages(),
                        },
                    )
                    .await?;
                current_state.collect_artifacts(output.get_artifacts().to_vec());
                current_state.collect(output.into());
            }
            OutputArtifact::Query => {
                let machine =
                    Agent::<QueryState>::new(execution_context.project.clone(), agentic_config)
                        .await?;
                let output = FSM::new(machine)
                    .execute(
                        execution_context,
                        AgenticInput {
                            prompt: self.objective.clone(),
                            trace: current_state.get_messages(),
                        },
                    )
                    .await?;
                current_state.collect(output.into());
            }
            OutputArtifact::App => {
                let machine =
                    Agent::<DataAppState>::new(execution_context.project.clone(), agentic_config)
                        .await?;
                let output = FSM::new(machine)
                    .execute(
                        execution_context,
                        AgenticInput {
                            prompt: self.objective.clone(),
                            trace: current_state.get_messages(),
                        },
                    )
                    .await?;
                current_state.collect(output.into());
            }
            _ => todo!("Handle other output artifact types"),
        };
        Ok(current_state)
    }
}
