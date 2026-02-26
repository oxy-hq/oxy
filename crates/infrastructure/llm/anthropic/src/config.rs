use oxy_shared::HeaderValue;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use std::collections::HashMap;

/// Anthropic model configuration
#[skip_serializing_none]
#[derive(Deserialize, Debug, Clone, Serialize, JsonSchema)]
pub struct AnthropicModelConfig {
    pub name: String,
    pub model_ref: String,
    pub key_var: String,
    #[serde(default = "super::default_api_url")]
    pub api_url: Option<String>,
    #[serde(default)]
    pub headers: Option<HashMap<String, HeaderValue>>,
}

impl AnthropicModelConfig {
    /// Get the user-defined name for this model configuration
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the underlying model name/reference used by the LLM provider
    pub fn model_name(&self) -> &str {
        &self.model_ref
    }

    /// Get the key variable name for API key resolution
    pub fn key_var(&self) -> Option<&str> {
        Some(&self.key_var)
    }

    /// Get the custom headers (if any)
    pub fn headers(&self) -> Option<&HashMap<String, HeaderValue>> {
        self.headers.as_ref()
    }
}
