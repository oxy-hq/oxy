use crate::constants::OXY_SDK_SYSTEM_PROMPT;
use crate::types::SemanticQueryParams;
use garde::Validate;
use indoc::indoc;
use itertools::Itertools;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::PartialEq;
use std::collections::HashMap;
use std::hash::Hash;
use std::path::PathBuf;
use utoipa::ToSchema;

pub use variables::Variables;

use super::validate::{AgentValidationContext, validate_model, validate_task};
use crate::adapters::secrets::SecretsManager;
use crate::config::validate::validate_file_path;
use crate::config::validate::{
    ValidationContext, validate_agent_exists, validate_consistency_prompt,
    validate_database_exists, validate_env_var, validate_omni_integration_exists,
    validate_task_data_reference,
};
pub use duckdb::{CatalogConfig, DuckDBOptions, DuckLakeConfig, S3StorageSecret, StorageConfig};
pub use oxy_llm::{
    AnthropicModelConfig, GeminiModelConfig, HeaderValue, Model, OllamaModelConfig,
    OpenAIModelConfig, default_openai_api_url,
};
use oxy_shared::errors::OxyError;
pub use semantics::{SemanticDimension, Semantics};
pub use variables::Variable;
pub use workflow::WorkflowWithRawVariables;

mod duckdb;
mod semantics;
mod variables;
mod workflow;

#[derive(Serialize, Deserialize, Validate, Debug, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct Config {
    #[garde(dive)]
    pub defaults: Option<Defaults>,
    #[garde(custom(validate_models))]
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

    #[serde(default)]
    #[garde(dive)]
    pub slack: Option<SlackSettings>,

    /// Optional MCP configuration for exposing resources as tools
    /// If not specified, all agents and workflows are exposed by default
    #[serde(skip_serializing_if = "Option::is_none")]
    #[garde(dive)]
    pub mcp: Option<McpConfig>,

    /// Optional A2A configuration for exposing agents via A2A protocol
    /// If not specified or empty, no agents are exposed via A2A
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[garde(dive)]
    pub a2a: Option<crate::config::a2a_config::A2aConfig>,
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
    /// Row count threshold for switching to Arrow format (file path response)
    /// If query result exceeds this threshold, return file_path instead of row arrays
    /// Default: 1000 rows
    #[serde(default = "default_arrow_threshold_rows")]
    #[garde(skip)]
    pub arrow_threshold_rows: usize,
}

fn default_arrow_threshold_rows() -> usize {
    1000
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Validate)]
#[garde(context(ValidationContext))]
pub struct OmniTopic {
    #[garde(length(min = 1))]
    pub name: String,
    #[garde(length(min = 1))]
    pub model_id: String,
}

/// Slack integration settings for project-level configuration
#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Validate)]
#[garde(context(ValidationContext))]
pub struct SlackSettings {
    /// Default agent to use for Slack DM conversations
    /// This is used when users message the bot directly via Slack's AI/Agent interface
    #[garde(skip)]
    pub default_agent: String,

    /// Base URL for the Oxy web app (for deep links back to app in Slack messages)
    /// If not specified, "View in Oxy" links will not be included in responses
    #[serde(default)]
    #[garde(skip)]
    pub oxy_app_url: Option<String>,

    /// Bot token for Slack API calls (direct value, not recommended for production)
    #[serde(default)]
    #[garde(skip)]
    #[schemars(skip)]
    pub bot_token: Option<String>,

    /// Environment variable containing the bot token
    #[serde(default)]
    #[garde(skip)]
    pub bot_token_var: Option<String>,

    /// Signing secret for verifying Slack requests (direct value, not recommended for production)
    #[serde(default)]
    #[garde(skip)]
    #[schemars(skip)]
    pub signing_secret: Option<String>,

    /// Environment variable containing the signing secret
    #[serde(default)]
    #[garde(skip)]
    pub signing_secret_var: Option<String>,
}

impl SlackSettings {
    /// Get the bot token, resolving from environment variable if needed
    pub async fn get_bot_token(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        secret_manager
            .resolve_config_value(
                self.bot_token.as_deref(),
                self.bot_token_var.as_deref(),
                "Slack bot_token",
                None,
            )
            .await
    }

    /// Get the signing secret, resolving from environment variable if needed
    pub async fn get_signing_secret(
        &self,
        secret_manager: &SecretsManager,
    ) -> Result<String, OxyError> {
        secret_manager
            .resolve_config_value(
                self.signing_secret.as_deref(),
                self.signing_secret_var.as_deref(),
                "Slack signing_secret",
                None,
            )
            .await
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Validate)]
#[garde(context(ValidationContext))]
pub struct McpConfig {
    /// List of file patterns to expose as MCP tools
    /// Supports glob patterns like "agents/*.agent.yml"
    /// Examples:
    /// - "agents/sql-generator.agent.yml" (specific file)
    /// - "agents/*.agent.yml" (all agents in directory)
    /// - "workflows/**/*.workflow.yml" (all workflows recursively)
    /// - "semantics/topics/*.topic.yml" (semantic topics)
    /// - "sqls/queries/*.sql" (SQL files)
    #[serde(default)]
    #[garde(skip)]
    pub tools: Vec<String>,
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
        secret_manager
            .resolve_config_value(
                self.password.as_deref(),
                self.password_var.as_deref(),
                "password",
                None,
            )
            .await
    }

    pub async fn get_host(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        secret_manager
            .resolve_config_value(
                self.host.as_deref(),
                self.host_var.as_deref(),
                "host",
                Some("localhost"),
            )
            .await
    }

    pub async fn get_port(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        secret_manager
            .resolve_config_value(
                self.port.as_deref(),
                self.port_var.as_deref(),
                "port",
                Some("5432"),
            )
            .await
    }

    pub async fn get_user(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        secret_manager
            .resolve_config_value(
                self.user.as_deref(),
                self.user_var.as_deref(),
                "user",
                Some("postgres"),
            )
            .await
    }

    pub async fn get_database(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        secret_manager
            .resolve_config_value(
                self.database.as_deref(),
                self.database_var.as_deref(),
                "database",
                Some("postgres"),
            )
            .await
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
        secret_manager
            .resolve_config_value(
                self.password.as_deref(),
                self.password_var.as_deref(),
                "password",
                None,
            )
            .await
    }

    pub async fn get_host(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        secret_manager
            .resolve_config_value(
                self.host.as_deref(),
                self.host_var.as_deref(),
                "host",
                Some("localhost"),
            )
            .await
    }

    pub async fn get_port(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        secret_manager
            .resolve_config_value(
                self.port.as_deref(),
                self.port_var.as_deref(),
                "port",
                Some("5439"),
            )
            .await
    }

    pub async fn get_user(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        secret_manager
            .resolve_config_value(
                self.user.as_deref(),
                self.user_var.as_deref(),
                "user",
                Some("awsuser"),
            )
            .await
    }

    pub async fn get_database(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        secret_manager
            .resolve_config_value(
                self.database.as_deref(),
                self.database_var.as_deref(),
                "database",
                Some("dev"),
            )
            .await
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
        secret_manager
            .resolve_config_value(
                self.password.as_deref(),
                self.password_var.as_deref(),
                "password",
                None,
            )
            .await
    }

    pub async fn get_host(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        secret_manager
            .resolve_config_value(
                self.host.as_deref(),
                self.host_var.as_deref(),
                "host",
                Some("localhost"),
            )
            .await
    }

    pub async fn get_port(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        secret_manager
            .resolve_config_value(
                self.port.as_deref(),
                self.port_var.as_deref(),
                "port",
                Some("3306"),
            )
            .await
    }

    pub async fn get_user(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        secret_manager
            .resolve_config_value(
                self.user.as_deref(),
                self.user_var.as_deref(),
                "user",
                Some("root"),
            )
            .await
    }

    pub async fn get_database(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        secret_manager
            .resolve_config_value(
                self.database.as_deref(),
                self.database_var.as_deref(),
                "database",
                Some("mysql"),
            )
            .await
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
        secret_manager
            .resolve_config_value(
                self.password.as_deref(),
                self.password_var.as_deref(),
                "password",
                None,
            )
            .await
    }

    pub async fn get_host(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        secret_manager
            .resolve_config_value(
                self.host.as_deref(),
                self.host_var.as_deref(),
                "ClickHouse host",
                None,
            )
            .await
    }

    pub async fn get_user(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        secret_manager
            .resolve_config_value(
                self.user.as_deref(),
                self.user_var.as_deref(),
                "ClickHouse user",
                None,
            )
            .await
    }

    pub async fn get_database(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        secret_manager
            .resolve_config_value(
                self.database.as_deref(),
                self.database_var.as_deref(),
                "ClickHouse database",
                None,
            )
            .await
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

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Validate, Default)]
#[garde(context(ValidationContext))]
pub struct MotherDuck {
    #[garde(custom(validate_env_var))]
    pub token_var: String,
    #[serde(default)]
    #[garde(skip)]
    pub database: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub schemas: HashMap<String, Vec<String>>,
}

impl MotherDuck {
    pub async fn get_token(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        secret_manager
            .resolve_config_value(None, Some(&self.token_var), "MotherDuck token", None)
            .await
    }
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
    #[serde(default = "default_agent_max_iterations")]
    #[garde(skip)]
    pub max_iterations: usize,

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

    #[serde(flatten)]
    #[garde(skip)]
    pub variables: Option<Variables>,
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
    #[serde(flatten)]
    #[garde(dive)]
    pub options: DuckDBOptions,
}

#[derive(Serialize, Deserialize, Debug, Validate, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
#[serde(untagged)] // Consider using tagged enum here if migrations are possible
pub enum SnowflakeAuthType {
    Password {
        #[garde(length(min = 1))]
        password: String,
    },
    PasswordVar {
        #[garde(length(min = 1))]
        password_var: String,
    },
    PrivateKey {
        #[garde(custom(validate_file_path))]
        private_key_path: PathBuf,
    },
    BrowserAuth {
        #[serde(default = "default_snowflake_browser_timeout")]
        #[garde(skip)]
        browser_timeout_secs: u64, // in seconds
        #[garde(skip)]
        cache_dir: Option<PathBuf>,
    },
}

pub fn default_snowflake_browser_timeout() -> u64 {
    120
}

impl SnowflakeAuthType {
    pub fn get_password(&self) -> Option<&String> {
        match self {
            SnowflakeAuthType::Password { password, .. } => Some(password),
            _ => None,
        }
    }

    pub fn get_password_var(&self) -> Option<&String> {
        match self {
            SnowflakeAuthType::PasswordVar { password_var, .. } => Some(password_var),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Validate, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct Snowflake {
    #[garde(skip)]
    pub account: String,
    #[garde(skip)]
    pub username: String,
    #[garde(skip)]
    pub warehouse: String,
    #[garde(skip)]
    pub database: String,
    #[garde(skip)]
    pub schema: Option<String>,
    #[garde(skip)]
    pub role: Option<String>,
    #[serde(flatten)]
    #[garde(dive)]
    pub auth_type: SnowflakeAuthType,
    #[garde(skip)]
    #[serde(default)]
    pub datasets: HashMap<String, Vec<String>>,
    #[serde(default)]
    #[garde(skip)]
    pub filters: HashMap<String, schemars::schema::SchemaObject>,
}

impl Snowflake {
    pub async fn get_password(&self, secret_manager: &SecretsManager) -> Result<String, OxyError> {
        secret_manager
            .resolve_config_value(
                self.auth_type.get_password().map(|x| x.as_str()),
                self.auth_type.get_password_var().map(|x| x.as_str()),
                "Snowflake password",
                None,
            )
            .await
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
    #[serde(rename = "motherduck")]
    MotherDuck(#[garde(dive)] MotherDuck),
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
            DatabaseType::MotherDuck(_) => write!(f, "motherduck"),
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
            DatabaseType::MotherDuck(_) => "duckdb".to_string(),
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
                    HashMap::from_iter([("".to_string(), vec!["*".to_string()])]) // Default to CORE schema
                } else {
                    sf.datasets.clone()
                }
            }
            DatabaseType::MotherDuck(md) => {
                if md.schemas.is_empty() {
                    HashMap::from_iter([(String::default(), vec!["*".to_string()])])
                } else {
                    md.schemas.clone()
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

/// ClickHouse-specific connection override parameters
///
/// Allows overriding ClickHouse connection parameters at request time.
/// Used primarily by third-party API consumers to dynamically modify connection settings.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct ClickHouseConnectionOverride {
    /// Override the ClickHouse host/URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,

    /// Override the database name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
}

/// Snowflake-specific connection override parameters
///
/// Allows overriding Snowflake connection parameters at request time.
/// Used primarily by third-party API consumers to dynamically modify connection settings.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct SnowflakeConnectionOverride {
    /// Override the database name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,

    /// Override the schema
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    /// Override the warehouse
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warehouse: Option<String>,

    /// Override the account identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
}

/// Database-specific connection override
///
/// Different databases support different override parameters.
/// The connector will deserialize to the appropriate variant based on the database type.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(untagged)]
pub enum ConnectionOverride {
    ClickHouse(ClickHouseConnectionOverride),
    Snowflake(SnowflakeConnectionOverride),
}

impl TryFrom<ConnectionOverride> for ClickHouseConnectionOverride {
    type Error = oxy_shared::errors::OxyError;
    fn try_from(ovr: ConnectionOverride) -> Result<Self, Self::Error> {
        let ConnectionOverride::ClickHouse(ch) = ovr else {
            return Err(oxy_shared::errors::OxyError::ConfigurationError(
                "Invalid override type for ClickHouse".into(),
            ));
        };
        Ok(ch)
    }
}

impl TryFrom<ConnectionOverride> for SnowflakeConnectionOverride {
    type Error = oxy_shared::errors::OxyError;
    fn try_from(ovr: ConnectionOverride) -> Result<Self, Self::Error> {
        let ConnectionOverride::Snowflake(sf) = ovr else {
            return Err(oxy_shared::errors::OxyError::ConfigurationError(
                "Invalid override type for Snowflake".into(),
            ));
        };
        Ok(sf)
    }
}

/// Map of database name to connection overrides
///
/// Keys should match database names defined in config.yml under the `databases` section.
/// This allows API requests to override connection parameters for specific databases
/// without modifying the base configuration.
///
/// The override structure depends on the database type - the connector will automatically
/// deserialize to the correct variant based on the database configuration.
pub type ConnectionOverrides = HashMap<String, ConnectionOverride>;

/// Validate a list of models
fn validate_models(models: &Vec<Model>, ctx: &ValidationContext) -> garde::Result {
    for (i, model) in models.iter().enumerate() {
        match model {
            Model::OpenAI { config } => {
                if config.name.is_empty() {
                    return Err(garde::Error::new(format!(
                        "models[{}].name: length is lower than 1",
                        i
                    )));
                }
                if config.model_ref.is_empty() {
                    return Err(garde::Error::new(format!(
                        "models[{}].model_ref: length is lower than 1",
                        i
                    )));
                }
                validate_env_var(&config.key_var, ctx)
                    .map_err(|e| garde::Error::new(format!("models[{}].key_var: {}", i, e)))?;
            }
            Model::Google { config } => {
                if config.name.is_empty() {
                    return Err(garde::Error::new(format!(
                        "models[{}].name: length is lower than 1",
                        i
                    )));
                }
                if config.model_ref.is_empty() {
                    return Err(garde::Error::new(format!(
                        "models[{}].model_ref: length is lower than 1",
                        i
                    )));
                }
                validate_env_var(&config.key_var, ctx)
                    .map_err(|e| garde::Error::new(format!("models[{}].key_var: {}", i, e)))?;
            }
            Model::Ollama { config } => {
                if config.name.is_empty() {
                    return Err(garde::Error::new(format!(
                        "models[{}].name: length is lower than 1",
                        i
                    )));
                }
                if config.model_ref.is_empty() {
                    return Err(garde::Error::new(format!(
                        "models[{}].model_ref: length is lower than 1",
                        i
                    )));
                }
                if config.api_key.is_empty() {
                    return Err(garde::Error::new(format!(
                        "models[{}].api_key: length is lower than 1",
                        i
                    )));
                }
                if config.api_url.is_empty() {
                    return Err(garde::Error::new(format!(
                        "models[{}].api_url: length is lower than 1",
                        i
                    )));
                }
            }
            Model::Anthropic { config } => {
                if config.name.is_empty() {
                    return Err(garde::Error::new(format!(
                        "models[{}].name: length is lower than 1",
                        i
                    )));
                }
                if config.model_ref.is_empty() {
                    return Err(garde::Error::new(format!(
                        "models[{}].model_ref: length is lower than 1",
                        i
                    )));
                }
                validate_env_var(&config.key_var, ctx)
                    .map_err(|e| garde::Error::new(format!("models[{}].key_var: {}", i, e)))?;
            }
        }
    }
    Ok(())
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

    #[garde(skip)]
    pub variables: Option<HashMap<String, Value>>,

    /// Custom consistency evaluation prompt for this specific task
    /// Overrides workflow-level consistency_prompt if specified
    #[garde(custom(validate_consistency_prompt))]
    pub consistency_prompt: Option<String>,

    #[garde(dive)]
    pub export: Option<TaskExport>,
}

impl Hash for AgentTask {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.agent_ref.hash(state);
        self.prompt.hash(state);
        if let Some(ref vars) = self.variables {
            for (key, value) in vars.iter().sorted_by_cached_key(|(key, _)| *key) {
                key.hash(state);
                value.hash(state);
            }
        }
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

    // Optional variables for semantic layer expressions
    #[garde(skip)]
    #[serde(default)]
    pub variables: Option<HashMap<String, Value>>,
}

impl Hash for SemanticQueryTask {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.query.hash(state);
        // Variables affect query results, so include them in hash
        if let Some(variables) = &self.variables {
            for (key, value) in variables {
                key.hash(state);
                value.to_string().hash(state); // Hash the JSON string representation
            }
        }
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

// Custom schema functions for JSON values to ensure OpenAI compatibility
fn json_value_schema(_gen: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
    use schemars::schema::{InstanceType, Schema, SchemaObject};
    Schema::Object(SchemaObject {
        instance_type: Some(
            vec![
                InstanceType::String,
                InstanceType::Number,
                InstanceType::Boolean,
                InstanceType::Null,
            ]
            .into(),
        ),
        ..Default::default()
    })
}

fn json_value_array_schema(
    _gen: &mut schemars::r#gen::SchemaGenerator,
) -> schemars::schema::Schema {
    use schemars::schema::{ArrayValidation, InstanceType, Schema, SchemaObject, SingleOrVec};
    Schema::Object(SchemaObject {
        instance_type: Some(vec![InstanceType::Array].into()),
        array: Some(Box::new(ArrayValidation {
            items: Some(SingleOrVec::Single(Box::new(Schema::Object(
                SchemaObject {
                    instance_type: Some(
                        vec![
                            InstanceType::String,
                            InstanceType::Number,
                            InstanceType::Boolean,
                            InstanceType::Null,
                        ]
                        .into(),
                    ),
                    ..Default::default()
                },
            )))),
            ..Default::default()
        })),
        ..Default::default()
    })
}

/// Scalar comparison filter (eq, neq, gt, gte, lt, lte)
#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema, ToSchema)]
#[garde(context(ValidationContext))]
pub struct ScalarFilter {
    #[garde(skip)]
    #[schemars(
        schema_with = "json_value_schema",
        description = "The value to compare. Can be a string, number, boolean, or null."
    )]
    pub value: Value,
}

/// Array-based filter (in, not_in)
#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema, ToSchema)]
#[garde(context(ValidationContext))]
pub struct ArrayFilter {
    #[garde(skip)]
    #[schemars(
        schema_with = "json_value_array_schema",
        description = "Array of values to filter by. Each value can be a string, number, boolean, or null."
    )]
    pub values: Vec<Value>,
}

/// Date range filter (in_date_range, not_in_date_range)
#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema, ToSchema)]
#[garde(context(ValidationContext))]
pub struct DateRangeFilter {
    #[garde(skip)]
    #[schemars(
        schema_with = "json_value_schema",
        description = "Start of the date range. Can be a string (ISO date or relative like 'today', '7 days ago'), number (timestamp), or null."
    )]
    pub from: Value,
    #[garde(skip)]
    #[schemars(
        schema_with = "json_value_schema",
        description = "End of the date range. Can be a string (ISO date or relative like 'today', '7 days ago'), number (timestamp), or null."
    )]
    pub to: Value,
}

impl DateRangeFilter {
    /// Convert relative date expressions to ISO datetime strings
    /// Supports natural language expressions like "now", "today", "7 days ago", "last week", etc.
    /// Uses chrono-english for robust natural language parsing.
    pub fn resolve_relative_dates(&self) -> Result<Self, oxy_shared::errors::OxyError> {
        Ok(Self {
            from: Self::resolve_date_value(&self.from)?,
            to: Self::resolve_date_value(&self.to)?,
        })
    }

    fn resolve_date_value(value: &Value) -> Result<Value, oxy_shared::errors::OxyError> {
        match value {
            Value::String(s) => {
                let resolved = Self::parse_relative_date(s)?;
                Ok(Value::String(resolved))
            }
            other => Ok(other.clone()),
        }
    }

    fn parse_relative_date(expr: &str) -> Result<String, oxy_shared::errors::OxyError> {
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
#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema, ToSchema)]
#[garde(context(ValidationContext))]
#[serde(tag = "op")]
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

#[derive(Serialize, Deserialize, Debug, Clone, Validate, ToSchema)]
#[garde(context(ValidationContext))]
pub struct SemanticFilter {
    #[garde(length(min = 1))]
    pub field: String,
    #[serde(flatten)]
    #[garde(dive)]
    pub filter_type: SemanticFilterType,
}

// Custom JSON schema implementation to flatten filter_type variants
// That produce JSON schema compatible with OpenAI
impl JsonSchema for SemanticFilter {
    fn schema_name() -> String {
        "SemanticFilter".to_string()
    }

    fn json_schema(r#gen: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::{InstanceType, Metadata, Schema, SchemaObject, SubschemaValidation};

        // Generate the full schema for SemanticFilterType first
        // This will add it to the definitions and return a reference
        let _filter_ref = r#gen.subschema_for::<SemanticFilterType>();

        // Get the actual schema from the definitions
        let definitions = r#gen.definitions();
        let filter_type_schema = definitions.get("SemanticFilterType").cloned();

        // Use anyOf to combine the field with filter_type variants
        // Since filter_type is flattened, we need to merge it at the top level
        let mut subschemas = Vec::new();

        // Extract the oneOf variants from SemanticFilterType and convert to anyOf
        if let Some(Schema::Object(filter_obj)) = filter_type_schema
            && let Some(subschema_validation) = &filter_obj.subschemas
            && let Some(one_of) = &subschema_validation.one_of
        {
            // For each variant in oneOf, create an anyOf schema that includes the field property
            for variant in one_of {
                let mut combined = SchemaObject::default();
                combined.instance_type = Some(InstanceType::Object.into());

                // Add field property to each variant
                let mut field_schema_clone = SchemaObject::default();
                field_schema_clone.instance_type = Some(InstanceType::String.into());
                field_schema_clone.metadata = Some(Box::new(Metadata {
                            description: Some("The measure/dimension to apply the filter on. Must by full name: <view_name>.<field_name>".to_string()),
                            ..Default::default()
                        }));

                combined
                    .object()
                    .properties
                    .insert("field".to_string(), Schema::Object(field_schema_clone));
                combined.object().required.insert("field".to_string());

                // Merge the filter_type variant properties
                if let Schema::Object(variant_obj) = variant
                    && let Some(props) = &variant_obj.object
                {
                    for (key, value) in &props.properties {
                        combined
                            .object()
                            .properties
                            .insert(key.clone(), value.clone());
                    }
                    for req in &props.required {
                        combined.object().required.insert(req.clone());
                    }
                }

                subschemas.push(Schema::Object(combined));
            }
        }

        // Return a schema with anyOf at the top level
        let mut schema = SchemaObject::default();
        schema.subschemas = Some(Box::new(SubschemaValidation {
            any_of: Some(subschemas),
            ..Default::default()
        }));

        Schema::Object(schema)
    }
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
    pub query: crate::types::tool_params::OmniQueryParams,
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
    pub consistency_prompt: Option<String>,
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
    /// Global consistency evaluation prompt for all agent tasks in this workflow
    /// This can be overridden per-task via AgentTask.consistency_prompt
    #[garde(custom(validate_consistency_prompt))]
    pub consistency_prompt: Option<String>,
}

fn default_is_verified() -> bool {
    true
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
    #[serde(skip_serializing, default = "default_is_verified")]
    #[schemars(skip)]
    pub is_verified: bool,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct AgentTool {
    pub name: String,
    pub description: String,
    pub agent_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<Variables>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<HashMap<String, Value>>,

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<HashMap<String, Value>>,
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
pub struct CreateV0AppTool {
    pub name: String,
    #[serde(default = "default_create_v0_app_tool_description")]
    pub description: String,
    #[serde(default = "default_create_v0_app_tool_system_instruction")]
    pub system_instruction: String,
    #[serde(default = "default_github_repo")]
    pub github_repo: Option<String>,
    #[serde(default = "default_oxy_api_key_var")]
    pub oxy_api_key_var: String,
    #[serde(default = "default_v0_api_key_var")]
    pub v0_api_key_var: String,
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

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Validate, ToSchema)]
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

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Validate, ToSchema)]
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

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Validate, ToSchema)]
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
    #[serde(rename = "create_v0_app")]
    CreateV0App(CreateV0AppTool),
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

impl ToolType {
    /// Render tool configuration with variable substitution using the provided renderer
    pub async fn render(
        &self,
        renderer: &crate::execute::renderer::Renderer,
    ) -> Result<Self, oxy_shared::errors::OxyError> {
        use oxy_shared::errors::OxyError;

        Ok(match self {
            ToolType::ExecuteSQL(tool) => {
                // Register and render description
                renderer.register_template(&tool.description)?;
                let rendered_description =
                    renderer
                        .render_async(&tool.description)
                        .await
                        .map_err(|e| {
                            OxyError::RuntimeError(format!(
                                "Failed to render ExecuteSQL description: {}",
                                e
                            ))
                        })?;
                // Register and render database
                renderer.register_template(&tool.database)?;
                let rendered_database =
                    renderer.render_async(&tool.database).await.map_err(|e| {
                        OxyError::RuntimeError(format!(
                            "Failed to render ExecuteSQL database: {}",
                            e
                        ))
                    })?;

                // Render variables if present
                let rendered_variables = Self::render_variables(&tool.variables, renderer).await?;

                ToolType::ExecuteSQL(ExecuteSQLTool {
                    name: tool.name.clone(),
                    description: rendered_description,
                    database: rendered_database,
                    dry_run_limit: tool.dry_run_limit,
                    variables: rendered_variables,
                    sql: tool.sql.clone(),
                })
            }
            ToolType::ValidateSQL(tool) => {
                // Register and render description
                renderer.register_template(&tool.description)?;
                let rendered_description =
                    renderer
                        .render_async(&tool.description)
                        .await
                        .map_err(|e| {
                            OxyError::RuntimeError(format!(
                                "Failed to render ValidateSQL description: {}",
                                e
                            ))
                        })?;
                // Register and render database
                renderer.register_template(&tool.database)?;
                let rendered_database =
                    renderer.render_async(&tool.database).await.map_err(|e| {
                        OxyError::RuntimeError(format!(
                            "Failed to render ValidateSQL database: {}",
                            e
                        ))
                    })?;

                ToolType::ValidateSQL(ValidateSQLTool {
                    name: tool.name.clone(),
                    description: rendered_description,
                    database: rendered_database,
                })
            }
            ToolType::SemanticQuery(tool) => {
                // Register and render description
                renderer.register_template(&tool.description)?;
                let rendered_description =
                    renderer
                        .render_async(&tool.description)
                        .await
                        .map_err(|e| {
                            OxyError::RuntimeError(format!(
                                "Failed to render SemanticQuery description: {}",
                                e
                            ))
                        })?;
                let rendered_topic = if let Some(topic) = &tool.topic {
                    renderer.register_template(topic)?;
                    Some(renderer.render_async(topic).await.map_err(|e| {
                        OxyError::RuntimeError(format!(
                            "Failed to render SemanticQuery topic: {}",
                            e
                        ))
                    })?)
                } else {
                    None
                };

                // Render variables if present
                let rendered_variables = Self::render_variables(&tool.variables, renderer).await?;

                ToolType::SemanticQuery(SemanticQueryTool {
                    name: tool.name.clone(),
                    description: rendered_description,
                    dry_run_limit: tool.dry_run_limit,
                    topic: rendered_topic,
                    variables: rendered_variables,
                })
            }
            ToolType::OmniQuery(tool) => {
                // Register and render description
                renderer.register_template(&tool.description)?;
                let rendered_description =
                    renderer
                        .render_async(&tool.description)
                        .await
                        .map_err(|e| {
                            OxyError::RuntimeError(format!(
                                "Failed to render OmniQuery description: {}",
                                e
                            ))
                        })?;
                // Register and render topic
                renderer.register_template(&tool.topic)?;
                let rendered_topic = renderer.render_async(&tool.topic).await.map_err(|e| {
                    OxyError::RuntimeError(format!("Failed to render OmniQuery topic: {}", e))
                })?;
                // Register and render integration
                renderer.register_template(&tool.integration)?;
                let rendered_integration =
                    renderer
                        .render_async(&tool.integration)
                        .await
                        .map_err(|e| {
                            OxyError::RuntimeError(format!(
                                "Failed to render OmniQuery integration: {}",
                                e
                            ))
                        })?;

                ToolType::OmniQuery(OmniQueryTool {
                    name: tool.name.clone(),
                    description: rendered_description,
                    topic: rendered_topic,
                    integration: rendered_integration,
                })
            }
            ToolType::Workflow(tool) => {
                // Register and render description
                renderer.register_template(&tool.description)?;
                let rendered_description =
                    renderer
                        .render_async(&tool.description)
                        .await
                        .map_err(|e| {
                            OxyError::RuntimeError(format!(
                                "Failed to render Workflow description: {}",
                                e
                            ))
                        })?;
                // Register and render workflow_ref
                renderer.register_template(&tool.workflow_ref)?;
                let rendered_workflow_ref = renderer
                    .render_async(&tool.workflow_ref)
                    .await
                    .map_err(|e| {
                        OxyError::RuntimeError(format!("Failed to render Workflow ref: {}", e))
                    })?;

                // Variables in WorkflowTool are passed through, not rendered here
                ToolType::Workflow(WorkflowTool {
                    name: tool.name.clone(),
                    description: rendered_description,
                    workflow_ref: rendered_workflow_ref,
                    variables: tool.variables.clone(),
                    output_task_ref: tool.output_task_ref.clone(),
                    is_verified: tool.is_verified,
                })
            }
            ToolType::Agent(tool) => {
                // Register and render description
                renderer.register_template(&tool.description)?;
                let rendered_description =
                    renderer
                        .render_async(&tool.description)
                        .await
                        .map_err(|e| {
                            OxyError::RuntimeError(format!(
                                "Failed to render Agent description: {}",
                                e
                            ))
                        })?;
                // Register and render agent_ref
                renderer.register_template(&tool.agent_ref)?;
                let rendered_agent_ref =
                    renderer.render_async(&tool.agent_ref).await.map_err(|e| {
                        OxyError::RuntimeError(format!("Failed to render Agent ref: {}", e))
                    })?;

                // Render variables if present
                let rendered_variables =
                    Self::render_variables_schema(&tool.variables, renderer).await?;

                ToolType::Agent(AgentTool {
                    name: tool.name.clone(),
                    description: rendered_description,
                    agent_ref: rendered_agent_ref,
                    variables: rendered_variables,
                    is_verified: tool.is_verified,
                })
            }
            ToolType::Retrieval(tool) => {
                // Register and render description
                renderer.register_template(&tool.description)?;
                let rendered_description =
                    renderer
                        .render_async(&tool.description)
                        .await
                        .map_err(|e| {
                            OxyError::RuntimeError(format!(
                                "Failed to render Retrieval description: {}",
                                e
                            ))
                        })?;

                ToolType::Retrieval(RetrievalConfig {
                    name: tool.name.clone(),
                    description: rendered_description,
                    src: tool.src.clone(),
                    api_url: tool.api_url.clone(),
                    api_key: tool.api_key.clone(),
                    key_var: tool.key_var.clone(),
                    embedding_config: tool.embedding_config.clone(),
                    db_config: tool.db_config.clone(),
                })
            }
            ToolType::Visualize(tool) => {
                // Register and render description
                renderer.register_template(&tool.description)?;
                let rendered_description =
                    renderer
                        .render_async(&tool.description)
                        .await
                        .map_err(|e| {
                            OxyError::RuntimeError(format!(
                                "Failed to render Visualize description: {}",
                                e
                            ))
                        })?;

                ToolType::Visualize(VisualizeTool {
                    name: tool.name.clone(),
                    description: rendered_description,
                })
            }
            ToolType::CreateDataApp(tool) => {
                // Register and render description
                renderer.register_template(&tool.description)?;
                let rendered_description =
                    renderer
                        .render_async(&tool.description)
                        .await
                        .map_err(|e| {
                            OxyError::RuntimeError(format!(
                                "Failed to render CreateDataApp description: {}",
                                e
                            ))
                        })?;

                ToolType::CreateDataApp(CreateDataAppTool {
                    name: tool.name.clone(),
                    description: rendered_description,
                })
            }
            ToolType::CreateV0App(tool) => {
                // Register and render description
                renderer.register_template(&tool.description)?;
                let rendered_description =
                    renderer
                        .render_async(&tool.description)
                        .await
                        .map_err(|e| {
                            OxyError::RuntimeError(format!(
                                "Failed to render CreateV0AppTool description: {}",
                                e
                            ))
                        })?;

                ToolType::CreateV0App(CreateV0AppTool {
                    name: tool.name.clone(),
                    description: rendered_description,
                    system_instruction: tool.system_instruction.clone(),
                    github_repo: tool.github_repo.clone(),
                    oxy_api_key_var: tool.oxy_api_key_var.clone(),
                    v0_api_key_var: tool.v0_api_key_var.clone(),
                })
            }
        })
    }

    /// Helper method to render variables with the renderer
    async fn render_variables(
        variables: &Option<HashMap<String, Value>>,
        renderer: &crate::execute::renderer::Renderer,
    ) -> Result<Option<HashMap<String, Value>>, oxy_shared::errors::OxyError> {
        use oxy_shared::errors::OxyError;

        if let Some(vars) = variables {
            let mut rendered_vars = HashMap::new();
            for (key, value) in vars {
                // Convert to string, render, then parse back to Value
                let value_str = if value.is_string() {
                    value.as_str().unwrap_or_default().to_string()
                } else {
                    serde_json::to_string(value).map_err(|e| {
                        OxyError::RuntimeError(format!(
                            "Failed to serialize variable {}: {}",
                            key, e
                        ))
                    })?
                };

                // Render inline template expression
                let rendered = renderer.render_async(&value_str).await.map_err(|e| {
                    OxyError::RuntimeError(format!("Failed to render variable {}: {}", key, e))
                })?;

                // Try to parse back as JSON, otherwise keep as string
                let rendered_value =
                    serde_json::from_str(&rendered).unwrap_or(serde_json::Value::String(rendered));

                rendered_vars.insert(key.clone(), rendered_value);
            }
            Ok(Some(rendered_vars))
        } else {
            Ok(None)
        }
    }

    /// Helper method to render Variables (schema-based) with the renderer
    async fn render_variables_schema(
        variables: &Option<Variables>,
        renderer: &crate::execute::renderer::Renderer,
    ) -> Result<Option<Variables>, oxy_shared::errors::OxyError> {
        use oxy_shared::errors::OxyError;

        if let Some(vars) = variables {
            // Variables contains SchemaObject with default values, not runtime values
            // We only need to render the default values in the schema metadata
            let mut rendered_schemas = HashMap::new();

            for (key, schema) in &vars.variables {
                let mut new_schema = schema.clone();

                // If there's a default value in metadata, render it
                if let Some(metadata) = &schema.metadata
                    && let Some(default_value) = &metadata.default
                {
                    let value_str = if default_value.is_string() {
                        default_value.as_str().unwrap_or_default().to_string()
                    } else {
                        serde_json::to_string(default_value).map_err(|e| {
                            OxyError::RuntimeError(format!(
                                "Failed to serialize variable {}: {}",
                                key, e
                            ))
                        })?
                    };

                    // Render inline template expression
                    let rendered = renderer.render_async(&value_str).await.map_err(|e| {
                        OxyError::RuntimeError(format!("Failed to render variable {}: {}", key, e))
                    })?;

                    // Try to parse back as JSON, otherwise keep as string
                    let rendered_value = serde_json::from_str(&rendered)
                        .unwrap_or(serde_json::Value::String(rendered));

                    // Update the metadata with rendered default value
                    let mut new_metadata = (**metadata).clone();
                    new_metadata.default = Some(rendered_value);
                    new_schema.metadata = Some(Box::new(new_metadata));
                }

                rendered_schemas.insert(key.clone(), new_schema);
            }

            Ok(Some(Variables {
                variables: rendered_schemas,
            }))
        } else {
            Ok(None)
        }
    }
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
    "Query the database via the semantic layer.".to_string()
}

fn default_validate_sql_tool_description() -> String {
    "Validate the SQL query. If the query is invalid, fix it and run again.".to_string()
}

fn default_create_data_app_tool_description() -> String {
    "Create a data app/dashboard to visualize metrics.".to_string()
}
fn default_create_v0_app_tool_system_instruction() -> String {
    OXY_SDK_SYSTEM_PROMPT.to_string()
}

fn default_create_v0_app_tool_description() -> String {
    "Use this when user wants to build interactive UIs, dashboards, or data visualizations. The app will be deployed and can query Oxy tables via SDK. Make sure to persist the execute sql in order to use it with Oxy SDK.".to_string()
}

fn default_github_repo() -> Option<String> {
    Some("https://github.com/oxy-hq/data-app-template".to_string())
}

fn default_oxy_api_key_var() -> String {
    "OXY_API_KEY".to_string()
}

fn default_v0_api_key_var() -> String {
    "V0_API_KEY".to_string()
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

fn default_agent_max_iterations() -> usize {
    15
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

fn default_v0_api_url() -> String {
    "https://api.v0.dev".to_string()
}

fn default_v0_key_var() -> String {
    "V0_API_KEY".to_string()
}

#[cfg(test)]
mod tests {
    use schemars::schema_for;

    #[test]
    fn test_semantic_query_params_schema() {
        use crate::service::types::SemanticQueryParams;
        let schema = schema_for!(SemanticQueryParams);
        let json = serde_json::to_string_pretty(&schema).unwrap();
        println!("\n{}\n", json);

        // Verify that the schema doesn't have "items": true or "value": true
        assert!(
            !json.contains(r#""items": true"#),
            "Schema should not contain 'items': true"
        );
        assert!(
            !json.contains(r#""value": true"#),
            "Schema should not contain 'value': true"
        );
        assert!(
            !json.contains(r#""from": true"#),
            "Schema should not contain 'from': true"
        );
        assert!(
            !json.contains(r#""to": true"#),
            "Schema should not contain 'to': true"
        );
    }
}
