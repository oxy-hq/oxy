/// Common trait for all LLM model configurations
pub trait ModelConfig {
    /// Get the user-defined name for this model configuration
    fn name(&self) -> &str;

    /// Get the underlying model name/reference used by the LLM provider
    fn model_name(&self) -> &str;

    /// Get the key variable name for API key resolution (if applicable)
    fn key_var(&self) -> Option<&str>;
}
