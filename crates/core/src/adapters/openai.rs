use async_openai::{
    Client,
    config::{AzureConfig, Config, OpenAIConfig},
    types::{ChatCompletionTool, ChatCompletionToolArgs, FunctionObject, FunctionObjectArgs},
};
use reqwest::header::HeaderMap;
use secrecy::SecretString;

use crate::{
    config::{
        constants::{ANTHROPIC_API_URL, GEMINI_API_URL},
        model::{Model, RetrievalConfig, ToolType},
    },
    errors::OxyError,
    tools::types::{RetrievalParams, SQLParams},
    tools::visualize::types::VisualizeParams,
};

#[derive(Debug, Clone)]
pub enum ConfigType {
    Default(OpenAIConfig),
    Azure(AzureConfig),
}

/// This is a wrapper around OpenAIConfig and AzureConfig
/// to allow for dynamic configuration of the client
/// based on the model configuration
impl Config for ConfigType {
    fn headers(&self) -> HeaderMap {
        match &self {
            ConfigType::Default(config) => config.headers(),
            ConfigType::Azure(config) => config.headers(),
        }
    }
    fn url(&self, path: &str) -> String {
        match &self {
            ConfigType::Default(config) => config.url(path),
            ConfigType::Azure(config) => config.url(path),
        }
    }
    fn query(&self) -> Vec<(&str, &str)> {
        match &self {
            ConfigType::Default(config) => config.query(),
            ConfigType::Azure(config) => config.query(),
        }
    }

    fn api_base(&self) -> &str {
        match &self {
            ConfigType::Default(config) => config.api_base(),
            ConfigType::Azure(config) => config.api_base(),
        }
    }

    fn api_key(&self) -> &SecretString {
        match &self {
            ConfigType::Default(config) => config.api_key(),
            ConfigType::Azure(config) => config.api_key(),
        }
    }
}

impl TryFrom<&Model> for ConfigType {
    type Error = OxyError;

    fn try_from(model: &Model) -> Result<Self, Self::Error> {
        match model {
            Model::OpenAI {
                name: _,
                model_ref: _,
                api_url,
                azure,
                key_var,
            } => {
                let api_key = std::env::var(key_var).map_err(|e| {
                    OxyError::ConfigurationError(format!(
                        "OpenAI key not found in environment variable {}:\n{}",
                        key_var, e
                    ))
                })?;

                match azure {
                    Some(azure) => {
                        let mut config = AzureConfig::new()
                            .with_api_version(&azure.azure_api_version)
                            .with_deployment_id(&azure.azure_deployment_id)
                            .with_api_key(key_var.clone());
                        if let Some(api_url) = api_url {
                            config = config.with_api_base(api_url);
                        }
                        Ok(ConfigType::Azure(config))
                    }
                    None => {
                        let mut config = OpenAIConfig::new().with_api_key(api_key);
                        if let Some(api_url) = api_url {
                            config = config.with_api_base(api_url);
                        }
                        Ok(ConfigType::Default(config))
                    }
                }
            }
            Model::Ollama {
                name: _,
                model_ref: _,
                api_key,
                api_url,
            } => {
                let config = OpenAIConfig::new()
                    .with_api_base(api_url)
                    .with_api_key(api_key);
                Ok(ConfigType::Default(config))
            }
            Model::Google {
                name: _,
                model_ref: _,
                key_var,
            } => {
                let api_key = std::env::var(key_var).map_err(|e| {
                    OxyError::ConfigurationError(format!(
                        "Gemini API key not found in environment variable {}:\n{}",
                        key_var, e
                    ))
                })?;
                let config = OpenAIConfig::new()
                    .with_api_base(GEMINI_API_URL)
                    .with_api_key(api_key);
                Ok(ConfigType::Default(config))
            }
            Model::Anthropic {
                name: _,
                model_ref: _,
                key_var,
                api_url,
            } => {
                let api_key = std::env::var(key_var).map_err(|e| {
                    OxyError::ConfigurationError(format!(
                        "Anthropic API key not found in environment variable {}:\n{}",
                        key_var, e
                    ))
                })?;
                let config = OpenAIConfig::new()
                    .with_api_base(api_url.clone().unwrap_or(ANTHROPIC_API_URL.to_string()))
                    .with_api_key(api_key);
                Ok(ConfigType::Default(config))
            }
        }
    }
}

impl TryFrom<&RetrievalConfig> for ConfigType {
    type Error = OxyError;

    fn try_from(retrieval: &RetrievalConfig) -> Result<Self, Self::Error> {
        let api_key = match &retrieval.api_key {
            Some(key) => key,
            None => &std::env::var(&retrieval.key_var).map_err(|e| {
                OxyError::ConfigurationError(format!(
                    "OpenAI key not found in environment variable {}:\n{}",
                    retrieval.key_var, e
                ))
            })?,
        };
        Ok(ConfigType::Default(
            OpenAIConfig::new()
                .with_api_key(api_key)
                .with_api_base(retrieval.api_url.to_string()),
        ))
    }
}

pub type OpenAIClient = Client<ConfigType>;

pub trait OpenAIToolConfig {
    fn description(&self) -> String;
    fn tool_kind(&self) -> String;
    fn handle(&self) -> String;
    fn params_schema(&self) -> serde_json::Value;
}

impl OpenAIToolConfig for &ToolType {
    fn description(&self) -> String {
        match self {
            ToolType::ExecuteSQL(e) => e.description.clone(),
            ToolType::ValidateSQL(v) => v.description.clone(),
            ToolType::Retrieval(r) => r.description.clone(),
            ToolType::Visualize(v) => v.description.clone(),
        }
    }

    fn handle(&self) -> String {
        match self {
            ToolType::ExecuteSQL(e) => e.name.clone(),
            ToolType::ValidateSQL(v) => v.name.clone(),
            ToolType::Retrieval(r) => r.name.clone(),
            ToolType::Visualize(v) => v.name.clone(),
        }
    }

    fn tool_kind(&self) -> String {
        match self {
            ToolType::ExecuteSQL(_) => "execute_sql".to_string(),
            ToolType::ValidateSQL(_) => "validate_sql".to_string(),
            ToolType::Retrieval(_) => "retrieval".to_string(),
            ToolType::Visualize(_) => "visualize".to_string(),
        }
    }

    fn params_schema(&self) -> serde_json::Value {
        match self {
            ToolType::ExecuteSQL(_) => serde_json::json!(&schemars::schema_for!(SQLParams)),
            ToolType::ValidateSQL(_) => serde_json::json!(&schemars::schema_for!(SQLParams)),
            ToolType::Retrieval(_) => serde_json::json!(&schemars::schema_for!(RetrievalParams)),
            ToolType::Visualize(_) => serde_json::json!(&schemars::schema_for!(VisualizeParams)),
        }
    }
}

impl From<&ToolType> for FunctionObject {
    fn from(tool: &ToolType) -> Self {
        FunctionObjectArgs::default()
            .name(tool.handle())
            .description(tool.description())
            .parameters(tool.params_schema())
            .build()
            .unwrap()
    }
}

impl From<&ToolType> for ChatCompletionTool {
    fn from(tool: &ToolType) -> Self {
        ChatCompletionToolArgs::default()
            .function::<FunctionObject>(tool.into())
            .build()
            .unwrap()
    }
}
