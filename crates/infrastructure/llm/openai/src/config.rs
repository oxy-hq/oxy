use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use std::collections::HashMap;

use oxy_shared::AzureModel;

/// Default OpenAI API URL
pub const OPENAI_API_URL: &str = "https://api.openai.com/v1";

/// Returns the default OpenAI API URL for serde defaults
pub fn default_openai_api_url() -> Option<String> {
    Some(OPENAI_API_URL.to_string())
}

/// Header value that can be either a direct string or an environment variable reference
#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(untagged)]
pub enum HeaderValue {
    /// Direct header value
    Direct(String),
    /// Header value from environment variable
    EnvVar {
        /// Environment variable name containing the header value
        #[serde(rename = "env_var")]
        env_var: String,
    },
}

impl HeaderValue {
    /// Get the env var name if this is an EnvVar variant
    pub fn env_var_name(&self) -> Option<&str> {
        match self {
            HeaderValue::EnvVar { env_var } => Some(env_var),
            HeaderValue::Direct(_) => None,
        }
    }

    /// Get the direct value if this is a Direct variant
    pub fn direct_value(&self) -> Option<&str> {
        match self {
            HeaderValue::Direct(value) => Some(value),
            HeaderValue::EnvVar { .. } => None,
        }
    }
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
