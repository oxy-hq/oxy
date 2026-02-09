//! Mock implementations for LLM API services.
//!
//! Provides wiremock-based mocks for OpenAI-compatible APIs.

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Creates a mock OpenAI chat completions endpoint.
///
/// # Arguments
/// * `mock_server` - The wiremock MockServer to mount the mock on
/// * `response_content` - The content to return in the assistant message
///
/// # Example
/// ```ignore
/// let mock_server = MockServer::start().await;
/// mock_openai_chat_completion(&mock_server, "Hello! How can I help?").await;
/// ```
pub async fn mock_openai_chat_completion(mock_server: &MockServer, response_content: &str) {
    let response_body = json!({
        "id": "chatcmpl-test-123",
        "object": "chat.completion",
        "created": 1234567890,
        "model": "gpt-4",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": response_content
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 20,
            "total_tokens": 30
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
        .mount(mock_server)
        .await;
}

/// Creates a mock OpenAI embeddings endpoint.
///
/// # Arguments
/// * `mock_server` - The wiremock MockServer to mount the mock on
/// * `embedding_dimension` - The dimension of the embedding vectors to return
pub async fn mock_openai_embeddings(mock_server: &MockServer, embedding_dimension: usize) {
    let embedding: Vec<f32> = (0..embedding_dimension).map(|i| i as f32 * 0.01).collect();

    let response_body = json!({
        "object": "list",
        "data": [{
            "object": "embedding",
            "embedding": embedding,
            "index": 0
        }],
        "model": "text-embedding-ada-002",
        "usage": {
            "prompt_tokens": 5,
            "total_tokens": 5
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
        .mount(mock_server)
        .await;
}

/// Creates a mock that returns an API error response.
///
/// Useful for testing error handling paths.
pub async fn mock_openai_error(mock_server: &MockServer, status_code: u16, error_message: &str) {
    let response_body = json!({
        "error": {
            "message": error_message,
            "type": "api_error",
            "code": "api_error"
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(status_code).set_body_json(response_body))
        .mount(mock_server)
        .await;
}

/// Creates a mock for Anthropic Claude API.
pub async fn mock_anthropic_messages(mock_server: &MockServer, response_content: &str) {
    let response_body = json!({
        "id": "msg_test_123",
        "type": "message",
        "role": "assistant",
        "content": [{
            "type": "text",
            "text": response_content
        }],
        "model": "claude-3-sonnet-20240229",
        "stop_reason": "end_turn",
        "usage": {
            "input_tokens": 10,
            "output_tokens": 20
        }
    });

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
        .mount(mock_server)
        .await;
}
