use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Ollama model configuration
#[derive(Deserialize, Debug, Clone, Serialize, JsonSchema)]
pub struct OllamaModelConfig {
    pub name: String,
    pub model_ref: String,
    pub api_key: String,
    pub api_url: String,
}

impl OllamaModelConfig {
    /// Get the user-defined name for this model configuration
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the underlying model name/reference used by the LLM provider
    pub fn model_name(&self) -> &str {
        &self.model_ref
    }

    /// Get the key variable name for API key resolution (Ollama doesn't use key_var)
    pub fn key_var(&self) -> Option<&str> {
        None
    }
}
