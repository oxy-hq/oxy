use async_openai::{
    Client,
    config::{AzureConfig, Config, OpenAIConfig},
    types::{
        ChatCompletionNamedToolChoice, ChatCompletionTool, ChatCompletionToolArgs,
        ChatCompletionToolType, FunctionName, FunctionObject, FunctionObjectArgs,
        ReasoningEffort as OpenAIReasoningEffort,
        responses::ReasoningConfig as OpenAIReasoningConfig,
    },
};
use axum::http::HeaderMap;
use schemars::schema::RootSchema;
use secrecy::SecretString;
use std::path::PathBuf;

use crate::{
    adapters::{create_app_schema, viz_schema},
    config::{
        constants::{ANTHROPIC_API_URL, GEMINI_API_URL},
        model::{Model, ReasoningConfig, ReasoningEffort, RetrievalConfig, ToolType},
    },
    errors::OxyError,
    execute::types::event::ArtifactKind,
    project::resolve_project_path,
    service::{secret_resolver::SecretResolverService, workflow::get_workflow},
    tools::types::{
        AgentParams, EmptySQLParams, ExecuteOmniParams, OmniTopicInfoParams, RetrievalParams,
        SQLParams,
    },
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

pub trait IntoOpenAIConfig {
    fn into_openai_config(
        &self,
    ) -> impl std::future::Future<Output = Result<ConfigType, OxyError>> + std::marker::Send;
}

impl IntoOpenAIConfig for Model {
    async fn into_openai_config(&self) -> Result<ConfigType, OxyError> {
        let secret_resolver = SecretResolverService::new();
        match self {
            Model::OpenAI {
                name: _,
                model_ref: _,
                api_url,
                azure,
                key_var,
            } => {
                let api_key = secret_resolver.resolve_secret(key_var).await.map_err(|_| {
                    OxyError::ConfigurationError("OpenAI key not found".to_string())
                })?;
                let api_key = match api_key {
                    Some(secret) => secret.value,
                    None => {
                        return Err(OxyError::ConfigurationError(
                            "OpenAI key not found".to_string(),
                        ));
                    }
                };

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
                let api_key = secret_resolver.resolve_secret(key_var).await.map_err(|e| {
                    OxyError::ConfigurationError("Gemini API key not found".to_string())
                })?;
                let api_key = match api_key {
                    Some(secret) => secret.value,
                    None => {
                        return Err(OxyError::ConfigurationError(
                            "Gemini API key not found".to_string(),
                        ));
                    }
                };
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
                let api_key = secret_resolver.resolve_secret(key_var).await.map_err(|e| {
                    OxyError::ConfigurationError("Anthropic API key not found".to_string())
                })?;
                let api_key = match api_key {
                    Some(secret) => secret.value,
                    None => {
                        return Err(OxyError::ConfigurationError(
                            "Gemini API key not found".to_string(),
                        ));
                    }
                };
                let config = OpenAIConfig::new()
                    .with_api_base(api_url.clone().unwrap_or(ANTHROPIC_API_URL.to_string()))
                    .with_api_key(api_key);
                Ok(ConfigType::Default(config))
            }
        }
    }
}

impl IntoOpenAIConfig for RetrievalConfig {
    async fn into_openai_config(&self) -> Result<ConfigType, OxyError> {
        let secret_resolver = SecretResolverService::new();
        let key_var = self.key_var.clone();
        let api_url = self.api_url.clone();
        let api_key = secret_resolver
            .resolve_secret(&key_var)
            .await
            .map_err(|e| {
                OxyError::ConfigurationError(format!("Retrieval API key not found: {e}"))
            })?;
        let api_key = match api_key {
            Some(secret) => secret.value,
            None => {
                return Err(OxyError::ConfigurationError(
                    "Retrieval API key not found".to_string(),
                ));
            }
        };
        Ok(ConfigType::Default(
            OpenAIConfig::new()
                .with_api_key(api_key)
                .with_api_base(api_url.to_string()),
        ))
    }
}

pub type OpenAIClient = Client<ConfigType>;

pub trait OpenAIToolConfig {
    fn description(&self) -> String;
    fn tool_kind(&self) -> String;
    fn handle(&self) -> String;
    fn artifact(&self) -> Option<(String, ArtifactKind)>;
    async fn params_schema(&self) -> Result<serde_json::Value, OxyError>;
}

impl OpenAIToolConfig for &ToolType {
    fn description(&self) -> String {
        match self {
            ToolType::ExecuteSQL(e) => e.description.clone(),
            ToolType::ValidateSQL(v) => v.description.clone(),
            ToolType::Retrieval(r) => r.description.clone(),
            ToolType::ExecuteOmni(execute_omni_tool) => {
                let model: Result<crate::config::model::OmniSemanticModel, OxyError> =
                    execute_omni_tool.load_semantic_model();
                match model {
                    Ok(model) => model.get_description(),
                    Err(e) => {
                        format!("Failed to load semantic model: {e}")
                    }
                }
            }
            ToolType::Workflow(w) => w.description.clone(),
            ToolType::Agent(agent_tool) => agent_tool.description.clone(),
            ToolType::Visualize(v) => v.description.clone(),
            ToolType::OmniTopicInfo(v) => v.get_description(),
            ToolType::CreateDataApp(v) => v.description.clone(),
        }
    }

    fn handle(&self) -> String {
        match self {
            ToolType::ExecuteSQL(e) => e.name.clone(),
            ToolType::ValidateSQL(v) => v.name.clone(),
            ToolType::Retrieval(r) => r.name.clone(),
            ToolType::ExecuteOmni(e) => e.name.clone(),
            ToolType::Workflow(w) => w.name.clone(),
            ToolType::Agent(agent_tool) => agent_tool.name.clone(),
            ToolType::Visualize(v) => v.name.clone(),
            ToolType::OmniTopicInfo(omni_topic_info_tool) => omni_topic_info_tool.name.clone(),
            ToolType::CreateDataApp(create_data_app_tool) => create_data_app_tool.name.clone(),
        }
    }

    fn artifact(&self) -> Option<(String, ArtifactKind)> {
        match self {
            ToolType::ExecuteSQL(sql) => Some((
                self.handle(),
                ArtifactKind::ExecuteSQL {
                    sql: sql.sql.clone().unwrap_or_default(),
                    database: sql.database.to_string(),
                },
            )),
            ToolType::Workflow(wf) => Some((
                self.handle(),
                ArtifactKind::Workflow {
                    r#ref: wf.workflow_ref.clone(),
                },
            )),
            ToolType::Agent(ag) => Some((
                self.handle(),
                ArtifactKind::Agent {
                    r#ref: ag.agent_ref.clone(),
                },
            )),
            _ => None,
        }
    }

    fn tool_kind(&self) -> String {
        match self {
            ToolType::ExecuteSQL(_) => "execute_sql".to_string(),
            ToolType::ValidateSQL(_) => "validate_sql".to_string(),
            ToolType::Retrieval(_) => "retrieval".to_string(),
            ToolType::Workflow(_) => "workflow".to_string(),
            ToolType::Agent(_) => "agent".to_string(),
            ToolType::Visualize(_) => "visualize".to_string(),
            ToolType::ExecuteOmni(_) => "execute_omni".to_string(),
            ToolType::OmniTopicInfo(_) => "omni_topic_info".to_string(),
            ToolType::CreateDataApp(_) => "create_data_app".to_string(),
        }
    }

    async fn params_schema(&self) -> Result<serde_json::Value, OxyError> {
        match self {
            ToolType::ExecuteSQL(sql_tool) => match sql_tool.sql {
                None => Ok(serde_json::json!(&schemars::schema_for!(SQLParams))),
                Some(_) => Ok(serde_json::json!(&schemars::schema_for!(EmptySQLParams))),
            },
            ToolType::ValidateSQL(_) => Ok(serde_json::json!(&schemars::schema_for!(SQLParams))),
            ToolType::Retrieval(_) => {
                Ok(serde_json::json!(&schemars::schema_for!(RetrievalParams)))
            }
            ToolType::Workflow(w) => generate_workflow_run_schema(&w.workflow_ref.clone()).await,
            ToolType::Agent(_) => Ok(serde_json::json!(&schemars::schema_for!(AgentParams))),
            ToolType::Visualize(_) => Ok(serde_json::from_str(viz_schema::VIZ_SCHEMA).unwrap()),
            ToolType::ExecuteOmni(_) => {
                Ok(serde_json::json!(&schemars::schema_for!(ExecuteOmniParams)))
            }
            ToolType::OmniTopicInfo(_) => Ok(serde_json::json!(&schemars::schema_for!(
                OmniTopicInfoParams
            ))),
            ToolType::CreateDataApp(_) => {
                // we need to manually create the schema for CreateDataAppParams
                // because this schema is quite complex and the library we use
                // schemars does not generate a compatiible schema with OpenAI.
                Ok(serde_json::from_str(create_app_schema::CREATE_APP_SCHEMA).unwrap())
            }
        }
    }
}

impl From<ToolType> for ChatCompletionNamedToolChoice {
    fn from(val: ToolType) -> Self {
        ChatCompletionNamedToolChoice {
            r#type: ChatCompletionToolType::Function,
            function: FunctionName {
                name: (&val).handle(),
            },
        }
    }
}

impl From<ReasoningEffort> for OpenAIReasoningEffort {
    fn from(effort: ReasoningEffort) -> Self {
        match effort {
            ReasoningEffort::Low => OpenAIReasoningEffort::Low,
            ReasoningEffort::Medium => OpenAIReasoningEffort::Medium,
            ReasoningEffort::High => OpenAIReasoningEffort::High,
        }
    }
}

impl From<ReasoningConfig> for OpenAIReasoningConfig {
    fn from(reasoning_config: ReasoningConfig) -> Self {
        OpenAIReasoningConfig {
            effort: Some(reasoning_config.effort.into()),
            ..Default::default()
        }
    }
}

async fn generate_workflow_run_schema(workflow_path: &str) -> Result<serde_json::Value, OxyError> {
    let project_path = resolve_project_path()?;
    let workflow_config =
        get_workflow(PathBuf::from(workflow_path), Some(project_path.clone())).await?;
    let schema = Into::<RootSchema>::into(&workflow_config.variables.unwrap_or_default());
    let json_schema = serde_json::json!(schema);
    Ok(json_schema)
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
