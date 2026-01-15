// Shared message types for agent/chat communication
use async_openai::types::chat::{
    ChatCompletionRequestMessage, ChatCompletionRequestUserMessage,
    ChatCompletionRequestUserMessageContent,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn new(role: String, content: String) -> Self {
        Self { role, content }
    }

    pub fn user(content: String) -> Self {
        Self {
            role: "user".to_string(),
            content,
        }
    }

    pub fn assistant(content: String) -> Self {
        Self {
            role: "assistant".to_string(),
            content,
        }
    }

    pub fn system(content: String) -> Self {
        Self {
            role: "system".to_string(),
            content,
        }
    }
}

impl From<Message> for ChatCompletionRequestMessage {
    fn from(msg: Message) -> Self {
        match msg.role.as_str() {
            "user" => ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
                content: ChatCompletionRequestUserMessageContent::Text(msg.content),
                name: None,
            }),
            _ => ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
                content: ChatCompletionRequestUserMessageContent::Text(msg.content),
                name: None,
            }),
        }
    }
}
