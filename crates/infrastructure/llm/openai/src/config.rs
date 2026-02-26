use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use std::collections::HashMap;

use oxy_shared::AzureModel;

// Re-export HeaderValue so existing code importing from oxy_openai still works
pub use oxy_shared::HeaderValue;

/// Default OpenAI API URL
pub const OPENAI_API_URL: &str = "https://api.openai.com/v1";

/// Returns the default OpenAI API URL for serde defaults
pub fn default_openai_api_url() -> Option<String> {
    Some(OPENAI_API_URL.to_string())
}

/// OpenAI model configuration
#[skip_serializing_none]
#[derive(Deserialize, Debug, Clone, Serialize, JsonSchema)]
pub struct OpenAIModelConfig {
    pub name: String,
    pub model_ref: String,
    pub key_var: String,
    #[serde(default = "default_openai_api_url")]
    pub api_url: Option<String>,
    #[serde(flatten)]
    pub azure: Option<AzureModel>,
    #[serde(default)]
    pub headers: Option<HashMap<String, HeaderValue>>,
}

impl OpenAIModelConfig {
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
