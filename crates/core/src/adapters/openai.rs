use async_openai::{
    Client,
    config::{AzureConfig, Config, OpenAIConfig},
    types::{ChatCompletionTool, ChatCompletionToolArgs, FunctionObject, FunctionObjectArgs},
};
use axum::http::HeaderMap;
use secrecy::SecretString;
use serde_json::{Value, json};
use std::path::PathBuf;

use crate::{
    config::{
        constants::{ANTHROPIC_API_URL, GEMINI_API_URL},
        model::{Model, RetrievalConfig, ToolType},
    },
    errors::OxyError,
    service::workflow::get_workflow,
    tools::{
        types::{RetrievalParams, SQLParams},
        visualize::types::VisualizeParams,
    },
    utils::find_project_path,
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
                            .with_api_key(api_key);
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
    async fn params_schema(&self) -> Result<serde_json::Value, OxyError>;
}

impl OpenAIToolConfig for &ToolType {
    fn description(&self) -> String {
        match self {
            ToolType::ExecuteSQL(e) => e.description.clone(),
            ToolType::ValidateSQL(v) => v.description.clone(),
            ToolType::Retrieval(r) => r.description.clone(),
            ToolType::Workflow(w) => w.description.clone(),
            ToolType::Visualize(v) => v.description.clone(),
        }
    }

    fn handle(&self) -> String {
        match self {
            ToolType::ExecuteSQL(e) => e.name.clone(),
            ToolType::ValidateSQL(v) => v.name.clone(),
            ToolType::Retrieval(r) => r.name.clone(),
            ToolType::Workflow(w) => w.name.clone(),
            ToolType::Visualize(v) => v.name.clone(),
        }
    }

    fn tool_kind(&self) -> String {
        match self {
            ToolType::ExecuteSQL(_) => "execute_sql".to_string(),
            ToolType::ValidateSQL(_) => "validate_sql".to_string(),
            ToolType::Retrieval(_) => "retrieval".to_string(),
            ToolType::Workflow(_) => "workflow".to_string(),
            ToolType::Visualize(_) => "visualize".to_string(),
        }
    }

    async fn params_schema(&self) -> Result<serde_json::Value, OxyError> {
        match self {
            ToolType::ExecuteSQL(_) => Ok(serde_json::json!(&schemars::schema_for!(SQLParams))),
            ToolType::ValidateSQL(_) => Ok(serde_json::json!(&schemars::schema_for!(SQLParams))),
            ToolType::Retrieval(_) => {
                Ok(serde_json::json!(&schemars::schema_for!(RetrievalParams)))
            }
            ToolType::Workflow(w) => {
                let schema = generate_workflow_run_schema(&w.workflow_ref.clone())
                    .await
                    .unwrap();
                Ok(serde_json::json!(schema))
            }
            ToolType::Visualize(_) => {
                Ok(serde_json::json!(&schemars::schema_for!(VisualizeParams)))
            }
        }
    }
}

async fn generate_workflow_run_schema(
    workflow_path: &str,
) -> Result<serde_json::Map<String, Value>, OxyError> {
    let project_path = find_project_path().unwrap();
    let workflow_config =
        get_workflow(PathBuf::from(workflow_path), Some(project_path.clone())).await?;

    let variables = workflow_config.variables;

    if variables.is_none() {
        let mut schema = serde_json::Map::new();
        schema.insert("type".to_string(), Value::String("object".to_string()));

        return Ok(schema);
    }
    let mut schema = serde_json::Map::new();
    let mut variable_schema = serde_json::Map::new();
    let mut properties = serde_json::Map::new();
    let variables = variables.unwrap();

    for (key, _) in variables.iter() {
        properties.insert(
            key.clone(),
            json!(
                {
                    "type": "string",
                }
            ),
        );
    }
    variable_schema.insert("type".to_string(), Value::String("object".to_string()));
    variable_schema.insert("properties".to_string(), Value::Object(properties));

    schema.insert(
        "properties".to_string(),
        json!({
            "variables": variable_schema,
        }),
    );
    schema.insert("type".to_string(), Value::String("object".to_string()));

    Ok(schema)
}

pub trait AsyncFunctionObject {
    async fn from_tool_async(tool: &ToolType) -> Self;
}

impl AsyncFunctionObject for FunctionObject {
    async fn from_tool_async(tool: &ToolType) -> Self {
        let mut binding = FunctionObjectArgs::default();
        let mut function_args = binding.name(tool.handle()).description(tool.description());
        let params_schema = tool.params_schema().await.unwrap();
        if !params_schema.is_null()
            && params_schema.is_object()
            && params_schema
                .as_object()
                .unwrap()
                .get("properties")
                .is_some()
        {
            function_args = function_args.parameters(params_schema);
        }

        function_args.build().unwrap()
    }
}

impl AsyncFunctionObject for ChatCompletionTool {
    async fn from_tool_async(tool: &ToolType) -> Self {
        ChatCompletionToolArgs::default()
            .function::<FunctionObject>(FunctionObject::from_tool_async(tool).await)
            .build()
            .unwrap()
    }
}
