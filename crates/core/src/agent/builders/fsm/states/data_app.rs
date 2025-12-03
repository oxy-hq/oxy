use async_openai::types::chat::ChatCompletionRequestMessage;

use crate::{
    adapters::openai::OpenAIAdapter,
    agent::builders::fsm::{
        config::{AgenticConfig, AgenticInput},
        control::{Memory, TransitionContext, TransitionContextDelegator, TriggerBuilder},
        data_app::{
            BuildDataApp, CollectInsights, CollectInsightsDelegator, GenerateInsight, Insights,
            config::Insight,
        },
        query::{AutoSQL, Dataset, PrepareData, PrepareDataDelegator, config::Query},
        subflow::ArtifactsState,
        viz::{CollectViz, CollectVizDelegator, VizState},
    },
    config::{constants::AGENT_START_TRANSITION, model::AppConfig},
    errors::OxyError,
    execute::{
        ExecutionContext,
        builders::fsm::Trigger,
        types::{Output, OutputContainer},
    },
};

pub struct DataAppState {
    memory: Memory,
    data: Dataset,
    viz: VizState,
    #[allow(dead_code)]
    artifacts: ArtifactsState,
    app: Option<AppConfig>,
    insight: Insights,
}

impl std::fmt::Debug for DataAppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataAppState").finish()
    }
}

impl DataAppState {
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
            app: None,
            insight: Insights::new(),
        }
    }

    pub fn from_input(instruction: String, input: &AgenticInput) -> Self {
        Self::new(instruction, input.prompt.clone(), input.trace.clone())
    }

    pub fn set_app(&mut self, app: AppConfig) {
        self.app = Some(app);
    }

    pub fn get_app(&self) -> Option<&AppConfig> {
        self.app.as_ref()
    }
}

impl TransitionContextDelegator for DataAppState {
    fn target(&self) -> &dyn TransitionContext {
        &self.memory
    }

    fn target_mut(&mut self) -> &mut dyn TransitionContext {
        &mut self.memory
    }
}

impl PrepareDataDelegator for DataAppState {
    fn target(&self) -> &dyn PrepareData {
        &self.data
    }

    fn target_mut(&mut self) -> &mut dyn PrepareData {
        &mut self.data
    }
}

impl CollectVizDelegator for DataAppState {
    fn target(&self) -> &dyn CollectViz {
        &self.viz
    }

    fn target_mut(&mut self) -> &mut dyn CollectViz {
        &mut self.viz
    }
}

impl CollectInsightsDelegator for DataAppState {
    fn target(&self) -> &dyn CollectInsights {
        &self.insight
    }

    fn target_mut(&mut self) -> &mut dyn CollectInsights {
        &mut self.insight
    }
}

#[async_trait::async_trait]
impl TriggerBuilder for DataAppState {
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

    async fn build_insight_trigger(
        &self,
        execution_context: &ExecutionContext,
        agentic_config: &AgenticConfig,
        insight_config: &Insight,
        objective: String,
    ) -> Result<Box<dyn Trigger<State = Self>>, OxyError>
    where
        Self: std::fmt::Debug,
    {
        let model_ref = insight_config
            .model
            .as_deref()
            .unwrap_or(&agentic_config.model);
        let openai_adapter =
            OpenAIAdapter::from_config(execution_context.project.clone(), model_ref).await?;
        Ok(Box::new(GenerateInsight::new(
            openai_adapter,
            objective,
            insight_config.clone(),
        )))
    }

    async fn build_data_app_trigger(
        &self,
        _execution_context: &ExecutionContext,
        _agentic_config: &AgenticConfig,
        objective: String,
    ) -> Result<Box<dyn Trigger<State = Self>>, OxyError>
    where
        Self: std::fmt::Debug,
    {
        Ok(Box::new(BuildDataApp::new(objective)))
    }
}

impl From<DataAppState> for OutputContainer {
    fn from(val: DataAppState) -> Self {
        Output::Text(val.memory.get_content().to_string()).into()
    }
}
