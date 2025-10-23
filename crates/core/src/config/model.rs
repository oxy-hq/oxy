use crate::service::types::SemanticQueryParams;
use garde::Validate;
use indoc::indoc;
use itertools::Itertools;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::skip_serializing_none;
use std::cmp::PartialEq;
use std::collections::HashMap;
use std::hash::Hash;
use std::path::PathBuf;

pub use variables::Variables;

use super::validate::{AgentValidationContext, validate_model, validate_task};
use crate::adapters::secrets::SecretsManager;
use crate::config::validate::validate_optional_private_key_path;
use crate::config::validate::{
    ValidationContext, validate_agent_exists, validate_database_exists, validate_env_var,
    validate_omni_integration_exists, validate_task_data_reference,
};
use crate::errors::OxyError;
pub use semantics::{SemanticDimension, Semantics};
pub use variables::Variable;
pub use workflow::WorkflowWithRawVariables;

mod semantics;
mod variables;
mod workflow;

#[derive(Serialize, Deserialize, Validate, Debug, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct Config {
    #[garde(dive)]
    pub defaults: Option<Defaults>,
    #[garde(dive)]
    pub models: Vec<Model>,
    #[garde(dive)]
    pub databases: Vec<Database>,
    #[garde(skip)]
    pub builder_agent: Option<PathBuf>,

    #[serde(skip)]
    #[garde(skip)]
    #[schemars(skip)]
    pub project_path: PathBuf,

    #[serde(default)]
    #[garde(skip)]
    pub integrations: Vec<Integration>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Validate)]
#[garde(context(ValidationContext))]
pub struct Integration {
    #[garde(skip)]
    pub name: String,
    #[serde(flatten)]
    #[garde(skip)]
    pub integration_type: IntegrationType,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum IntegrationType {
    #[serde(rename = "omni")]
    Omni(OmniIntegration),
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Validate)]
#[garde(context(ValidationContext))]
pub struct OmniIntegration {
    #[garde(custom(validate_env_var))]
    pub api_key_var: String,
    #[garde(length(min = 1))]
    pub base_url: String,
    #[garde(dive)]
    pub topics: Vec<OmniTopic>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Validate)]
#[garde(context(ValidationContext))]
pub struct OmniTopic {
    #[garde(length(min = 1))]
    pub name: String,
    #[garde(length(min = 1))]
    pub model_id: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, JsonSchema)]
pub struct SemanticModels {
    pub table: String,
    pub database: String,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub description: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub entities: Vec<Entity>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub dimensions: Vec<Dimension>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub measures: Vec<Measure>,
}

#[derive(Clone, Serialize, Deserialize, Debug, JsonSchema)]
pub struct Entity {
    pub name: String,
    pub description: String,
    pub sample: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug, JsonSchema)]
pub struct Dimension {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synonyms: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub sample: Vec<String>,
    #[serde(rename = "type", alias = "type")]
    pub data_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_partition_key: Option<bool>,
}

#[derive(Clone, Serialize, Deserialize, Debug, JsonSchema)]
pub struct Measure {
    pub name: String,
    pub sql: String,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Validate, Default)]
#[garde(context(ValidationContext))]
pub struct Postgres {
    #[serde(default)]
    #[garde(skip)]
    pub host: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub host_var: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub port: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub port_var: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub user: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub user_var: Option<String>,
    #[garde(skip)]
    #[schemars(skip)]
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub password_var: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub database: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub database_var: Option<String>,
}

impl Postgres {
    pub async fn get_password(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        if let Some(password) = &self.password
            && !password.is_empty()
        {
            return Ok(password.clone());
        }
        let value = secret_manager
            .resolve_secret(self.password_var.as_deref().unwrap_or(""))
            .await?;
        match value {
            Some(res) => Ok(res),
            None => Err(OxyError::SecretNotFound(self.password_var.clone())),
        }
    }

    pub async fn get_host(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        if let Some(host) = &self.host
            && !host.is_empty()
        {
            return Ok(host.clone());
        }
        if let Some(host_var) = &self.host_var {
            let value = secret_manager.resolve_secret(host_var).await?;
            match value {
                Some(res) => Ok(res),
                None => Err(OxyError::SecretNotFound(Some(host_var.clone()))),
            }
        } else {
            Ok("localhost".to_string())
        }
    }

    pub async fn get_port(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        if let Some(port) = &self.port
            && !port.is_empty()
        {
            return Ok(port.clone());
        }
        if let Some(port_var) = &self.port_var {
            let value = secret_manager.resolve_secret(port_var).await?;
            match value {
                Some(res) => Ok(res),
                None => Err(OxyError::SecretNotFound(Some(port_var.clone()))),
            }
        } else {
            Ok("5432".to_string())
        }
    }

    pub async fn get_user(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        if let Some(user) = &self.user
            && !user.is_empty()
        {
            return Ok(user.clone());
        }
        if let Some(user_var) = &self.user_var {
            let value = secret_manager.resolve_secret(user_var).await?;
            match value {
                Some(res) => Ok(res),
                None => Err(OxyError::SecretNotFound(Some(user_var.clone()))),
            }
        } else {
            Ok("postgres".to_string())
        }
    }

    pub async fn get_database(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        if let Some(database) = &self.database
            && !database.is_empty()
        {
            return Ok(database.clone());
        }
        if let Some(database_var) = &self.database_var {
            let value = secret_manager.resolve_secret(database_var).await?;
            match value {
                Some(res) => Ok(res),
                None => Err(OxyError::SecretNotFound(Some(database_var.clone()))),
            }
        } else {
            Ok("postgres".to_string())
        }
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Validate, Default)]
#[garde(context(ValidationContext))]
pub struct Redshift {
    #[serde(default)]
    #[garde(skip)]
    pub host: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub host_var: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub port: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub port_var: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub user: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub user_var: Option<String>,
    #[garde(skip)]
    #[schemars(skip)]
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub password_var: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub database: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub database_var: Option<String>,
}

impl Redshift {
    pub async fn get_password(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        if let Some(password) = &self.password
            && !password.is_empty()
        {
            return Ok(password.clone());
        }
        let value = secret_manager
            .resolve_secret(self.password_var.as_deref().unwrap_or(""))
            .await?;
        match value {
            Some(res) => Ok(res),
            None => Err(OxyError::SecretNotFound(self.password_var.clone())),
        }
    }

    pub async fn get_host(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        if let Some(host) = &self.host
            && !host.is_empty()
        {
            return Ok(host.clone());
        }
        if let Some(host_var) = &self.host_var {
            let value = secret_manager.resolve_secret(host_var).await?;
            match value {
                Some(res) => Ok(res),
                None => Err(OxyError::SecretNotFound(Some(host_var.clone()))),
            }
        } else {
            Ok("localhost".to_string())
        }
    }

    pub async fn get_port(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        if let Some(port) = &self.port
            && !port.is_empty()
        {
            return Ok(port.clone());
        }
        if let Some(port_var) = &self.port_var {
            let value = secret_manager.resolve_secret(port_var).await?;
            match value {
                Some(res) => Ok(res),
                None => Err(OxyError::SecretNotFound(Some(port_var.clone()))),
            }
        } else {
            Ok("5439".to_string())
        }
    }

    pub async fn get_user(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        if let Some(user) = &self.user
            && !user.is_empty()
        {
            return Ok(user.clone());
        }
        if let Some(user_var) = &self.user_var {
            let value = secret_manager.resolve_secret(user_var).await?;
            match value {
                Some(res) => Ok(res),
                None => Err(OxyError::SecretNotFound(Some(user_var.clone()))),
            }
        } else {
            Ok("awsuser".to_string())
        }
    }

    pub async fn get_database(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        if let Some(database) = &self.database
            && !database.is_empty()
        {
            return Ok(database.clone());
        }
        if let Some(database_var) = &self.database_var {
            let value = secret_manager.resolve_secret(database_var).await?;
            match value {
                Some(res) => Ok(res),
                None => Err(OxyError::SecretNotFound(Some(database_var.clone()))),
            }
        } else {
            Ok("dev".to_string())
        }
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Validate, Default)]
#[garde(context(ValidationContext))]
pub struct Mysql {
    #[serde(default)]
    #[garde(skip)]
    pub host: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub host_var: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub port: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub port_var: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub user: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub user_var: Option<String>,
    #[garde(skip)]
    #[schemars(skip)]
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub password_var: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub database: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub database_var: Option<String>,
}

impl Mysql {
    pub async fn get_password(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        if let Some(password) = &self.password
            && !password.is_empty()
        {
            return Ok(password.clone());
        }
        let value = secret_manager
            .resolve_secret(self.password_var.as_deref().unwrap_or(""))
            .await?;
        match value {
            Some(res) => Ok(res),
            None => Err(OxyError::SecretNotFound(self.password_var.clone())),
        }
    }

    pub async fn get_host(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        if let Some(host) = &self.host
            && !host.is_empty()
        {
            return Ok(host.clone());
        }
        if let Some(host_var) = &self.host_var {
            let value = secret_manager.resolve_secret(host_var).await?;
            match value {
                Some(res) => Ok(res),
                None => Err(OxyError::SecretNotFound(Some(host_var.clone()))),
            }
        } else {
            Ok("localhost".to_string())
        }
    }

    pub async fn get_port(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        if let Some(port) = &self.port
            && !port.is_empty()
        {
            return Ok(port.clone());
        }
        if let Some(port_var) = &self.port_var {
            let value = secret_manager.resolve_secret(port_var).await?;
            match value {
                Some(res) => Ok(res),
                None => Err(OxyError::SecretNotFound(Some(port_var.clone()))),
            }
        } else {
            Ok("3306".to_string())
        }
    }

    pub async fn get_user(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        if let Some(user) = &self.user
            && !user.is_empty()
        {
            return Ok(user.clone());
        }
        if let Some(user_var) = &self.user_var {
            let value = secret_manager.resolve_secret(user_var).await?;
            match value {
                Some(res) => Ok(res),
                None => Err(OxyError::SecretNotFound(Some(user_var.clone()))),
            }
        } else {
            Ok("root".to_string())
        }
    }

    pub async fn get_database(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        if let Some(database) = &self.database
            && !database.is_empty()
        {
            return Ok(database.clone());
        }
        if let Some(database_var) = &self.database_var {
            let value = secret_manager.resolve_secret(database_var).await?;
            match value {
                Some(res) => Ok(res),
                None => Err(OxyError::SecretNotFound(Some(database_var.clone()))),
            }
        } else {
            Ok("mysql".to_string())
        }
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Validate, Default)]
#[garde(context(ValidationContext))]
pub struct ClickHouse {
    #[serde(default)]
    #[garde(skip)]
    pub host: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub host_var: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub user: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub user_var: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    #[schemars(skip)]
    pub password: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub password_var: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub database: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub database_var: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub schemas: HashMap<String, Vec<String>>,
    #[serde(default)]
    #[garde(skip)]
    pub role: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub settings_prefix: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub filters: HashMap<String, schemars::schema::SchemaObject>,
}

impl ClickHouse {
    pub async fn get_password(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        if let Some(password) = &self.password
            && !password.is_empty()
        {
            return Ok(password.clone());
        }
        let value = secret_manager
            .resolve_secret(self.password_var.as_deref().unwrap_or(""))
            .await?;
        match value {
            Some(res) => Ok(res),
            None => Err(OxyError::SecretNotFound(self.password_var.clone())),
        }
    }

    pub async fn get_host(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        if let Some(host) = &self.host
            && !host.is_empty()
        {
            return Ok(host.clone());
        }
        if let Some(host_var) = &self.host_var {
            let value = secret_manager.resolve_secret(host_var).await?;
            match value {
                Some(res) => Ok(res),
                None => Err(OxyError::SecretNotFound(Some(host_var.clone()))),
            }
        } else {
            Err(OxyError::ConfigurationError(
                "ClickHouse host or host_var must be specified".to_string(),
            ))
        }
    }

    pub async fn get_user(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        if let Some(user) = &self.user
            && !user.is_empty()
        {
            return Ok(user.clone());
        }
        if let Some(user_var) = &self.user_var {
            let value = secret_manager.resolve_secret(user_var).await?;
            match value {
                Some(res) => Ok(res),
                None => Err(OxyError::SecretNotFound(Some(user_var.clone()))),
            }
        } else {
            Err(OxyError::ConfigurationError(
                "ClickHouse user or user_var must be specified".to_string(),
            ))
        }
    }

    pub async fn get_database(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        if let Some(database) = &self.database
            && !database.is_empty()
        {
            return Ok(database.clone());
        }
        if let Some(database_var) = &self.database_var {
            let value = secret_manager.resolve_secret(database_var).await?;
            match value {
                Some(res) => Ok(res),
                None => Err(OxyError::SecretNotFound(Some(database_var.clone()))),
            }
        } else {
            Err(OxyError::ConfigurationError(
                "ClickHouse database or database_var must be specified".to_string(),
            ))
        }
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Validate, Default)]
#[garde(context(ValidationContext))]
pub struct DOMO {
    #[garde(length(min = 1))]
    pub instance: String,
    #[garde(length(min = 1))]
    pub developer_token_var: String,
    #[garde(length(min = 1))]
    pub dataset_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Validate)]
#[garde(context(AgentValidationContext))]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Validate)]
#[garde(context(AgentValidationContext))]
pub struct ReasoningConfig {
    #[garde(dive)]
    pub effort: ReasoningEffort,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Validate)]
#[garde(context(ValidationContext))]
#[derive(Default)]
pub struct RouteRetrievalConfig {
    /// List of prompts that include this document / route for retrieval
    #[garde(skip)]
    #[serde(default)]
    pub include: Vec<String>,
    /// List of prompts that exclude this document / route for retrieval
    #[garde(skip)]
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Validate)]
#[garde(context(AgentValidationContext))]
pub struct AgentConfig {
    #[serde(default = "default_agent_name")]
    #[schemars(skip)]
    #[garde(skip)]
    pub name: String,
    #[garde(custom(validate_model))]
    pub model: String,
    #[serde(flatten)]
    #[garde(dive)]
    pub r#type: AgentType,
    #[garde(skip)]
    pub context: Option<Vec<AgentContext>>,
    #[serde(default)]
    #[garde(skip)]
    pub tests: Vec<EvalConfig>,

    #[garde(skip)]
    #[serde(default)]
    pub description: String,

    #[serde(default = "default_agent_public")]
    #[garde(skip)]
    pub public: bool,

    #[serde(default)]
    #[garde(skip)]
    pub retrieval: Option<RouteRetrievalConfig>,
    #[garde(dive)]
    pub reasoning: Option<ReasoningConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Validate)]
#[garde(context(AgentValidationContext))]
#[serde(untagged)]
pub enum AgentType {
    Routing(#[garde(dive)] RoutingAgent),
    Default(#[garde(dive)] DefaultAgent),
}

impl std::fmt::Display for AgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentType::Default(_) => write!(f, "default"),
            AgentType::Routing(_) => write!(f, "routing"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Validate)]
#[garde(context(AgentValidationContext))]
pub struct DefaultAgent {
    #[garde(length(min = 1))]
    pub system_instructions: String,
    #[serde(default, flatten)]
    #[garde(skip)]
    pub tools_config: AgentToolsConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Validate)]
#[garde(context(AgentValidationContext))]
#[serde(tag = "type", rename = "routing")]
pub struct RoutingAgent {
    #[serde(default = "default_routing_agent_instructions")]
    #[garde(length(min = 1))]
    pub system_instructions: String,
    #[garde(skip)]
    pub routes: Vec<String>,
    #[serde(default, flatten)]
    #[garde(skip)]
    pub db_config: VectorDBConfig,
    #[serde(flatten)]
    #[garde(skip)]
    pub embedding_config: EmbeddingConfig,
    #[serde(default = "default_synthesize_results")]
    #[garde(skip)]
    pub synthesize_results: bool,
    #[garde(length(min = 1))]
    pub route_fallback: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct AgentToolsConfig {
    #[serde(default = "default_tools")]
    pub tools: Vec<ToolType>,
    #[serde(default = "default_max_tool_calls")]
    pub max_tool_calls: usize,
    #[serde(default = "default_max_tool_concurrency")]
    pub max_tool_concurrency: usize,
}

impl Default for AgentToolsConfig {
    fn default() -> Self {
        AgentToolsConfig {
            tools: default_tools(),
            max_tool_calls: default_max_tool_calls(),
            max_tool_concurrency: default_max_tool_concurrency(),
        }
    }
}

#[derive(Debug, Validate, Deserialize, Serialize, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct AgentContext {
    #[garde(length(min = 1))]
    pub name: String,

    #[serde(flatten)]
    #[garde(dive)]
    #[serde(default)]
    pub context_type: AgentContextType,
}

#[derive(Debug, Validate, Deserialize, Serialize, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct FileContext {
    #[garde(length(min = 1))]
    pub src: Vec<String>,
}

#[derive(Debug, Validate, Deserialize, Serialize, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct SemanticModelContext {
    #[garde(length(min = 1))]
    pub src: String,
}

#[derive(Serialize, Deserialize, Debug, Validate, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentContextType {
    #[serde(rename = "file")]
    File(#[garde(dive)] FileContext),
    #[serde(rename = "semantic_model")]
    SemanticModel(#[garde(dive)] SemanticModelContext),
}

impl Default for AgentContextType {
    fn default() -> Self {
        AgentContextType::File(FileContext { src: Vec::new() })
    }
}

// These are settings stored as strings derived from the config.yml file's defaults section
#[derive(Debug, Validate, Deserialize, Serialize, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct Defaults {
    #[garde(length(min = 1))]
    #[garde(custom(|db: &Option<String>, ctx: &ValidationContext| {
        match db {
            Some(database) => validate_database_exists(database.as_str(), ctx),
            None => Ok(()),
        }
    }))]
    pub database: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Validate, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct BigQuery {
    #[garde(skip)]
    #[serde(default)]
    pub key_path: Option<PathBuf>,
    #[garde(skip)]
    #[serde(default)]
    pub key_path_var: Option<String>,
    #[garde(length(min = 1))]
    pub dataset: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub datasets: HashMap<String, Vec<String>>,
    #[garde(range(min = 1))]
    pub dry_run_limit: Option<u64>,
}

impl BigQuery {
    pub async fn get_key_path(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        if let Some(key_path) = &self.key_path {
            return Ok(key_path.to_string_lossy().to_string());
        }
        if let Some(key_path_var) = &self.key_path_var {
            let value = secret_manager.resolve_secret(key_path_var).await?;
            match value {
                Some(res) => Ok(res),
                None => Err(OxyError::SecretNotFound(Some(key_path_var.clone()))),
            }
        } else {
            Err(OxyError::ConfigurationError(
                "BigQuery key_path or key_path_var must be specified".to_string(),
            ))
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Validate, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct DuckDB {
    #[serde(alias = "dataset", rename = "dataset")]
    #[garde(length(min = 1))]
    pub file_search_path: String,
}

#[derive(Serialize, Deserialize, Debug, Validate, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct Snowflake {
    #[garde(skip)]
    pub account: String,
    #[garde(skip)]
    pub username: String,
    #[garde(skip)]
    pub password: Option<String>,
    #[garde(skip)]
    pub password_var: Option<String>,
    #[garde(skip)]
    pub warehouse: String,
    #[garde(skip)]
    pub database: String,
    #[garde(skip)]
    pub role: Option<String>,
    #[garde(custom(validate_optional_private_key_path))]
    #[serde(default)]
    pub private_key_path: Option<PathBuf>,
    #[garde(skip)]
    #[serde(default)]
    pub datasets: HashMap<String, Vec<String>>,
}

impl Snowflake {
    /// Validates that the Snowflake configuration has proper authentication configured
    pub fn validate_auth(&self) -> Result<(), OxyError> {
        let has_private_key = self.private_key_path.is_some();
        let has_password_var = self.password_var.is_some();
        let has_password = self.password.as_ref().is_some_and(|p| !p.is_empty());

        if !has_private_key && !has_password_var && !has_password {
            return Err(OxyError::ConfigurationError(
                "Snowflake configuration must have either 'private_key_path' or 'password_var' (or 'password') configured".to_string()
            ));
        }

        Ok(())
    }

    pub async fn get_password(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        // First validate that we have proper auth configuration
        self.validate_auth()?;

        if let Some(password) = &self.password
            && !password.is_empty()
        {
            return Ok(password.clone());
        }

        if let Some(password_var) = &self.password_var {
            let value = secret_manager.resolve_secret(password_var).await?;
            match value {
                Some(res) => Ok(res),
                None => Err(OxyError::SecretNotFound(self.password_var.clone())),
            }
        } else {
            Err(OxyError::ConfigurationError(
                "No password or password_var configured for Snowflake".to_string(),
            ))
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Validate, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
#[serde(tag = "type")]
pub enum DatabaseType {
    #[serde(rename = "bigquery")]
    Bigquery(#[garde(dive)] BigQuery),
    #[serde(rename = "duckdb")]
    DuckDB(#[garde(dive)] DuckDB),
    #[serde(rename = "snowflake")]
    Snowflake(#[garde(dive)] Snowflake),
    #[serde(rename = "postgres")]
    Postgres(#[garde(dive)] Postgres),
    #[serde(rename = "redshift")]
    Redshift(#[garde(dive)] Redshift),
    #[serde(rename = "mysql")]
    Mysql(#[garde(dive)] Mysql),
    #[serde(rename = "clickhouse")]
    ClickHouse(#[garde(dive)] ClickHouse),
    #[serde(rename = "domo")]
    DOMO(#[garde(dive)] DOMO),
}

impl std::fmt::Display for DatabaseType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseType::Bigquery(_) => write!(f, "bigquery"),
            DatabaseType::DuckDB(_) => write!(f, "duckdb"),
            DatabaseType::Snowflake(_) => write!(f, "snowflake"),
            DatabaseType::Postgres(_) => write!(f, "postgres"),
            DatabaseType::Redshift(_) => write!(f, "redshift"),
            DatabaseType::Mysql(_) => write!(f, "mysql"),
            DatabaseType::ClickHouse(_) => write!(f, "clickhouse"),
            DatabaseType::DOMO(_) => write!(f, "domo"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Validate, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct Database {
    #[garde(length(min = 1))]
    pub name: String,

    #[serde(flatten)]
    #[garde(dive)]
    pub database_type: DatabaseType,
}

impl Database {
    pub fn dialect(&self) -> String {
        match &self.database_type {
            DatabaseType::Bigquery(_) => "bigquery".to_owned(),
            DatabaseType::DuckDB(_) => "duckdb".to_owned(),
            DatabaseType::Postgres(_) => "postgres".to_owned(),
            DatabaseType::Redshift(_) => "postgres".to_owned(),
            DatabaseType::Mysql(_) => "mysql".to_owned(),
            DatabaseType::ClickHouse(_) => "clickhouse".to_string(),
            DatabaseType::Snowflake(_) => "snowflake".to_string(),
            DatabaseType::DOMO(_) => "domo".to_string(),
        }
    }

    pub fn datasets(&self) -> HashMap<String, Vec<String>> {
        match &self.database_type {
            DatabaseType::Bigquery(bq) => match (bq.dataset.is_some(), bq.datasets.is_empty()) {
                (true, _) => HashMap::from_iter([(
                    bq.dataset.clone().unwrap().to_string(),
                    vec!["*".to_string()],
                )]),
                (false, false) => bq.datasets.clone(),
                (false, true) => {
                    HashMap::from_iter([("`region-us`".to_string(), vec!["*".to_string()])])
                }
            },
            DatabaseType::ClickHouse(ch) => {
                if ch.schemas.is_empty() {
                    HashMap::from_iter([(String::default(), vec!["*".to_string()])])
                } else {
                    ch.schemas.clone()
                }
            }
            DatabaseType::Snowflake(sf) => {
                if sf.datasets.is_empty() {
                    HashMap::from_iter([("CORE".to_string(), vec!["*".to_string()])]) // Default to CORE schema
                } else {
                    sf.datasets.clone()
                }
            }
            _ => Default::default(),
        }
    }

    pub fn with_datasets(self, datasets: Vec<String>) -> Self {
        if datasets.is_empty() {
            return self;
        }

        match &self.database_type {
            DatabaseType::Bigquery(bq) => {
                let mut datasets_map = HashMap::new();
                for dataset in datasets {
                    let tables = bq.datasets.get(&dataset).cloned();
                    datasets_map.insert(dataset, tables.unwrap_or(vec!["*".to_string()]));
                }
                Database {
                    database_type: DatabaseType::Bigquery(BigQuery {
                        datasets: datasets_map,
                        ..bq.clone()
                    }),
                    ..self
                }
            }
            DatabaseType::ClickHouse(ch) => {
                let mut datasets_map = HashMap::new();
                for dataset in datasets {
                    let tables = ch.schemas.get(&dataset).cloned();
                    datasets_map.insert(dataset, tables.unwrap_or(vec!["*".to_string()]));
                }
                Database {
                    database_type: DatabaseType::ClickHouse(ClickHouse {
                        schemas: datasets_map,
                        ..ch.clone()
                    }),
                    ..self
                }
            }
            _ => self,
        }
    }
}

#[derive(Deserialize, Debug, Clone, Serialize, JsonSchema)]
pub struct AzureModel {
    pub azure_deployment_id: String,
    pub azure_api_version: String,
}

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
    /// Resolve the header value, either directly or from environment variable
    pub async fn resolve(&self, secrets_manager: &SecretsManager) -> Result<String, OxyError> {
        match self {
            HeaderValue::Direct(value) => Ok(value.clone()),
            HeaderValue::EnvVar { env_var } => {
                let result = secrets_manager.resolve_secret(env_var).await?;
                match result {
                    Some(res) => Ok(res),
                    None => Err(OxyError::SecretNotFound(Some(env_var.clone()))),
                }
            }
        }
    }
}

#[skip_serializing_none]
#[derive(Deserialize, Debug, Clone, Validate, Serialize, JsonSchema)]
#[garde(context(ValidationContext))]
#[serde(tag = "vendor")]
pub enum Model {
    #[serde(rename = "openai")]
    OpenAI {
        #[garde(length(min = 1))]
        name: String,
        #[garde(length(min = 1))]
        model_ref: String,
        #[garde(custom(validate_env_var))]
        key_var: String,
        #[serde(default = "default_openai_api_url")]
        #[garde(skip)]
        api_url: Option<String>,
        #[serde(flatten)]
        #[garde(skip)]
        azure: Option<AzureModel>,
        #[serde(default)]
        #[garde(skip)]
        headers: Option<HashMap<String, HeaderValue>>,
    },
    #[serde(rename = "google")]
    Google {
        #[garde(length(min = 1))]
        name: String,
        #[garde(length(min = 1))]
        model_ref: String,
        #[garde(custom(validate_env_var))]
        key_var: String,
    },
    #[serde(rename = "ollama")]
    Ollama {
        #[garde(length(min = 1))]
        name: String,
        #[garde(length(min = 1))]
        model_ref: String,
        #[garde(length(min = 1))]
        api_key: String,
        #[garde(length(min = 1))]
        api_url: String,
    },
    #[serde(rename = "anthropic")]
    Anthropic {
        #[garde(length(min = 1))]
        name: String,
        #[garde(length(min = 1))]
        model_ref: String,
        #[garde(custom(validate_env_var))]
        key_var: String,
        #[serde(default = "default_anthropic_api_url")]
        #[garde(skip)]
        api_url: Option<String>,
    },
}

impl Model {
    pub fn model_name(&self) -> &str {
        match self {
            Model::OpenAI { model_ref, .. } => model_ref,
            Model::Ollama { model_ref, .. } => model_ref,
            Model::Google { model_ref, .. } => model_ref,
            Model::Anthropic { model_ref, .. } => model_ref,
        }
    }

    /// Resolve headers for OpenAI models, returning a HashMap with resolved values
    pub async fn resolve_headers(
        &self,
        secrets_manager: &SecretsManager,
    ) -> Result<HashMap<String, String>, OxyError> {
        match self {
            Model::OpenAI { headers, .. } => {
                let mut resolved_headers = HashMap::new();
                if let Some(headers_map) = headers {
                    for (key, header_value) in headers_map {
                        let resolved_value = header_value.resolve(secrets_manager).await?;
                        resolved_headers.insert(key.clone(), resolved_value);
                    }
                }
                Ok(resolved_headers)
            }
            _ => Ok(HashMap::new()), // Other models don't support custom headers yet
        }
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(tag = "type")]
pub enum AnonymizerConfig {
    #[serde(rename = "flash_text")]
    FlashText {
        #[serde(flatten)]
        source: FlashTextSourceType,
        #[serde(default = "default_anonymizer_pluralize")]
        pluralize: bool,
        #[serde(default = "default_case_sensitive")]
        case_sensitive: bool,
    },
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(untagged)]
pub enum FlashTextSourceType {
    Keywords {
        keywords_file: PathBuf,
        #[serde(default = "default_anonymizer_replacement")]
        replacement: String,
    },
    Mapping {
        mapping_file: PathBuf,
        #[serde(default = "default_delimiter")]
        delimiter: String,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, JsonSchema)]
pub enum FileFormat {
    #[serde(rename = "json")]
    Json,
    #[serde(rename = "markdown")]
    #[default]
    Markdown,
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct AgentTask {
    #[garde(length(min = 1))]
    pub prompt: String,
    #[garde(custom(validate_agent_exists))]
    pub agent_ref: String,
    #[serde(default = "default_retry")]
    #[garde(skip)]
    pub retry: usize,

    #[serde(default = "default_consistency_run")]
    #[garde(skip)]
    pub consistency_run: usize,

    #[garde(dive)]
    pub export: Option<TaskExport>,
}

impl Hash for AgentTask {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.agent_ref.hash(state);
        self.prompt.hash(state);
    }
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub enum ExportFormat {
    #[serde(rename = "sql")]
    SQL,
    #[serde(rename = "csv")]
    CSV,
    #[serde(rename = "json")]
    JSON,
    #[serde(rename = "txt")]
    TXT,
    #[serde(rename = "docx")]
    DOCX,
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct TaskExport {
    #[garde(length(min = 1))]
    pub path: String,
    #[garde(dive)]
    pub format: ExportFormat,
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct TaskCache {
    #[serde(default = "default_cache_enabled")]
    #[garde(skip)]
    pub enabled: bool,
    #[garde(length(min = 1))]
    pub path: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema, Hash)]
#[garde(context(ValidationContext))]
#[serde(untagged)]
pub enum SQL {
    File {
        #[garde(length(min = 1))]
        sql_file: String,
    },
    Query {
        #[garde(length(min = 1))]
        sql_query: String,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct ExecuteSQLTask {
    #[garde(custom(validate_database_exists))]
    pub database: String,
    #[garde(dive)]
    #[serde(flatten)]
    pub sql: SQL,
    #[serde(default)]
    #[garde(skip)]
    pub variables: Option<HashMap<String, String>>,

    #[garde(dive)]
    pub export: Option<TaskExport>,

    #[garde(range(min = 1))]
    pub dry_run_limit: Option<u64>,
}

impl Hash for ExecuteSQLTask {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.database.hash(state);
        self.sql.hash(state);
        if let Some(ref vars) = self.variables {
            for (key, value) in vars.iter().sorted() {
                key.hash(state);
                value.hash(state);
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct SemanticQueryTask {
    // TODO: validate
    #[garde(skip)]
    #[serde(flatten)]
    pub query: SemanticQueryParams,

    // Optional export configuration (reuses existing task export logic)
    #[garde(dive)]
    pub export: Option<TaskExport>,
}

impl Hash for SemanticQueryTask {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.query.hash(state);
        // Export options don't affect semantic equivalence for caching
    }
}

// -----------------------------------------------------------------------------
// Supporting Enums & Structs
// -----------------------------------------------------------------------------
#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema, Hash, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SemanticOrderDirection {
    Asc,
    Desc,
}

/// Scalar comparison filter (eq, neq, gt, gte, lt, lte)
#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct ScalarFilter {
    #[garde(skip)]
    pub value: Value,
}

/// Array-based filter (in, not_in)
#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct ArrayFilter {
    #[garde(skip)]
    pub values: Vec<Value>,
}

/// Date range filter (in_date_range, not_in_date_range)
#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct DateRangeFilter {
    #[garde(skip)]
    pub from: Value,
    #[garde(skip)]
    pub to: Value,
}

impl DateRangeFilter {
    /// Convert relative date expressions to ISO datetime strings
    /// Supports natural language expressions like "now", "today", "7 days ago", "last week", etc.
    /// Uses chrono-english for robust natural language parsing.
    pub fn resolve_relative_dates(&self) -> Result<Self, crate::errors::OxyError> {
        Ok(Self {
            from: Self::resolve_date_value(&self.from)?,
            to: Self::resolve_date_value(&self.to)?,
        })
    }

    fn resolve_date_value(value: &Value) -> Result<Value, crate::errors::OxyError> {
        match value {
            Value::String(s) => {
                let resolved = Self::parse_relative_date(s)?;
                Ok(Value::String(resolved))
            }
            other => Ok(other.clone()),
        }
    }

    fn parse_relative_date(expr: &str) -> Result<String, crate::errors::OxyError> {
        use chrono::Utc;

        // Try parsing with chrono-english for natural language dates
        match chrono_english::parse_date_string(expr, Utc::now(), chrono_english::Dialect::Us) {
            Ok(datetime) => Ok(datetime.to_rfc3339()),
            Err(_) => {
                // If chrono-english can't parse it, assume it's already a valid datetime string
                // and return it as-is
                Ok(expr.to_string())
            }
        }
    }
}

/// Enum representing different filter types with their appropriate value types
#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
#[serde(tag = "op", content = "value")]
pub enum SemanticFilterType {
    #[serde(rename = "eq")]
    Eq(#[garde(dive)] ScalarFilter),
    #[serde(rename = "neq")]
    Neq(#[garde(dive)] ScalarFilter),
    #[serde(rename = "gt")]
    Gt(#[garde(dive)] ScalarFilter),
    #[serde(rename = "gte")]
    Gte(#[garde(dive)] ScalarFilter),
    #[serde(rename = "lt")]
    Lt(#[garde(dive)] ScalarFilter),
    #[serde(rename = "lte")]
    Lte(#[garde(dive)] ScalarFilter),
    #[serde(rename = "in")]
    In(#[garde(dive)] ArrayFilter),
    #[serde(rename = "not_in")]
    NotIn(#[garde(dive)] ArrayFilter),
    #[serde(rename = "in_date_range")]
    InDateRange(#[garde(dive)] DateRangeFilter),
    #[serde(rename = "not_in_date_range")]
    NotInDateRange(#[garde(dive)] DateRangeFilter),
}

impl Hash for SemanticFilterType {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash the discriminant first
        std::mem::discriminant(self).hash(state);
        // Then hash the value(s)
        match self {
            SemanticFilterType::Eq(f)
            | SemanticFilterType::Neq(f)
            | SemanticFilterType::Gt(f)
            | SemanticFilterType::Gte(f)
            | SemanticFilterType::Lt(f)
            | SemanticFilterType::Lte(f) => {
                if let Ok(s) = serde_json::to_string(&f.value) {
                    s.hash(state);
                }
            }
            SemanticFilterType::In(f) | SemanticFilterType::NotIn(f) => {
                for v in &f.values {
                    if let Ok(s) = serde_json::to_string(v) {
                        s.hash(state);
                    }
                }
            }
            SemanticFilterType::InDateRange(f) | SemanticFilterType::NotInDateRange(f) => {
                if let Ok(s) = serde_json::to_string(&f.from) {
                    s.hash(state);
                }
                if let Ok(s) = serde_json::to_string(&f.to) {
                    s.hash(state);
                }
            }
        }
    }
}

impl PartialEq for SemanticFilterType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (SemanticFilterType::Eq(a), SemanticFilterType::Eq(b))
            | (SemanticFilterType::Neq(a), SemanticFilterType::Neq(b))
            | (SemanticFilterType::Gt(a), SemanticFilterType::Gt(b))
            | (SemanticFilterType::Gte(a), SemanticFilterType::Gte(b))
            | (SemanticFilterType::Lt(a), SemanticFilterType::Lt(b))
            | (SemanticFilterType::Lte(a), SemanticFilterType::Lte(b)) => {
                serde_json::to_string(&a.value).ok() == serde_json::to_string(&b.value).ok()
            }
            (SemanticFilterType::In(a), SemanticFilterType::In(b))
            | (SemanticFilterType::NotIn(a), SemanticFilterType::NotIn(b)) => {
                a.values.len() == b.values.len()
                    && a.values.iter().zip(b.values.iter()).all(|(x, y)| {
                        serde_json::to_string(x).ok() == serde_json::to_string(y).ok()
                    })
            }
            (SemanticFilterType::InDateRange(a), SemanticFilterType::InDateRange(b))
            | (SemanticFilterType::NotInDateRange(a), SemanticFilterType::NotInDateRange(b)) => {
                serde_json::to_string(&a.from).ok() == serde_json::to_string(&b.from).ok()
                    && serde_json::to_string(&a.to).ok() == serde_json::to_string(&b.to).ok()
            }
            _ => false,
        }
    }
}

impl Eq for SemanticFilterType {}

impl SemanticFilterType {
    /// Get the operator name as a string (for CubeJS conversion)
    pub fn operator_name(&self) -> &'static str {
        match self {
            SemanticFilterType::Eq(_) => "equals",
            SemanticFilterType::Neq(_) => "notEquals",
            SemanticFilterType::Gt(_) => "gt",
            SemanticFilterType::Gte(_) => "gte",
            SemanticFilterType::Lt(_) => "lt",
            SemanticFilterType::Lte(_) => "lte",
            SemanticFilterType::In(_) => "equals",
            SemanticFilterType::NotIn(_) => "notEquals",
            SemanticFilterType::InDateRange(_) => "inDateRange",
            SemanticFilterType::NotInDateRange(_) => "notInDateRange",
        }
    }

    /// Get the values as a Vec<Value> for CubeJS conversion
    pub fn values(&self) -> Vec<Value> {
        match self {
            SemanticFilterType::Eq(f)
            | SemanticFilterType::Neq(f)
            | SemanticFilterType::Gt(f)
            | SemanticFilterType::Gte(f)
            | SemanticFilterType::Lt(f)
            | SemanticFilterType::Lte(f) => vec![f.value.clone()],
            SemanticFilterType::In(f) | SemanticFilterType::NotIn(f) => f.values.clone(),
            SemanticFilterType::InDateRange(f) | SemanticFilterType::NotInDateRange(f) => {
                vec![f.from.clone(), f.to.clone()]
            }
        }
    }

    /// Check if this filter type requires array values
    pub fn requires_array(&self) -> bool {
        matches!(
            self,
            SemanticFilterType::In(_)
                | SemanticFilterType::NotIn(_)
                | SemanticFilterType::InDateRange(_)
                | SemanticFilterType::NotInDateRange(_)
        )
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct SemanticFilter {
    #[garde(length(min = 1))]
    pub field: String,
    #[serde(flatten)]
    #[garde(dive)]
    pub filter_type: SemanticFilterType,
}

impl Hash for SemanticFilter {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.field.hash(state);
        self.filter_type.hash(state);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct SemanticOrder {
    #[garde(length(min = 1))]
    pub field: String,
    #[serde(default = "default_order_direction")]
    #[garde(skip)]
    pub direction: SemanticOrderDirection,
}

impl Hash for SemanticOrder {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.field.hash(state);
        self.direction.hash(state);
    }
}

fn default_order_direction() -> SemanticOrderDirection {
    SemanticOrderDirection::Asc
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct FormatterTask {
    #[garde(length(min = 1))]
    pub template: String,
    #[garde(dive)]
    pub export: Option<TaskExport>,
}

impl Hash for FormatterTask {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.template.hash(state);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct WorkflowTask {
    #[garde(skip)]
    pub src: PathBuf,
    #[garde(skip)]
    pub variables: Option<HashMap<String, Value>>,
    #[garde(dive)]
    pub export: Option<TaskExport>,
}

impl Hash for WorkflowTask {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.src.hash(state);
        if let Some(ref vars) = self.variables {
            for (key, value) in vars.iter().sorted_by_cached_key(|(key, _)| *key) {
                key.hash(state);
                value.hash(state);
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct OmniQueryTask {
    #[garde(custom(validate_omni_integration_exists))]
    pub integration: String,
    #[garde(length(min = 1))]
    pub topic: String,
    #[serde(flatten)]
    #[garde(skip)]
    pub query: crate::tools::types::OmniQueryParams,
    #[garde(dive)]
    pub export: Option<TaskExport>,
}

impl Hash for OmniQueryTask {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.integration.hash(state);
        self.topic.hash(state);
        self.query.hash(state);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Hash)]
#[serde(untagged)]
pub enum LoopValues {
    Template(String),
    Array(Vec<serde_json::Value>),
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct LoopSequentialTask {
    #[garde(skip)]
    pub values: LoopValues,
    #[garde(dive)]
    pub tasks: Vec<Task>,
    #[garde(skip)]
    #[serde(default = "default_loop_concurrency")]
    pub concurrency: usize,
}

impl Hash for LoopSequentialTask {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.values.hash(state);
        for task in &self.tasks {
            task.hash(state);
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema, Hash)]
#[garde(context(ValidationContext))]
pub struct Condition {
    #[garde(length(min = 1))]
    #[serde(rename = "if")]
    pub if_expr: String,
    #[garde(dive)]
    pub tasks: Vec<Task>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct ConditionalTask {
    #[garde(length(min = 1))]
    pub conditions: Vec<Condition>,
    #[garde(skip)]
    #[serde(default, rename = "else")]
    pub else_tasks: Option<Vec<Task>>,
}

impl Hash for ConditionalTask {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        for condition in &self.conditions {
            condition.if_expr.hash(state);
            for task in &condition.tasks {
                task.hash(state);
            }
        }
        if let Some(ref else_tasks) = self.else_tasks {
            for task in else_tasks {
                task.hash(state);
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema, Hash)]
#[garde(context(ValidationContext))]
#[serde(tag = "type")]
pub enum TaskType {
    #[serde(rename = "agent")]
    Agent(#[garde(dive)] AgentTask),
    #[serde(rename = "execute_sql")]
    ExecuteSQL(#[garde(dive)] ExecuteSQLTask),
    #[serde(rename = "semantic_query")]
    SemanticQuery(#[garde(dive)] SemanticQueryTask),
    #[serde(rename = "omni_query")]
    OmniQuery(#[garde(dive)] OmniQueryTask),
    #[serde(rename = "loop_sequential")]
    LoopSequential(#[garde(dive)] LoopSequentialTask),
    #[serde(rename = "formatter")]
    Formatter(#[garde(dive)] FormatterTask),
    #[serde(rename = "workflow")]
    Workflow(#[garde(dive)] WorkflowTask),
    #[serde(rename = "conditional")]
    Conditional(#[garde(dive)] ConditionalTask),
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize, JsonSchema)]
pub struct TempWorkflow {
    pub tasks: Vec<Task>,
    #[serde(flatten)]
    pub variables: Option<Variables>,
    #[serde(default = "default_tests")]
    pub tests: Vec<EvalConfig>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub retrieval: Option<RouteRetrievalConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct Task {
    #[schemars(
        description = "Unique name for the task within the workflow. Format: alphanumeric and underscores only, starting with a letter."
    )]
    #[garde(length(min = 1))]
    pub name: String,
    #[serde(flatten)]
    #[garde(dive)]
    #[garde(custom(validate_task))]
    pub task_type: TaskType,
    #[garde(dive)]
    pub cache: Option<TaskCache>,
}

impl Task {
    pub fn kind(&self) -> &str {
        match &self.task_type {
            TaskType::Agent(_) => "agent",
            TaskType::ExecuteSQL(_) => "execute_sql",
            TaskType::SemanticQuery(_) => "semantic_query",
            TaskType::OmniQuery(_) => "omni_query",
            TaskType::LoopSequential(_) => "loop",
            TaskType::Formatter(_) => "formatter",
            TaskType::Workflow(_) => "sub_workflow",
            TaskType::Conditional(_) => "conditional",
            TaskType::Unknown => "unknown",
        }
    }
}

impl Hash for Task {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.task_type.hash(state);
    }
}

#[derive(Serialize, Deserialize, Debug, Validate, JsonSchema, Clone)]
#[garde(context(ValidationContext))]
pub struct EvalConfig {
    #[garde(dive)]
    #[serde(flatten)]
    pub kind: EvalKind,
    #[garde(dive)]
    #[serde(default = "default_solvers")]
    pub metrics: Vec<SolverKind>,
    #[garde(skip)]
    #[serde(default = "default_consistency_concurrency")]
    pub concurrency: usize,
    #[garde(skip)]
    pub task_ref: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Validate, JsonSchema, Clone)]
#[serde(tag = "type")]
#[garde(context(ValidationContext))]
pub enum EvalKind {
    #[serde(rename = "consistency")]
    Consistency(#[garde(dive)] Consistency),
    #[serde(rename = "custom")]
    Custom(#[garde(dive)] Custom),
}

#[derive(Serialize, Deserialize, Debug, Validate, JsonSchema, Clone)]
#[garde(context(ValidationContext))]
pub struct Consistency {
    #[garde(skip)]
    #[serde(default = "default_n")]
    pub n: usize,
    #[garde(length(min = 1))]
    pub task_description: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Validate, JsonSchema, Clone)]
#[garde(context(ValidationContext))]
pub struct Custom {
    #[garde(length(min = 1))]
    pub dataset: String,
    #[garde(length(min = 1))]
    pub workflow_variable_name: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub is_context_id: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SolverKind {
    ContextRecall(#[garde(dive)] ContextRecallSolver),
    Similarity(#[garde(dive)] SimilaritySolver),
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[serde(tag = "distance")]
#[garde(context(ValidationContext))]
#[derive(Default)]
pub enum DistanceMethod {
    #[default]
    Levenshtein,
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct ContextRecallSolver {
    #[serde(default)]
    #[garde(dive)]
    pub distance: DistanceMethod,
    #[garde(range(min = 0 as f32, max = 1_f32))]
    #[serde(default = "default_threshold")]
    pub threshold: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct SimilaritySolver {
    #[garde(length(min = 1))]
    #[serde(default = "default_consistency_prompt")]
    pub prompt: String,
    #[garde(length(min = 1))]
    pub model_ref: Option<String>,
    #[garde(skip)]
    #[serde(default = "default_scores")]
    pub scores: HashMap<String, f32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct Workflow {
    #[serde(skip)]
    #[schemars(skip)]
    #[garde(skip)]
    pub name: String,
    #[garde(dive)]
    pub tasks: Vec<Task>,
    #[serde(default = "default_tests")]
    #[garde(dive)]
    pub tests: Vec<EvalConfig>,
    #[serde(flatten)]
    #[garde(skip)]
    pub variables: Option<Variables>,
    #[garde(skip)]
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    #[garde(dive)]
    pub retrieval: Option<RouteRetrievalConfig>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct VisualizeTool {
    pub name: String,
    #[serde(default = "default_visualize_tool_description")]
    pub description: String,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct WorkflowTool {
    pub name: String,
    pub description: String,
    pub workflow_ref: String,
    pub variables: Option<Variables>,
    pub output_task_ref: Option<String>,
    #[serde(skip)]
    #[schemars(skip)]
    pub is_verified: bool,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct AgentTool {
    pub name: String,
    pub description: String,
    pub agent_ref: String,
    #[serde(skip)]
    #[schemars(skip)]
    pub is_verified: bool,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct EmbeddingConfig {
    #[serde(default = "default_embed_model")]
    pub embed_model: String,
    #[serde(default = "default_retrieval_n_dims")]
    pub n_dims: usize,
    #[serde(default = "default_retrieval_top_k")]
    pub top_k: usize,
    #[serde(default = "default_retrieval_factor")]
    pub factor: usize,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(untagged)]
pub enum VectorDBConfig {
    LanceDB {
        #[serde(default = "default_lance_db_path")]
        db_path: String,
    },
}

impl Default for VectorDBConfig {
    fn default() -> Self {
        VectorDBConfig::LanceDB {
            db_path: default_lance_db_path(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct RetrievalConfig {
    pub name: String,
    #[serde(default = "default_retrieval_tool_description")]
    pub description: String,
    pub src: Vec<String>,
    #[serde(default = "default_api_url")]
    pub api_url: String,
    pub api_key: Option<String>,
    #[serde(default = "default_key_var")]
    pub key_var: String,
    #[serde(flatten)]
    pub embedding_config: EmbeddingConfig,
    #[serde(flatten, default)]
    pub db_config: VectorDBConfig,
}

impl RetrievalConfig {
    pub fn get_api_key(&self) -> Result<String, OxyError> {
        match &self.api_key {
            Some(key) => Ok(key.to_string()),
            None => std::env::var(&self.key_var).map_err(|_| {
                OxyError::AgentError(format!(
                    "API key not found in environment variable {}",
                    self.key_var
                ))
            }),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct ExecuteSQLTool {
    pub name: String,
    #[serde(default = "default_sql_tool_description")]
    pub description: String,
    pub database: String,
    pub dry_run_limit: Option<u64>,

    #[serde(skip)]
    #[schemars(skip)]
    pub sql: Option<String>, // Used for routing agent
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct SemanticQueryTool {
    pub name: String,
    #[serde(default = "default_semantic_query_tool_description")]
    pub description: String,
    pub dry_run_limit: Option<u64>,
    pub topic: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct ValidateSQLTool {
    pub name: String,
    #[serde(default = "default_validate_sql_tool_description")]
    pub description: String,
    pub database: String,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct CreateDataAppTool {
    pub name: String,
    #[serde(default = "default_create_data_app_tool_description")]
    pub description: String,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct OmniQueryTool {
    pub name: String,
    #[serde(default = "default_omni_query_tool_description")]
    pub description: String,
    pub topic: String,
    pub integration: String,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct MarkdownDisplay {
    pub content: String,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Validate)]
#[garde(context(ValidationContext))]
pub struct LineChartDisplay {
    #[garde(length(min = 1))]
    pub x: String,
    #[garde(length(min = 1))]
    pub y: String,
    #[garde(skip)]
    pub x_axis_label: Option<String>,
    #[garde(skip)]
    pub y_axis_label: Option<String>,
    #[garde(length(min = 1))]
    #[garde(custom(validate_task_data_reference))]
    #[schemars(description = "reference data output from a table using table name")]
    pub data: String,
    #[garde(skip)]
    pub series: Option<String>,
    #[garde(skip)]
    pub title: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Validate)]
#[garde(context(ValidationContext))]
pub struct BarChartDisplay {
    #[garde(length(min = 1))]
    pub x: String,
    #[garde(length(min = 1))]
    pub y: String,
    #[garde(skip)]
    pub title: Option<String>,
    #[garde(length(min = 1))]
    #[garde(custom(validate_task_data_reference))]
    #[schemars(description = "reference data output from a table using table name")]
    pub data: String,
    #[garde(skip)]
    pub series: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Validate)]
#[garde(context(ValidationContext))]
pub struct PieChartDisplay {
    #[garde(length(min = 1))]
    pub name: String,
    #[garde(length(min = 1))]
    pub value: String,
    #[garde(skip)]
    pub title: Option<String>,
    #[garde(length(min = 1))]
    #[garde(custom(validate_task_data_reference))]
    #[schemars(description = "reference data output from a table using table name")]
    pub data: String,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Validate)]
#[garde(context(ValidationContext))]
pub struct TableDisplay {
    #[garde(length(min = 1))]
    #[garde(custom(validate_task_data_reference))]
    pub data: String,
    #[garde(skip)]
    pub title: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Validate)]
#[serde(tag = "type")]
#[garde(context(ValidationContext))]
pub enum Display {
    #[serde(rename = "markdown")]
    Markdown(#[garde(skip)] MarkdownDisplay),
    #[serde(rename = "line_chart")]
    LineChart(#[garde(dive)] LineChartDisplay),
    #[serde(rename = "pie_chart")]
    PieChart(#[garde(dive)] PieChartDisplay),
    #[serde(rename = "bar_chart")]
    BarChart(#[garde(dive)] BarChartDisplay),
    #[serde(rename = "table")]
    Table(#[garde(dive)] TableDisplay),
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Validate, Default)]
#[garde(context(ValidationContext))]
pub struct AppConfig {
    #[schemars(description = "tasks to prepare the data for the app")]
    #[garde(dive)]
    #[garde(length(min = 1))]
    pub tasks: Vec<Task>,
    #[schemars(description = "display blocks to render the app")]
    #[garde(length(min = 1))]
    #[garde(dive)]
    pub display: Vec<Display>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(tag = "type")]
pub enum ToolType {
    #[serde(rename = "execute_sql")]
    ExecuteSQL(ExecuteSQLTool),
    #[serde(rename = "validate_sql")]
    ValidateSQL(ValidateSQLTool),
    #[serde(rename = "retrieval")]
    Retrieval(RetrievalConfig),
    #[serde(rename = "visualize")]
    Visualize(VisualizeTool),
    #[serde(rename = "workflow")]
    Workflow(WorkflowTool),
    #[serde(rename = "agent")]
    Agent(AgentTool),
    #[serde(rename = "create_data_app")]
    CreateDataApp(CreateDataAppTool),
    #[serde(rename = "omni_query")]
    OmniQuery(OmniQueryTool),
    #[serde(rename = "semantic_query")]
    SemanticQuery(SemanticQueryTool),
}

impl From<ExecuteSQLTool> for ToolType {
    fn from(tool: ExecuteSQLTool) -> Self {
        ToolType::ExecuteSQL(tool)
    }
}
impl From<ValidateSQLTool> for ToolType {
    fn from(tool: ValidateSQLTool) -> Self {
        ToolType::ValidateSQL(tool)
    }
}
impl From<RetrievalConfig> for ToolType {
    fn from(tool: RetrievalConfig) -> Self {
        ToolType::Retrieval(tool)
    }
}

impl From<OmniQueryTool> for ToolType {
    fn from(tool: OmniQueryTool) -> Self {
        ToolType::OmniQuery(tool)
    }
}

impl From<SemanticQueryTool> for ToolType {
    fn from(tool: SemanticQueryTool) -> Self {
        ToolType::SemanticQuery(tool)
    }
}

fn default_openai_api_url() -> Option<String> {
    Some("https://api.openai.com/v1".to_string())
}

fn default_anthropic_api_url() -> Option<String> {
    Some("https://api.anthropic.com/v1".to_string())
}

fn default_anonymizer_replacement() -> String {
    "FLASH".to_string()
}

fn default_delimiter() -> String {
    ",".to_string()
}

fn default_anonymizer_pluralize() -> bool {
    false
}

fn default_case_sensitive() -> bool {
    false
}

fn default_retry() -> usize {
    1
}

fn default_consistency_run() -> usize {
    1
}

fn default_retrieval_tool_description() -> String {
    "Retrieve the relevant SQL queries to support query generation.".to_string()
}

fn default_visualize_tool_description() -> String {
    "Render a chart based on the data provided, make sure to use the correct chart type and fields."
        .to_string()
}

fn default_lance_db_path() -> String {
    ".lancedb".to_string()
}

fn default_embed_model() -> String {
    "text-embedding-3-small".to_string()
}

fn default_api_url() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_key_var() -> String {
    "OPENAI_API_KEY".to_string()
}

fn default_retrieval_n_dims() -> usize {
    512
}

fn default_retrieval_top_k() -> usize {
    4
}

fn default_retrieval_factor() -> usize {
    5
}

fn default_sql_tool_description() -> String {
    "Execute the SQL query. If the query is invalid, fix it and run again.".to_string()
}

fn default_semantic_query_tool_description() -> String {
    "Query the semantic layer using natural language. This tool generates SQL from semantic models via CubeJS and executes it.".to_string()
}

fn default_cube_url() -> String {
    "http://localhost:4000".to_string()
}

fn default_omni_tool_description() -> String {
    "Execute query on the database. Construct from Omni semantic model.".to_string()
}

fn default_validate_sql_tool_description() -> String {
    "Validate the SQL query. If the query is invalid, fix it and run again.".to_string()
}

fn default_create_data_app_tool_description() -> String {
    "Create a data app/dashboard to visualize metrics.".to_string()
}

fn default_omni_query_tool_description() -> String {
    "Query data through Omni's semantic layer API. Use this tool to execute queries against topics, dimensions, and measures defined in the Omni semantic model.".to_string()
}

fn default_tools() -> Vec<ToolType> {
    vec![]
}

fn default_max_tool_calls() -> usize {
    10
}

fn default_max_tool_concurrency() -> usize {
    10
}

fn default_cache_enabled() -> bool {
    false
}

fn default_scores() -> HashMap<String, f32> {
    HashMap::from_iter([("A".to_string(), 1.0), ("B".to_string(), 0.0)])
}

fn default_n() -> usize {
    10
}

fn default_threshold() -> f32 {
    0.5
}

fn default_solvers() -> Vec<SolverKind> {
    vec![SolverKind::Similarity(SimilaritySolver {
        prompt: default_consistency_prompt(),
        model_ref: None,
        scores: default_scores(),
    })]
}

fn default_routing_agent_instructions() -> String {
    indoc! {"You are a routing agent. Your job is to route the task to the correct tool. Follow the steps below:
  1. Reasoning the task to find the most relevant tools.
  2. If tools were found that match the query (even partially), USE THEM immediately. Do not ask for clarification.
  3. Only if NO relevant tools are found, explain why.
  4. Synthesize the results from the tool and return it to the user. DO NOT return the raw results from the tool.
  Your task:"
    }
    .to_string()
}

fn default_synthesize_results() -> bool {
    true
}

fn default_agent_public() -> bool {
    true
}

fn default_agent_name() -> String {
    "".to_string()
}

fn default_consistency_prompt() -> String {
    indoc! {"
    You are comparing a pair of submitted answers on a given question. Here is the data:
    [BEGIN DATA]
    ************
    [Question]: {{ task_description }}
    ************
    [Submission 1]: {{submission_1}}
    ************
    [Submission 2]: {{submission_2}}
    ************
    [END DATA]

    Compare the factual content of the submitted answers. Ignore any differences in style, grammar, punctuation. Answer the question by selecting one of the following options:
    A. The submitted answers are either a superset or contains each other and is fully consistent with it.
    B. There is a disagreement between the submitted answers.

    - First, highlight the disagreements between the two submissions.
    Following is the syntax to highlight the differences:

    (1) <factual_content>
    +++ <submission_1_factual_content_diff>
    --- <submission_2_factual_content_diff>

    [BEGIN EXAMPLE]
    Here are the key differences between the two submissions:
    (1) Capital of France
    +++ Paris
    --- France
    [END EXAMPLE]

    - Then reason about the highlighted differences. The submitted answers may either be a subset or superset of each other, or it may conflict. Determine which case applies.
    - At the end, print only a single choice from AB (without quotes or brackets or punctuation) on its own line corresponding to the correct answer. e.g A

    Reasoning:
    "}.to_string()
}

fn default_tests() -> Vec<EvalConfig> {
    vec![]
}

fn default_loop_concurrency() -> usize {
    1
}

fn default_consistency_concurrency() -> usize {
    10
}
