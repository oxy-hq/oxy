use std::marker::PhantomData;

use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessage, ChatCompletionRequestSystemMessageContent,
    ChatCompletionRequestUserMessage, ChatCompletionRequestUserMessageContent,
};
use tokio_stream::StreamExt;

use crate::{
    adapters::openai::OpenAIAdapter,
    agent::builders::fsm::{
        config::{AgenticConfig, Transition, TransitionMode, TriggerType},
        control::config::{EndMode, OutputArtifact, StartMode},
        data_app::config::Insight,
        machine::Agent,
        query::config::Query,
        viz::config::Visualize,
    },
    config::constants::AGENT_END_TRANSITION,
    errors::OxyError,
    execute::{
        ExecutionContext,
        builders::fsm::{State, Trigger},
        types::{Chunk, Output},
    },
};

pub trait TransitionContext {
    fn increase_iteration(&mut self);
    fn max_iterations_reached(&self, max_iteration: usize) -> bool;
    fn user_query(&self) -> &str;
    fn transition_name(&self) -> &str;
    fn set_transition_name(&mut self, name: &str);
    fn add_message(&mut self, message: String);
    fn add_tool_call(
        &mut self,
        objective: &str,
        tool_call: ChatCompletionMessageToolCall,
        tool_ret: String,
    );
    fn get_plan(&self) -> Option<&String>;
    fn set_plan(&mut self, plan: String);
    fn get_content(&self) -> &str;
    fn set_content(&mut self, content: String);
    fn get_messages(&self) -> Vec<ChatCompletionRequestMessage>;
}

pub trait TransitionContextDelegator {
    fn target(&self) -> &dyn TransitionContext;
    fn target_mut(&mut self) -> &mut dyn TransitionContext;
}

impl<T> TransitionContext for T
where
    T: TransitionContextDelegator,
{
    fn increase_iteration(&mut self) {
        self.target_mut().increase_iteration()
    }

    fn max_iterations_reached(&self, max_iteration: usize) -> bool {
        self.target().max_iterations_reached(max_iteration)
    }

    fn user_query(&self) -> &str {
        self.target().user_query()
    }

    fn transition_name(&self) -> &str {
        self.target().transition_name()
    }

    fn set_transition_name(&mut self, name: &str) {
        self.target_mut().set_transition_name(name)
    }

    fn add_message(&mut self, message: String) {
        self.target_mut().add_message(message)
    }

    fn add_tool_call(
        &mut self,
        objective: &str,
        tool_call: ChatCompletionMessageToolCall,
        tool_ret: String,
    ) {
        self.target_mut()
            .add_tool_call(objective, tool_call, tool_ret)
    }

    fn get_plan(&self) -> Option<&String> {
        self.target().get_plan()
    }

    fn set_plan(&mut self, plan: String) {
        self.target_mut().set_plan(plan)
    }

    fn get_content(&self) -> &str {
        self.target().get_content()
    }

    fn set_content(&mut self, content: String) {
        self.target_mut().set_content(content)
    }

    fn get_messages(&self) -> Vec<ChatCompletionRequestMessage> {
        self.target().get_messages()
    }
}

#[async_trait::async_trait]
pub trait TriggerBuilder {
    async fn build_viz_trigger(
        &self,
        _execution_context: &ExecutionContext,
        _agentic_config: &AgenticConfig,
        _viz_config: &Visualize,
        _objective: String,
    ) -> Result<Box<dyn Trigger<State = Self>>, OxyError>
    where
        Self: std::fmt::Debug,
    {
        Err(OxyError::RuntimeError(format!(
            "Viz trigger is not implemented for {self:?}"
        )))
    }

    async fn build_query_trigger(
        &self,
        _execution_context: &ExecutionContext,
        _agentic_config: &AgenticConfig,
        _query_config: &Query,
        _objective: String,
    ) -> Result<Box<dyn Trigger<State = Self>>, OxyError>
    where
        Self: std::fmt::Debug,
    {
        Err(OxyError::RuntimeError(format!(
            "Query trigger is not implemented for {self:?}"
        )))
    }

    async fn build_insight_trigger(
        &self,
        _execution_context: &ExecutionContext,
        _agentic_config: &AgenticConfig,
        _insight_config: &Insight,
        _objective: String,
    ) -> Result<Box<dyn Trigger<State = Self>>, OxyError>
    where
        Self: std::fmt::Debug,
    {
        Err(OxyError::RuntimeError(format!(
            "Insight trigger is not implemented for {self:?}"
        )))
    }

    async fn build_data_app_trigger(
        &self,
        _execution_context: &ExecutionContext,
        _agentic_config: &AgenticConfig,
        _objective: String,
    ) -> Result<Box<dyn Trigger<State = Self>>, OxyError>
    where
        Self: std::fmt::Debug,
    {
        Err(OxyError::RuntimeError(format!(
            "DataApp trigger is not implemented for {self:?}"
        )))
    }

    async fn build_subflow_trigger(
        &self,
        _execution_context: &ExecutionContext,
        _agentic_config: &AgenticConfig,
        _subflow_config: &crate::agent::builders::fsm::subflow::config::Subflow,
        _objective: String,
    ) -> Result<Box<dyn Trigger<State = Self>>, OxyError>
    where
        Self: std::fmt::Debug,
    {
        Err(OxyError::RuntimeError(format!(
            "Subflow trigger is not implemented for {self:?}"
        )))
    }

    async fn build(
        &self,
        execution_context: &ExecutionContext,
        agentic_config: &AgenticConfig,
        transition: Transition,
        objective: Option<String>,
    ) -> Result<Box<dyn Trigger<State = Self>>, OxyError>
    where
        Self: std::fmt::Debug + Sized + Send + Sync + TransitionContext + 'static,
    {
        match &transition.trigger {
            TriggerType::Start(start_config) => match &start_config.mode {
                StartMode::Default => Ok(Box::new(Idle::new())),
                StartMode::Plan {
                    model,
                    instruction,
                    example,
                } => {
                    let openai_adapter = OpenAIAdapter::from_config(
                        execution_context.project.clone(),
                        model.as_deref().unwrap_or(&agentic_config.model),
                    )
                    .await?;
                    let transitions = vec![
                        transition.get_transition_names(),
                        vec![AGENT_END_TRANSITION.to_string()],
                    ]
                    .concat();

                    Ok(Box::new(Plan::new(
                        openai_adapter,
                        instruction.to_string(),
                        example.to_string(),
                        agentic_config.list_transitions(&transitions)?,
                    )))
                }
            },
            TriggerType::End(end_config) => {
                let finalizer = match end_config.output_artifact {
                    OutputArtifact::App => Some(
                        self.build_data_app_trigger(
                            execution_context,
                            agentic_config,
                            objective.unwrap_or(self.user_query().to_string()),
                        )
                        .await?,
                    ),
                    _ => None,
                };
                match &end_config.mode {
                    EndMode::Default => match finalizer {
                        Some(finalizer) => Ok(finalizer),
                        None => Ok(Box::new(Idle::new())),
                    },
                    EndMode::Synthesize { model, instruction } => {
                        let model_ref = model.as_deref().unwrap_or(&agentic_config.model);
                        let openai_adapter = OpenAIAdapter::from_config(
                            execution_context.project.clone(),
                            model_ref,
                        )
                        .await?;
                        Ok(Box::new(Synthesize::new(
                            openai_adapter,
                            instruction.to_string(),
                            finalizer,
                        )))
                    }
                }
            }
            TriggerType::Query(query_config) => {
                self.build_query_trigger(
                    execution_context,
                    agentic_config,
                    query_config,
                    objective.unwrap_or(self.user_query().to_string()),
                )
                .await
            }
            TriggerType::Visualize(viz_config) => {
                self.build_viz_trigger(
                    execution_context,
                    agentic_config,
                    viz_config,
                    objective.unwrap_or(self.user_query().to_string()),
                )
                .await
            }
            TriggerType::Insight(insight_config) => {
                self.build_insight_trigger(
                    execution_context,
                    agentic_config,
                    insight_config,
                    objective.unwrap_or(self.user_query().to_string()),
                )
                .await
            }
            TriggerType::Subflow(subflow_config) => {
                self.build_subflow_trigger(
                    execution_context,
                    agentic_config,
                    subflow_config,
                    objective.unwrap_or(self.user_query().to_string()),
                )
                .await
            }
        }
    }
}

#[async_trait::async_trait]
impl<T> State for T
where
    T: TransitionContext + TriggerBuilder + std::fmt::Debug + Send + Sync + 'static,
{
    type Machine = Agent<Self>;

    async fn first_trigger(
        &mut self,
        execution_context: &ExecutionContext,
        machine: &mut Self::Machine,
    ) -> Result<Box<dyn Trigger<State = Self>>, OxyError> {
        let start_transition = machine.config.start_transition();
        self.set_transition_name(start_transition.trigger.get_name());
        self.build(execution_context, &machine.config, start_transition, None)
            .await
    }

    async fn next_trigger(
        &mut self,
        execution_context: &ExecutionContext,
        machine: &mut Self::Machine,
    ) -> Result<Option<Box<dyn Trigger<State = Self>>>, OxyError> {
        // Check if max iterations reached
        if self.max_iterations_reached(machine.config.max_iterations) {
            execution_context
                .write_chunk(Chunk {
                    key: None,
                    delta: Output::Text(format!(
                        "\n\n`max_iterations` of {} reached. Ending the agentic workflow.\n\n",
                        machine.config.max_iterations
                    )),
                    finished: true,
                })
                .await?;
            return Ok(None);
        }

        let current_transition = machine.config.find_transition(self.transition_name())?;
        if current_transition.trigger.is_end() {
            return Ok(None);
        }

        let mut objective = None;
        let mut get_transition = async || match &current_transition.next {
            TransitionMode::Always(next) => machine.config.find_transition(&next).map(Some),
            TransitionMode::Auto(items) => {
                if items.is_empty() {
                    return Ok(None);
                }

                if items.len() == 1 {
                    return machine.config.find_transition(&items[0]).map(Some);
                }

                let (transition, transition_objective) = machine
                    .select_transition(items, self.get_messages())
                    .await?;
                objective = Some(transition_objective);
                Ok(Some(transition))
            }
            TransitionMode::Plan => match &self.get_plan().is_some() {
                true => Ok(Some(machine.config.start_transition())),
                false => Ok(None),
            },
        };
        let transition = get_transition().await?;

        match transition {
            Some(t) => {
                self.set_transition_name(t.trigger.get_name());
                let mut transition_message =
                    format!("\n\nRunning transition: {}\n\n", t.trigger.get_name());
                if let Some(obj) = &objective {
                    transition_message.push_str(&format!("With objective: {obj}\n\n"));
                }
                execution_context
                    .write_chunk(Chunk {
                        key: None,
                        delta: Output::Text(transition_message),
                        finished: true,
                    })
                    .await?;

                // If the next transition is a start transition, we need to check if we should revise the plan
                if t.trigger.is_start() && self.get_plan().is_some() {
                    let should_revise = machine.should_revise_plan(self.get_messages()).await?;
                    if !should_revise {
                        return Ok(Some(Box::new(Idle::new())));
                    }
                }

                Ok(Some(
                    self.build(execution_context, &machine.config, t, objective)
                        .await?,
                ))
            }
            None => Ok(None),
        }
    }
}

pub struct Idle<S> {
    _state: PhantomData<S>,
}

impl<S> Idle<S> {
    pub fn new() -> Self {
        Self {
            _state: PhantomData,
        }
    }
}

#[async_trait::async_trait]
impl<S: Send + Sync> Trigger for Idle<S> {
    type State = S;

    async fn run(
        &self,
        _execution_context: &ExecutionContext,
        state: Self::State,
    ) -> Result<Self::State, OxyError> {
        Ok(state)
    }
}

pub struct Plan<S> {
    adapter: OpenAIAdapter,
    instruction: String,
    example: String,
    transitions: Vec<Transition>,
    _state: PhantomData<S>,
}

impl<S> Plan<S> {
    pub fn new(
        adapter: OpenAIAdapter,
        instruction: String,
        example: String,
        transitions: Vec<Transition>,
    ) -> Self {
        Self {
            adapter,
            instruction,
            example,
            transitions,
            _state: PhantomData,
        }
    }

    async fn prepare_messages(
        &self,
        execution_context: &ExecutionContext,
        messages: Vec<ChatCompletionRequestMessage>,
        revise_plan: bool,
    ) -> Result<Vec<ChatCompletionRequestMessage>, OxyError> {
        let instruction = execution_context
            .renderer
            .render_async(&self.instruction)
            .await?;
        let example = execution_context
            .renderer
            .render_async(&self.example)
            .await?;
        let available_actions = self
            .transitions
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.trigger.get_name(),
                    "description": t.trigger.get_description(),
                })
                .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        let mut messages = vec![
            vec![ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(format!(
                    "## Instruction \n{instruction}\n{example}### Available Actions:\n{available_actions}",
                )),
                ..Default::default()
            }
            .into()],
            messages,
        ]
        .concat();

        if revise_plan {
            messages.push(
                ChatCompletionRequestUserMessage {
                    content: ChatCompletionRequestUserMessageContent::Text(
                        "Based on the previous execution, please revise the plan if necessary."
                            .to_string(),
                    ),
                    ..Default::default()
                }
                .into(),
            );
        }
        Ok(messages)
    }
}

#[async_trait::async_trait]
impl<S> Trigger for Plan<S>
where
    S: TransitionContext + Send + Sync,
{
    type State = S;

    async fn run(&self, execution_context: &ExecutionContext, mut state: S) -> Result<S, OxyError> {
        let messages = self
            .prepare_messages(
                execution_context,
                state.get_messages(),
                state.get_plan().is_some(),
            )
            .await?;
        let mut stream = self.adapter.stream_text(messages).await?;
        let mut content = String::new();
        while let Some(chunk) = stream.next().await.transpose()?.flatten() {
            content.push_str(&chunk);
            execution_context
                .write_chunk(Chunk {
                    key: None,
                    delta: Output::Text(chunk),
                    finished: false,
                })
                .await?;
        }
        execution_context
            .write_chunk(Chunk {
                key: None,
                delta: Output::Text("".to_string()),
                finished: true,
            })
            .await?;
        state.set_plan(content);
        Ok(state)
    }
}

pub struct Synthesize<S> {
    adapter: OpenAIAdapter,
    instruction: String,
    finalizer: Option<Box<dyn Trigger<State = S>>>,
    _state: PhantomData<S>,
}

impl<S> Synthesize<S> {
    pub fn new(
        adapter: OpenAIAdapter,
        instruction: String,
        finalizer: Option<Box<dyn Trigger<State = S>>>,
    ) -> Self {
        Self {
            adapter,
            instruction,
            finalizer,
            _state: PhantomData,
        }
    }
}

#[async_trait::async_trait]
impl<S> Trigger for Synthesize<S>
where
    S: TransitionContext + Send + Sync,
{
    type State = S;

    async fn run(
        &self,
        execution_context: &ExecutionContext,
        mut current_state: Self::State,
    ) -> Result<Self::State, OxyError> {
        match &self.finalizer {
            Some(finalizer) => {
                current_state = finalizer.run(execution_context, current_state).await?;
            }
            None => {}
        }

        let instruction = execution_context
            .renderer
            .render_async(&self.instruction)
            .await?;
        let mut messages = vec![
            ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(instruction),
                ..Default::default()
            }
            .into(),
        ];
        messages.extend(current_state.get_messages());
        let mut stream = self.adapter.stream_text(messages).await?;
        let mut content = String::new();
        while let Some(chunk) = stream.next().await.transpose()?.flatten() {
            content.push_str(&chunk);
            execution_context
                .write_chunk(Chunk {
                    key: None,
                    delta: Output::Text(chunk),
                    finished: false,
                })
                .await?;
        }
        current_state.set_content(content);
        execution_context
            .write_chunk(Chunk {
                key: None,
                delta: Output::Text("".to_string()),
                finished: true,
            })
            .await?;
        Ok(current_state)
    }
}
