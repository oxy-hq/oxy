//! Local one-shot LLM judge for the eval LLM-as-judge path.
//!
//! A single chat completion against `model_name` via
//! `oxy::adapters::openai::OpenAIAdapter` — no tool calling, no streaming, and
//! no `oxy::execute` pipeline (`Executable`/`ParamMapper`). The solver drives it
//! directly with a plain async call.

use async_openai::types::chat::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestSystemMessageContent, ChatCompletionRequestUserMessage,
    ChatCompletionRequestUserMessageContent,
};

use oxy::adapters::openai::{OpenAIAdapter, OpenAIClient};
use oxy::execute::types::Output;
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

/// A one-shot LLM judge: a single chat completion, no tools, no streaming.
#[derive(Clone)]
pub struct OneShotJudge {
    client: OpenAIClient,
    model_name: String,
}

impl OneShotJudge {
    pub fn new(client: OpenAIClient, model_name: String) -> Self {
        Self { client, model_name }
    }

    pub async fn run(&self, input: OneShotInput) -> Result<OneShotOutput, OxyError> {
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
