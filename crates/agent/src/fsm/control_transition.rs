use std::marker::PhantomData;

use async_openai::types::chat::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestSystemMessageContent, ChatCompletionRequestUserMessage,
    ChatCompletionRequestUserMessageContent,
};
use tokio_stream::StreamExt;

/// Appends a sentinel user message if the message list does not already end with a user message.
///
/// Anthropic's API requires the conversation to end with a user message. This helper guards
/// every call site that builds a message list from accumulated state (which may end with an
/// assistant, planning, or system message) before forwarding to the LLM.
///
/// # When NOT to use this
///
/// Do **not** call this on tool-use retry loops where the list intentionally ends with a
/// `role: "tool"` (tool result) message. Anthropic's OpenAI-compat layer translates `role:
/// "tool"` → a `user` turn containing a `tool_result` block, so the conversation is already
/// user-terminated in Anthropic's model. Injecting an additional free-form user message after a
/// tool result would corrupt the tool-use conversation structure.
pub fn ensure_ends_with_user_message(
    messages: &mut Vec<ChatCompletionRequestMessage>,
    sentinel: &str,
) {
    if !matches!(messages.last(), Some(ChatCompletionRequestMessage::User(_))) {
        messages.push(
            ChatCompletionRequestUserMessage {
                content: ChatCompletionRequestUserMessageContent::Text(sentinel.to_string()),
                ..Default::default()
            }
            .into(),
        );
    }
}

use crate::fsm::{
    config::{AgenticConfig, Transition},
    data_app::config::Insight,
    query::config::Query,
    state::MachineContext,
    viz::config::Visualize,
};
use oxy::adapters::openai::OpenAIAdapter;
use oxy::execute::{
    ExecutionContext,
    builders::fsm::Trigger,
    types::{Chunk, Output},
};
use oxy_shared::errors::OxyError;

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
        _subflow_config: &crate::fsm::subflow_config::Subflow,
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
        objective: String,
    ) -> Result<Box<dyn Trigger<State = Self>>, OxyError>;
}

pub struct Idle<S> {
    _state: PhantomData<S>,
}

impl<S> Default for Idle<S> {
    fn default() -> Self {
        Self::new()
    }
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
        _state: &mut Self::State,
    ) -> Result<(), OxyError> {
        Ok(())
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
        _revise_plan: bool,
    ) -> Result<Vec<ChatCompletionRequestMessage>, OxyError> {
        let instruction = execution_context
            .renderer
            .render_async(&self.instruction)
            .await
            .ok()
            .unwrap_or(self.instruction.clone());
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
        let mut messages = [
            vec![
                ChatCompletionRequestSystemMessage {
                    content: ChatCompletionRequestSystemMessageContent::Text(format!(
                        "## Instruction
{instruction}

{example}

## Available Actions
You have access to these specialized agents:
{available_actions}

## Planning Guidelines
Create a clear, actionable plan by:
1. Breaking down the goal into specific steps
2. Sequencing steps logically (what must happen first?)
3. Assigning each step to the appropriate action from the list above
4. Being concrete - avoid vague steps like 'analyze data', instead specify what to analyze and why

Your plan should be a numbered list where each item describes:
- What specific task needs to be done
- Why it's necessary for achieving the goal
- Which action will handle it (if known)

Think through dependencies and order carefully - this plan guides the multi-agent workflow.",
                    )),
                    ..Default::default()
                }
                .into(),
            ],
            messages,
        ]
        .concat();
        // Claude requires the last message to be a user message (no assistant prefill).
        // Also handles the empty-history case where messages is [system_msg] only.
        ensure_ends_with_user_message(&mut messages, "Please create a plan for the above.");
        Ok(messages)
    }
}

#[async_trait::async_trait]
impl Trigger for Plan<MachineContext> {
    type State = MachineContext;

    async fn run(
        &self,
        execution_context: &ExecutionContext,
        state: &mut Self::State,
    ) -> Result<(), OxyError> {
        let messages = self
            .prepare_messages(
                execution_context,
                state
                    .list_messages()
                    .iter()
                    .map(|m| m.clone().into())
                    .collect(),
                state.get_plan().is_some(),
            )
            .await?;
        let mut stream = self.adapter.stream_text(messages).await?;
        let mut content = String::new();
        let streaming_context = execution_context
            .with_child_source(format!("plan_{}", uuid::Uuid::new_v4()), "text".to_string());
        while let Some(chunk) = stream.next().await.transpose()?.flatten() {
            content.push_str(&chunk);
            streaming_context
                .write_chunk(Chunk {
                    key: None,
                    delta: Output::Text(chunk),
                    finished: false,
                })
                .await?;
        }
        streaming_context
            .write_chunk(Chunk {
                key: None,
                delta: Output::Text("".to_string()),
                finished: true,
            })
            .await?;
        state.plan(&content);
        state.set_plan(Some(content));
        Ok(())
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
impl Trigger for Synthesize<MachineContext> {
    type State = MachineContext;

    async fn run(
        &self,
        execution_context: &ExecutionContext,
        current_state: &mut Self::State,
    ) -> Result<(), OxyError> {
        if let Some(finalizer) = &self.finalizer {
            finalizer.run(execution_context, current_state).await?;
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
        messages.extend(
            current_state
                .list_messages()
                .iter()
                .map(|m| m.clone().into()),
        );
        // Claude requires the last message to be a user message (no assistant prefill).
        ensure_ends_with_user_message(&mut messages, "Please synthesize the results above.");
        let mut stream = self.adapter.stream_text(messages).await?;
        let mut content = String::new();
        let streaming_context = execution_context
            .with_child_source(uuid::Uuid::new_v4().to_string(), "text".to_string());
        while let Some(chunk) = stream.next().await.transpose()?.flatten() {
            content.push_str(&chunk);
            streaming_context
                .write_chunk(Chunk {
                    key: None,
                    delta: Output::Text(chunk),
                    finished: false,
                })
                .await?;
        }
        current_state.set_content(Some(content));
        streaming_context
            .write_chunk(Chunk {
                key: None,
                delta: Output::Text("".to_string()),
                finished: true,
            })
            .await?;
        Ok(())
    }
}
