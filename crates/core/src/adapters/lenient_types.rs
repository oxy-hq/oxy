//! Lenient response types for OpenAI-compatible APIs (Groq, Mistral, Together, etc.)
//!
//! These types are more permissive than async-openai's strict types to handle
//! variations in OpenAI-compatible API responses:
//! - Groq returns `service_tier: "on_demand"` (not in OpenAI's enum)
//! - Mistral omits the `type` field in tool_calls
//! - Other providers may have similar variations

use serde::{Deserialize, Serialize};

/// Lenient ServiceTier that accepts any string value
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum LenientServiceTier {
    Auto,
    #[default]
    Default,
    Flex,
    Scale,
    Priority,
    /// Catches unknown variants like Groq's "on_demand"
    #[serde(other)]
    Other,
}

/// Lenient tool call type that defaults to "function"
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum LenientToolCallType {
    #[default]
    Function,
    #[serde(other)]
    Other,
}

/// Lenient function call in a tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LenientFunctionCall {
    pub name: String,
    pub arguments: String,
}

/// Lenient tool call that handles missing `type` field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LenientToolCall {
    pub id: String,
    /// Defaults to "function" if missing (Mistral compatibility)
    #[serde(default)]
    pub r#type: LenientToolCallType,
    pub function: LenientFunctionCall,
    /// Some providers include index, some don't
    #[serde(default)]
    pub index: Option<u32>,
}

/// Lenient chat completion message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LenientChatCompletionMessage {
    pub role: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<LenientToolCall>>,
    #[serde(default)]
    pub refusal: Option<String>,
}

/// Lenient finish reason
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LenientFinishReason {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
    FunctionCall,
    #[default]
    #[serde(other)]
    Other,
}

/// Lenient chat completion choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LenientChatCompletionChoice {
    pub index: u32,
    pub message: LenientChatCompletionMessage,
    #[serde(default)]
    pub finish_reason: Option<LenientFinishReason>,
    #[serde(default)]
    pub logprobs: Option<serde_json::Value>,
}

/// Lenient completion usage
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LenientCompletionUsage {
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
    #[serde(default)]
    pub total_tokens: u32,
    /// Catch any additional usage fields providers might add
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

/// Lenient chat completion response that handles various provider quirks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LenientChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<LenientChatCompletionChoice>,
    #[serde(default)]
    pub usage: Option<LenientCompletionUsage>,
    #[serde(default)]
    pub system_fingerprint: Option<String>,
    /// Lenient service_tier handling (Groq compatibility)
    #[serde(default)]
    pub service_tier: Option<LenientServiceTier>,
    /// Catch any additional fields providers might add (like Groq's x_groq)
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

// Conversion to async-openai types
impl From<LenientToolCall> for async_openai::types::chat::ChatCompletionMessageToolCall {
    fn from(tc: LenientToolCall) -> Self {
        Self {
            id: tc.id,
            function: async_openai::types::chat::FunctionCall {
                name: tc.function.name,
                arguments: tc.function.arguments,
            },
        }
    }
}

impl LenientChatCompletionResponse {
    /// Extract tool calls from the response
    pub fn extract_tool_calls(
        &self,
    ) -> Option<(
        Option<String>,
        Vec<async_openai::types::chat::ChatCompletionMessageToolCall>,
    )> {
        self.choices.first().map(|choice| {
            let content = choice.message.content.clone();
            let tool_calls = choice
                .message
                .tool_calls
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect();
            (content, tool_calls)
        })
    }

    /// Extract text content from the response
    pub fn extract_content(&self) -> Option<String> {
        self.choices
            .first()
            .and_then(|choice| choice.message.content.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_groq_response_with_on_demand_service_tier() {
        // Groq returns service_tier: "on_demand" which is not a valid OpenAI ServiceTier variant
        // This test verifies it deserializes as LenientServiceTier::Other instead of failing
        let json = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1234567890,
            "model": "llama-3.3-70b-versatile",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "test_func",
                            "arguments": "{}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "service_tier": "on_demand"
        }"#;

        let response: LenientChatCompletionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, "chatcmpl-123");
        // Verify that the unknown "on_demand" value is captured as LenientServiceTier::Other
        assert_eq!(response.service_tier, Some(LenientServiceTier::Other));
    }

    #[test]
    fn test_mistral_response_missing_type() {
        // Mistral omits the "type" field in tool_calls (OpenAI requires "type": "function")
        // This test verifies it deserializes with a default type and converts correctly
        let json = r#"{
            "id": "abc123",
            "object": "chat.completion",
            "created": 1234567890,
            "model": "mistral-large-latest",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_456",
                        "function": {
                            "name": "query_data",
                            "arguments": "{\"x\": 1}"
                        }
                    }],
                    "content": ""
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 50,
                "total_tokens": 150
            }
        }"#;

        let response: LenientChatCompletionResponse = serde_json::from_str(json).unwrap();

        // Verify deserialization defaults the missing type to Function
        assert_eq!(
            response.choices[0].message.tool_calls.as_ref().unwrap()[0].r#type,
            LenientToolCallType::Function
        );

        // Verify extract_tool_calls properly converts to async_openai types
        let (content, tool_calls) = response.extract_tool_calls().unwrap();
        assert_eq!(content, Some("".to_string()));
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_456");
        assert_eq!(tool_calls[0].function.name, "query_data");
        assert_eq!(tool_calls[0].function.arguments, "{\"x\": 1}");
    }
}
