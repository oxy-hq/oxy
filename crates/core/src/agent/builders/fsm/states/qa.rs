use async_openai::types::chat::ChatCompletionRequestMessage;

use crate::{
    adapters::openai::OpenAIAdapter,
    agent::builders::fsm::{
        config::{AgenticConfig, AgenticInput},
        control::{Memory, TransitionContext, TransitionContextDelegator, TriggerBuilder},
        query::{AutoSQL, Dataset, PrepareData, PrepareDataDelegator, config::Query},
        subflow::{ArtifactsState, CollectArtifact, CollectArtifactDelegator},
        viz::{CollectViz, CollectVizDelegator, VizState},
    },
    config::constants::AGENT_START_TRANSITION,
    errors::OxyError,
    execute::{
        ExecutionContext,
        builders::fsm::Trigger,
        types::{Output, OutputContainer},
    },
};

pub struct QAState {
    memory: Memory,
    data: Dataset,
    viz: VizState,
    artifacts: ArtifactsState,
}

impl std::fmt::Debug for QAState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QAState").finish()
    }
}

impl QAState {
    pub fn new(
        instruction: String,
        user_query: String,
        history: Vec<ChatCompletionRequestMessage>,
    ) -> Self {
        Self {
            memory: Memory::new(
                AGENT_START_TRANSITION.to_string(),
                instruction,
                user_query,
                history,
            ),
            data: Dataset::new(),
            artifacts: ArtifactsState::new(),
            viz: VizState::new(),
        }
    }

    pub fn from_input(instruction: String, input: &AgenticInput) -> Self {
        Self::new(instruction, input.prompt.clone(), input.trace.clone())
    }
}

impl TransitionContextDelegator for QAState {
    fn target(&self) -> &dyn TransitionContext {
        &self.memory
    }

    fn target_mut(&mut self) -> &mut dyn TransitionContext {
        &mut self.memory
    }
}

impl PrepareDataDelegator for QAState {
    fn target(&self) -> &dyn PrepareData {
        &self.data
    }

    fn target_mut(&mut self) -> &mut dyn PrepareData {
        &mut self.data
    }
}

impl CollectVizDelegator for QAState {
    fn target(&self) -> &dyn CollectViz {
        &self.viz
    }

    fn target_mut(&mut self) -> &mut dyn CollectViz {
        &mut self.viz
    }
}

impl CollectArtifactDelegator for QAState {
    fn target(&self) -> &dyn CollectArtifact {
        &self.artifacts
    }

    fn target_mut(&mut self) -> &mut dyn CollectArtifact {
        &mut self.artifacts
    }
}

#[async_trait::async_trait]
impl TriggerBuilder for QAState {
    async fn build_query_trigger(
        &self,
        execution_context: &ExecutionContext,
        agentic_config: &AgenticConfig,
        query_config: &Query,
        objective: String,
    ) -> Result<Box<dyn Trigger<State = Self>>, OxyError>
    where
        Self: std::fmt::Debug,
    {
        let model_ref = query_config
            .model
            .as_deref()
            .unwrap_or(&agentic_config.model);
        let openai_adapter =
            OpenAIAdapter::from_config(execution_context.project.clone(), model_ref).await?;
        Ok(Box::new(AutoSQL::new(
            openai_adapter,
            query_config.clone(),
            objective,
        )))
    }

    async fn build_viz_trigger(
        &self,
        execution_context: &ExecutionContext,
        agentic_config: &AgenticConfig,
        viz_config: &crate::agent::builders::fsm::viz::config::Visualize,
        objective: String,
    ) -> Result<Box<dyn Trigger<State = Self>>, OxyError>
    where
        Self: std::fmt::Debug,
    {
        let model_ref = viz_config.model.as_deref().unwrap_or(&agentic_config.model);
        let openai_adapter =
            OpenAIAdapter::from_config(execution_context.project.clone(), model_ref).await?;
        Ok(Box::new(
            crate::agent::builders::fsm::viz::GenerateViz::new(
                objective,
                openai_adapter,
                viz_config.clone(),
            ),
        ))
    }
}

impl From<QAState> for OutputContainer {
    fn from(val: QAState) -> Self {
        Output::Text(val.memory.get_content().to_string()).into()
    }
}
