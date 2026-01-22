mod config;

use async_openai::config::OpenAIConfig;
use oxy_shared::ConfigType;

// Export model configuration types
pub use config::AnthropicModelConfig;

/// The default Anthropic API URL
pub const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1";

/// Returns the default Anthropic API URL wrapped in Option for serde defaults
///
/// # Returns
/// `Some(ANTHROPIC_API_URL)` for use in serde default attributes
pub fn default_api_url() -> Option<String> {
    Some(ANTHROPIC_API_URL.to_string())
}

/// Creates an OpenAI-compatible config for Anthropic API
///
/// # Arguments
/// * `api_key` - The Anthropic API key (already resolved from secrets)
/// * `api_url` - Optional custom API URL (defaults to ANTHROPIC_API_URL)
///
/// # Returns
/// A `ConfigType` configured to use Anthropic's API endpoint
pub fn create_openai_config(api_key: impl Into<String>, api_url: Option<String>) -> ConfigType {
    let config = OpenAIConfig::new()
        .with_api_base(api_url.unwrap_or_else(|| ANTHROPIC_API_URL.to_string()))
        .with_api_key(api_key.into());
    ConfigType::Default(config)
}
