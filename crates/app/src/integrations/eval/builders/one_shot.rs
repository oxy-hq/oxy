//! Local OneShot LLM executable for the eval LLM-as-judge paths.
//!
//! Replaces the deleted `oxy_agent::agent::openai::{OneShotInput,
//! SimpleMapper, build_openai_executable}` with a minimal inline impl that
//! drives an LLM call via `oxy::adapters::openai::OpenAIAdapter` directly.
//! No tool calling, no streaming — just a single chat completion.

use async_openai::types::chat::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestSystemMessageContent, ChatCompletionRequestUserMessage,
    ChatCompletionRequestUserMessageContent,
};
use async_trait::async_trait;

use oxy::adapters::openai::{OpenAIAdapter, OpenAIClient};
use oxy::execute::{Executable, ExecutionContext, builders::map::ParamMapper, types::Output};
use oxy_shared::errors::OxyError;

#[derive(Clone, Debug)]
pub struct OneShotInput {
    pub system_instructions: String,
    pub user_input: Option<String>,
    pub memory: Vec<ChatCompletionRequestMessage>,
}

#[derive(Clone, Debug)]
pub struct OneShotOutput {
    pub content: Output,
}

#[derive(Clone, Debug)]
pub struct SimpleMapper;

#[async_trait]
impl ParamMapper<OneShotInput, OneShotInput> for SimpleMapper {
    async fn map(
        &self,
        _: &ExecutionContext,
        input: OneShotInput,
    ) -> Result<(OneShotInput, Option<ExecutionContext>), OxyError> {
        Ok((input, None))
    }
}

#[derive(Clone)]
pub struct OneShotExecutable {
    client: OpenAIClient,
    model_name: String,
}

#[async_trait]
impl Executable<OneShotInput> for OneShotExecutable {
    type Response = OneShotOutput;

    async fn execute(
        &mut self,
        _ctx: &ExecutionContext,
        input: OneShotInput,
    ) -> Result<Self::Response, OxyError> {
        let mut messages: Vec<ChatCompletionRequestMessage> = Vec::new();
        // System instructions first.
        messages.push(ChatCompletionRequestMessage::System(
            ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(input.system_instructions),
                name: None,
            },
        ));
        // Prior memory (typically empty for one-shot eval judges).
        messages.extend(input.memory);
        if let Some(user) = input.user_input {
            messages.push(ChatCompletionRequestMessage::User(
                ChatCompletionRequestUserMessage {
                    content: ChatCompletionRequestUserMessageContent::Text(user),
                    name: None,
                },
            ));
        }

        let adapter = OpenAIAdapter::new(self.client.clone(), self.model_name.clone());
        let text = adapter.generate_text(messages).await?;
        Ok(OneShotOutput {
            content: Output::Text(text),
        })
    }
}

/// Build a one-shot executable for the eval LLM-as-judge path. No tool
/// calling, no streaming — a single chat completion against `model_name`.
pub fn build_openai_executable(client: OpenAIClient, model_name: String) -> OneShotExecutable {
    OneShotExecutable { client, model_name }
}
