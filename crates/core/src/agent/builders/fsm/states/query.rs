use async_openai::types::ChatCompletionRequestMessage;

use crate::{
    adapters::openai::OpenAIAdapter,
    agent::builders::fsm::{
        config::{AgenticConfig, AgenticInput},
        control::{Memory, TransitionContext, TransitionContextDelegator, TriggerBuilder},
        query::{AutoSQL, Dataset, PrepareData, PrepareDataDelegator, config::Query},
    },
    config::constants::AGENT_START_TRANSITION,
    errors::OxyError,
    execute::{
        ExecutionContext,
        builders::fsm::Trigger,
        types::{Output, OutputContainer, Table},
    },
};

pub struct QueryState {
    memory: Memory,
    data: Dataset,
}

impl QueryState {
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
        }
    }

    pub fn from_input(instruction: String, input: &AgenticInput) -> Self {
        Self::new(instruction, input.prompt.clone(), input.trace.clone())
    }

    pub fn get_tables(&self) -> &[Table] {
        self.data.get_tables()
    }
}

impl std::fmt::Debug for QueryState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueryState").finish()
    }
}

impl TransitionContextDelegator for QueryState {
    fn target(&self) -> &dyn TransitionContext {
        &self.memory
    }

    fn target_mut(&mut self) -> &mut dyn TransitionContext {
        &mut self.memory
    }
}

impl PrepareDataDelegator for QueryState {
    fn target(&self) -> &dyn PrepareData {
        &self.data
    }

    fn target_mut(&mut self) -> &mut dyn PrepareData {
        &mut self.data
    }
}

#[async_trait::async_trait]
impl TriggerBuilder for QueryState {
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
}

impl Into<OutputContainer> for QueryState {
    fn into(self) -> OutputContainer {
        Output::Text(self.memory.get_content().to_string()).into()
    }
}
