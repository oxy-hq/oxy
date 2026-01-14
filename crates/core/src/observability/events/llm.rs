use async_openai::types::chat::ChatCompletionRequestMessage;
use tracing::{Level, event};

pub static NAME: &str = "llm.call";
pub static TYPE: &str = "llm";
pub static INPUT: &str = "llm.input";
pub static OUTPUT: &str = "llm.output";
pub static USAGE: &str = "llm.usage";

// Span names for tracing::instrument
pub const LLM_OPENAI_CALL: &str = "llm.openai.call";
pub const LLM_OSS_CALL: &str = "llm.oss.call";
pub const LLM_OPENAI_RESPONSE_CALL: &str = "llm.openai_response.call";
pub const LLM_TOOL_CALL: &str = "llm.tool.call";

// Span types for tracing::instrument
pub const LLM_CALL_TYPE: &str = "llm";

// Provider identifiers
pub const OPENAI: &str = "openai";

/// Log LLM token usage
pub fn usage(prompt_tokens: i64, completion_tokens: i64) {
    event!(
        Level::INFO,
        name = USAGE,
        is_visible = true,
        prompt_tokens = prompt_tokens,
        completion_tokens = completion_tokens,
        total_tokens = prompt_tokens + completion_tokens
    );
}

/// Log LLM input
pub fn input(messages: &Vec<ChatCompletionRequestMessage>) {
    event!(
        Level::INFO,
        name = INPUT,
        is_visible = true,
        messages = %serde_json::to_string(&messages).unwrap_or_default()
    );
}

/// Log LLM output
pub fn output<T: serde::Serialize>(result: &T) {
    event!(
        Level::INFO,
        name = OUTPUT,
        is_visible = true,
        status = "success",
        result = %serde_json::to_string(&result).unwrap_or_default()
    );
}
