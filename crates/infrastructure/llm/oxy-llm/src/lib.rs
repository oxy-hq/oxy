//! LLM model configuration types for Oxy
//!
//! This crate provides the core data types for configuring LLM providers.
//! Model-specific configuration is defined in the respective provider crates
//! (oxy-openai, oxy-anthropic, oxy-gemini, oxy-ollama) and re-exported here.
//! Validation and secret resolution are handled separately in the core crate.

mod model;
mod traits;

// Re-export the unified Model enum and all provider config types
pub use model::{
    AnthropicModelConfig, AzureModel, GeminiModelConfig, HeaderValue, Model, OPENAI_API_URL,
    OllamaModelConfig, OpenAIModelConfig, default_openai_api_url,
};

// Re-export the trait
pub use traits::ModelConfig;

// Re-export Anthropic's default API URL function
pub use oxy_anthropic::default_api_url as default_anthropic_api_url;
