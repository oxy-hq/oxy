use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Google Gemini model configuration
#[derive(Deserialize, Debug, Clone, Serialize, JsonSchema)]
pub struct GeminiModelConfig {
    pub name: String,
    pub model_ref: String,
    pub key_var: String,
}

impl GeminiModelConfig {
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
}
