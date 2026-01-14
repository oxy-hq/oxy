use tracing::{Level, event};

pub static NAME: &str = "tool.execute";
pub static TYPE: &str = "tool";
pub static INPUT: &str = "tool.input";
pub static OUTPUT: &str = "tool.output";

// Span names and types for tracing::instrument
pub const TOOL_EXECUTE: &str = "tool.execute";
pub const TOOL_TYPE: &str = "tool";

// Tool call span names and types (for individual tool calls)
pub const TOOL_CALL_EXECUTE: &str = "tool_call.execute";
pub const TOOL_CALL_TYPE: &str = "tool_call";
pub const TOOL_CALL_INPUT: &str = "tool_call.input";
pub const TOOL_CALL_OUTPUT: &str = "tool_call.output";

/// Log tool execution input
pub fn input<T: serde::Serialize>(input: &T) {
    event!(
        Level::INFO,
        name = INPUT,
        is_visible = true,
        input = %serde_json::to_string(&input).unwrap_or_default()
    );
}

/// Log tool execution output
pub fn output<T: serde::Serialize>(output: &T) {
    event!(
        Level::INFO,
        name = OUTPUT,
        is_visible = true,
        status = "success",
        output = %serde_json::to_string(&output).unwrap_or_default()
    );
}

/// Log individual tool call input
pub fn tool_call_input<T: serde::Serialize>(input: &T) {
    event!(
        Level::INFO,
        name = TOOL_CALL_INPUT,
        is_visible = true,
        input = %serde_json::to_string(&input).unwrap_or_default()
    );
}

/// Log whether tool call is verified
pub fn tool_call_is_verified(is_verified: bool) {
    event!(
        Level::INFO,
        name = "tool_call.is_verified",
        is_visible = true,
        is_verified = is_verified
    );
}

/// Log individual tool call output
pub fn tool_call_output<T: serde::Serialize>(output: &T) {
    event!(
        Level::INFO,
        name = TOOL_CALL_OUTPUT,
        is_visible = true,
        status = "success",
        output = %serde_json::to_string(&output).unwrap_or_default()
    );
}

/// Log individual tool call error
pub fn tool_call_error(error: &str) {
    event!(
        Level::ERROR,
        name = TOOL_CALL_OUTPUT,
        is_visible = true,
        status = "error",
        error = %error
    );
}
