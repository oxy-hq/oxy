mod config;

use async_openai::config::OpenAIConfig;
use oxy_shared::{ConfigType, CustomOpenAIConfig};
use std::collections::HashMap;

// Export model configuration types
pub use config::{default_novita_api_url, NovitaModelConfig, NOVITA_API_URL};

/// Creates an OpenAI-compatible config for Novita API
///
/// # Arguments
/// * `api_key` - The Novita API key (already resolved from secrets)
/// * `api_url` - Optional custom API URL (defaults to NOVITA_API_URL)
/// * `custom_headers` - Optional map of resolved custom HTTP headers
///
/// # Returns
/// A `ConfigType` configured to use Novita's API endpoint
pub fn create_openai_config(
    api_key: impl Into<String>,
    api_url: Option<String>,
    custom_headers: Option<HashMap<String, String>>,
) -> ConfigType {
    let config = OpenAIConfig::new()
        .with_api_base(api_url.unwrap_or_else(|| NOVITA_API_URL.to_string()))
        .with_api_key(api_key.into());

    if let Some(headers) = custom_headers {
        let config_with_headers = CustomOpenAIConfig::new(config, headers);
        ConfigType::WithHeaders(config_with_headers)
    } else {
        ConfigType::Default(config)
    }
}
