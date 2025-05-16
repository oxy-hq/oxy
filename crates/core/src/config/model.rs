use garde::Validate;
use indoc::indoc;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::skip_serializing_none;
use std::cmp::PartialEq;
use std::collections::HashMap;
use std::hash::Hash;
use std::path::PathBuf;
use std::{env, fs};
pub use variables::Variables;

use crate::config::validate::validate_file_path;
use crate::config::validate::{
    ValidationContext, validate_agent_exists, validate_database_exists, validate_env_var,
};
use crate::errors::OxyError;
use crate::utils::list_by_sub_extension;
use schemars::JsonSchema;

use super::validate::{AgentValidationContext, validate_model, validate_task};

mod variables;

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
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct SemanticModels {
    pub table: String,
    pub database: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<Entity>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dimensions: Vec<Dimension>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub measures: Vec<Measure>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct Entity {
    pub name: String,
    pub description: String,
    pub sample: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct Dimension {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synonyms: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sample: Vec<String>,
    #[serde(rename = "type", alias = "type")]
    pub data_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_partition_key: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct Measure {
    pub name: String,
    pub sql: String,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Validate, Default)]
#[garde(context(ValidationContext))]
pub struct Postgres {
    #[garde(length(min = 1))]
    #[serde(default)]
    pub host: Option<String>,
    #[garde(length(min = 1))]
    #[serde(default)]
    pub port: Option<String>,
    #[garde(length(min = 1))]
    #[serde(default)]
    pub user: Option<String>,
    #[garde(skip)]
    #[schemars(skip)]
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub password_var: Option<String>,
    #[garde(length(min = 1))]
    #[serde(default)]
    pub database: Option<String>,
}

impl Postgres {
    pub fn get_password(&self) -> Option<String> {
        if let Some(password) = &self.password {
            if !password.is_empty() {
                return Some(password.clone());
            }
        }
        if let Some(password_var) = &self.password_var {
            return env::var(password_var).ok();
        }
        None
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Validate, Default)]
#[garde(context(ValidationContext))]
pub struct Redshift {
    #[garde(length(min = 1))]
    #[serde(default)]
    pub host: Option<String>,
    #[garde(length(min = 1))]
    #[serde(default)]
    pub port: Option<String>,
    #[garde(length(min = 1))]
    #[serde(default)]
    pub user: Option<String>,
    #[garde(skip)]
    #[schemars(skip)]
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub password_var: Option<String>,
    #[garde(length(min = 1))]
    #[serde(default)]
    pub database: Option<String>,
}

impl Redshift {
    pub fn get_password(&self) -> Option<String> {
        if let Some(password) = &self.password {
            if !password.is_empty() {
                return Some(password.clone());
            }
        }
        if let Some(password_var) = &self.password_var {
            return env::var(password_var).ok();
        }
        None
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Validate, Default)]
#[garde(context(ValidationContext))]
pub struct Mysql {
    #[garde(length(min = 1))]
    #[serde(default)]
    pub host: Option<String>,
    #[garde(length(min = 1))]
    #[serde(default)]
    pub port: Option<String>,
    #[garde(length(min = 1))]
    #[serde(default)]
    pub user: Option<String>,
    #[garde(skip)]
    #[schemars(skip)]
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub password_var: Option<String>,
    #[garde(length(min = 1))]
    #[serde(default)]
    pub database: Option<String>,
}

impl Mysql {
    pub fn get_password(&self) -> Option<String> {
        if let Some(password) = &self.password {
            if !password.is_empty() {
                return Some(password.clone());
            }
        }
        if let Some(password_var) = &self.password_var {
            return env::var(password_var).ok();
        }
        None
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Validate, Default)]
#[garde(context(ValidationContext))]
pub struct ClickHouse {
    #[serde(default)]
    #[garde(length(min = 1))]
    pub host: String,
    #[serde(default)]
    #[garde(length(min = 1))]
    pub user: String,
    #[serde(default)]
    #[garde(skip)]
    #[schemars(skip)]
    pub password: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub password_var: Option<String>,
    #[serde(default)]
    #[garde(length(min = 1))]
    pub database: String,
    #[serde(default)]
    #[garde(skip)]
    pub schemas: HashMap<String, Vec<String>>,
}

impl ClickHouse {
    pub fn get_password(&self) -> Option<String> {
        if let Some(password) = &self.password {
            if !password.is_empty() {
                return Some(password.clone());
            }
        }
        if let Some(password_var) = &self.password_var {
            return env::var(password_var).ok();
        }
        None
    }
}

pub fn validate_agent_config() {}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Validate)]
#[garde(context(AgentValidationContext))]
pub struct AgentConfig {
    #[serde(skip)]
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
    #[garde(custom(validate_file_path))]
    pub key_path: PathBuf,
    #[garde(length(min = 1))]
    pub dataset: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub datasets: HashMap<String, Vec<String>>,
    #[garde(range(min = 1))]
    pub dry_run_limit: Option<u64>,
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
    pub password_var: String,
    #[garde(skip)]
    pub warehouse: String,
    #[garde(skip)]
    pub database: String,
    #[garde(skip)]
    pub role: Option<String>,
}

impl Snowflake {
    pub fn get_password(&self) -> Option<String> {
        if let Some(password) = &self.password {
            if !password.is_empty() {
                return Some(password.clone());
            }
        }
        if !self.password_var.is_empty() {
            return env::var(&self.password_var).ok();
        }
        None
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
            DatabaseType::ClickHouse(ch) => match ch.schemas.is_empty() {
                true => HashMap::from_iter([(String::new(), vec!["*".to_string()])]),
                false => ch.schemas.clone(),
            },
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

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Hash)]
#[serde(untagged)]
pub enum LoopValues {
    Template(String),
    Array(Vec<String>),
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
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct Task {
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

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
pub struct OmniTopicJoinItem {}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct OmniTopic {
    pub base_view: String,
    pub label: Option<String>,
    pub fields: Vec<String>,
    pub joins: HashMap<String, OmniTopicJoinItem>,
    pub default_filters: Option<HashMap<String, OmniFilter>>,
}

impl OmniTopic {
    pub fn get_pattern_priority(&self, pattern: &str) -> u8 {
        if "all_views.*" == pattern {
            return 1;
        }
        if pattern.starts_with("tag:") {
            return 3;
        }
        let parts = pattern.split('.').collect::<Vec<_>>();
        if parts.len() == 2 && parts[1] == "*" {
            return 2;
        }

        4
    }
    pub fn get_sorted_field_patterns(&self) -> Vec<String> {
        let mut sorted_fields = self.fields.clone();

        sorted_fields.sort_by(|a, b| {
            let a_priority = self.get_pattern_priority(a);
            let b_priority = self.get_pattern_priority(b);
            a_priority.cmp(&b_priority)
        });
        sorted_fields
    }
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
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct AgentTool {
    pub name: String,
    pub description: String,
    pub agent_ref: String,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct EmbeddingConfig {
    #[serde(default = "default_embed_table")]
    pub table: String,
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
pub struct ExecuteOmniTool {
    pub name: String,
    #[serde(default = "default_omni_tool_description")]
    pub description: String,
    pub model_path: PathBuf,
    pub database: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OmniDimension {
    pub sql: Option<String>,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum AggregateType {
    #[serde(rename = "count")]
    Count,
    #[serde(rename = "sum")]
    Sum,
    #[serde(rename = "average")]
    Avg,
    #[serde(rename = "min")]
    Min,
    #[serde(rename = "max")]
    Max,
    #[serde(rename = "average_distinct_on")]
    AverageDistinctOn,
    #[serde(rename = "median_distinct_on")]
    MedianDistinctOn,
    #[serde(rename = "count_distinct")]
    CountDistinct,
    #[serde(rename = "sum_distinct_on")]
    SumDistinctOn,
    #[serde(rename = "median")]
    Median,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(untagged)]
pub enum OmniFilter {
    Is(OmniIsFilter),
    Not(OmniNotFilter),
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct OmniIsFilter {
    pub is: Option<OmniFilterValue>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct OmniNotFilter {
    pub not: Option<OmniFilterValue>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(untagged)]
pub enum OmniFilterValue {
    String(String),
    Array(Vec<OmniFilterValue>),
    Int(i64),
    Bool(bool),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OmniMeasure {
    pub sql: Option<String>,
    pub description: Option<String>,
    pub aggregate_type: Option<AggregateType>,
    pub filters: Option<HashMap<String, OmniFilter>>,
    pub custom_primary_key_sql: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OmniView {
    pub schema: String,
    #[serde(flatten)]
    pub view_type: OmniViewType,
    pub dimensions: HashMap<String, OmniDimension>,
    #[serde(default)]
    pub measures: HashMap<String, OmniMeasure>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum OmniViewType {
    Table(OmniTableView),
    Query(OmniQueryView),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OmniTableView {
    pub table_name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OmniQueryView {
    pub sql: String,
}

impl OmniView {
    pub fn get_field(&self, field_name: &str) -> Option<OmniField> {
        if let Some(dimension) = self.dimensions.get(field_name) {
            return Some(OmniField::Dimension(dimension.clone()));
        }
        if let Some(measure) = self.measures.get(field_name) {
            return Some(OmniField::Measure(measure.clone()));
        }
        None
    }

    pub fn get_full_field_name(&self, field_name: &str, view_name: &str) -> String {
        match self.view_type.clone() {
            OmniViewType::Table(v) => {
                format!("{}.{}", v.table_name, field_name)
            }
            OmniViewType::Query(_) => {
                format!("{}.{}", view_name, field_name)
            }
        }
    }

    pub fn get_all_fields(&self) -> HashMap<String, OmniField> {
        let mut fields = HashMap::new();
        for (name, dimension) in &self.dimensions {
            fields.insert(name.to_string(), OmniField::Dimension(dimension.clone()));
        }
        for (name, measure) in &self.measures {
            fields.insert(name.to_string(), OmniField::Measure(measure.clone()));
        }
        fields
    }

    pub fn get_table_name(&self, view_name: &str) -> String {
        match &self.view_type {
            OmniViewType::Table(view) => {
                format!("{}.{}", self.schema, view.table_name)
            }
            OmniViewType::Query(v) => {
                format!("({}) as {}", v.sql, view_name)
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OmniSemanticModel {
    pub views: HashMap<String, OmniView>,
    pub topics: HashMap<String, OmniTopic>,
    pub relationships: Vec<OmniRelationShip>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum OmniField {
    Dimension(OmniDimension),
    Measure(OmniMeasure),
}

impl OmniSemanticModel {
    pub fn get_description(&self) -> String {
        "Execute query on the database through the semantic layer.".to_string()
    }

    pub fn get_fields_by_pattern(
        &self,
        pattern: &str,
    ) -> anyhow::Result<HashMap<String, OmniField>> {
        if pattern == "all_views.*" {
            return Ok(self.get_all_fields());
        }

        if pattern.starts_with("tag:") {
            // TODO: implement tag based field retrieval
            return Ok(HashMap::new());
        }

        let (view_name, field_name) = pattern
            .split_once('.')
            .ok_or(anyhow::anyhow!("Invalid pattern: {}", pattern))?;

        let view = self.views.get(view_name).ok_or(anyhow::anyhow!(
            "View {} not found in semantic model",
            view_name
        ))?;
        let mut fields = HashMap::new();
        if field_name == "*" {
            fields.extend(self.get_all_fields_from_view(view_name)?);
        } else {
            let field = view.get_field(field_name).ok_or(anyhow::anyhow!(
                "Field {} not found in view {}",
                field_name,
                view_name
            ))?;
            fields.insert(pattern.to_owned(), field);
        }

        Ok(fields)
    }

    pub fn get_all_fields(&self) -> HashMap<String, OmniField> {
        let mut fields = HashMap::new();

        for (view_name, view) in &self.views {
            for (name, dimension) in &view.dimensions {
                let field_name = format!("{}.{}", view_name, name);
                fields.insert(field_name, OmniField::Dimension(dimension.clone()));
            }
            for (name, measure) in &view.measures {
                let field_name = format!("{}.{}", view_name, name);
                fields.insert(field_name, OmniField::Measure(measure.clone()));
            }
        }

        fields
    }

    pub fn get_all_view_fields(
        &self,
        view_name: &str,
    ) -> anyhow::Result<HashMap<String, OmniField>> {
        let view = self.views.get(view_name).ok_or(anyhow::anyhow!(
            "View {} not found in semantic model",
            view_name
        ))?;
        let mut fields = HashMap::new();

        for (name, dimension) in &view.dimensions {
            let field_name = format!("{}.{}", view_name, name);
            fields.insert(field_name, OmniField::Dimension(dimension.clone()));
        }
        for (name, measure) in &view.measures {
            let field_name = format!("{}.{}", view_name, name);
            fields.insert(field_name, OmniField::Measure(measure.clone()));
        }

        Ok(fields)
    }

    pub fn get_all_fields_from_view(
        &self,
        view_name: &str,
    ) -> anyhow::Result<HashMap<String, OmniField>> {
        let view = self.views.get(view_name).ok_or(anyhow::anyhow!(
            "View {} not found in semantic model",
            view_name
        ))?;
        let mut fields = HashMap::new();

        for (name, dimension) in &view.dimensions {
            let field_name = format!("{}.{}", view_name, name);
            fields.insert(field_name, OmniField::Dimension(dimension.clone()));
        }
        for (name, measure) in &view.measures {
            let field_name = format!("{}.{}", view_name, name);
            fields.insert(field_name, OmniField::Measure(measure.clone()));
        }

        Ok(fields)
    }

    pub fn get_field(&self, view_name: &str, field_name: &str) -> anyhow::Result<OmniField> {
        let view = self.views.get(view_name).ok_or(anyhow::anyhow!(
            "View {} not found in semantic model",
            view_name
        ))?;
        let field = view.get_field(field_name).ok_or(anyhow::anyhow!(
            "Field {} not found in view {}",
            field_name,
            view_name
        ))?;
        Ok(field)
    }

    pub fn get_topic_fields(&self, topic_name: &str) -> anyhow::Result<HashMap<String, OmniField>> {
        let topic = self.topics.get(topic_name).ok_or(anyhow::anyhow!(
            "Topic {} not found in semantic model",
            topic_name
        ))?;
        let mut fields = HashMap::new();

        for field_pattern in &topic.get_sorted_field_patterns() {
            let mut exclution = false;
            let mut field_pattern_cleaned = field_pattern.to_owned();
            if field_pattern.starts_with("-") {
                exclution = true;
                field_pattern_cleaned = field_pattern[1..].to_string();
            }
            let pattern_fields = self.get_fields_by_pattern(&field_pattern_cleaned)?;
            if exclution {
                for (name, field) in pattern_fields {
                    fields.remove(&name);
                }
            } else {
                for (name, field) in pattern_fields {
                    fields.insert(name, field);
                }
            }
        }

        Ok(fields)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum OmniJoinType {
    #[serde(rename = "always_left")]
    AlwaysLeft,
    #[serde(rename = "full_outer")]
    FullOuter,
    #[serde(rename = "inner")]
    Inner,
}

impl OmniJoinType {
    pub fn to_sql(&self) -> String {
        match self {
            OmniJoinType::AlwaysLeft => "LEFT JOIN".to_string(),
            OmniJoinType::FullOuter => "FULL OUTER JOIN".to_string(),
            OmniJoinType::Inner => "INNER JOIN".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum OmniRelationShipType {
    #[serde(rename = "one_to_one")]
    OneToOne,
    #[serde(rename = "one_to_many")]
    OneToMany,
    #[serde(rename = "many_to_one")]
    ManyToOne,
    #[serde(rename = "many_to_many")]
    ManyToMany,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OmniRelationShip {
    pub join_from_view: String,
    pub join_to_view: String,
    pub join_type: OmniJoinType,
    pub on_sql: String,
    pub relationship_type: OmniRelationShipType,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct OmniTopicInfoTool {
    pub name: String,
    pub model_path: PathBuf,
    #[serde(default = "default_omni_info_description")]
    pub description: String,
}

impl OmniTopicInfoTool {
    pub fn get_description(&self) -> String {
        let mut description = "Get topic information. List of available topic:\n".to_string();
        let semantic_model = self
            .load_semantic_model()
            .expect("Failed to load semantic model");
        for (topic_name, topic) in semantic_model.topics {
            description.push_str(&format!("- {}\n", topic_name,));
        }
        description
    }

    pub fn load_semantic_model(&self) -> Result<OmniSemanticModel, OxyError> {
        // check if model path exists
        if !self.model_path.exists() {
            return Err(OxyError::AgentError(format!(
                "Model path {} does not exist",
                self.model_path.display()
            )));
        }
        let mut views = HashMap::new();
        let mut topics = HashMap::new();

        let view_paths = list_by_sub_extension(&self.model_path, ".view.yaml");
        let topic_paths = list_by_sub_extension(&self.model_path, ".topic.yaml");

        for view_path in view_paths {
            let entry = view_path;
            let file_bytes = fs::read(entry.clone())
                .map_err(|e| OxyError::AgentError(format!("Failed to read model path: {}", e)))?;
            let view: OmniView = serde_yaml::from_slice(&file_bytes).map_err(|e| {
                OxyError::AgentError(format!(
                    "Failed to parse view: {} {}",
                    entry.to_string_lossy(),
                    e
                ))
            })?;
            match view.view_type.clone() {
                OmniViewType::Table(omni_table_view) => {
                    let view_name = entry
                        .strip_prefix(self.model_path.clone())
                        .unwrap()
                        .to_string_lossy()
                        .to_string()
                        // .replace(".query.view.yaml", "")
                        .replace(".view.yaml", "")
                        .replace("/", "__");
                    views.insert(view_name, view);
                }
                OmniViewType::Query(omni_query_view) => {
                    let view_name = entry
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .to_string()
                        .replace(".query.view.yaml", "");
                    views.insert(view_name, view);
                }
            }
        }

        for topic_path in topic_paths {
            let entry = topic_path;
            let file_bytes = fs::read(entry.clone())
                .map_err(|e| OxyError::AgentError(format!("Failed to read model path: {}", e)))?;
            let topic_name = entry
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
                .replace(".topic.yaml", "");
            let topic = serde_yaml::from_slice(&file_bytes);

            match topic {
                Ok(topic) => {
                    topics.insert(topic_name, topic);
                }
                Err(e) => {
                    tracing::warn!("Failed to parse topic: {} {}", &topic_name, e);
                }
            }
        }

        let relationships_file_path = self.model_path.join("relationships.yaml");

        let relationships: Vec<OmniRelationShip> = if relationships_file_path.exists() {
            let file_bytes = fs::read(relationships_file_path)
                .map_err(|e| OxyError::AgentError(format!("Failed to read model path: {}", e)))?;
            serde_yaml::from_slice(&file_bytes).map_err(|e| {
                OxyError::AgentError(format!("Failed to parse relationships: {}", e))
            })?
        } else {
            vec![]
        };

        Ok(OmniSemanticModel {
            views,
            relationships,
            topics,
        })
    }
}

impl OmniView {
    pub fn get_model_description(&self) -> String {
        let mut description = format!("Schema: {}\n", self.schema);
        for (name, dimension) in &self.dimensions {
            let mut dimension_str = name.to_owned();
            if let Some(ref description) = dimension.description {
                dimension_str.push_str(&format!(" -  {}", description));
            }
            description.push_str(&format!("Dimension: {}\n", dimension_str));
        }
        for (name, measure) in &self.measures {
            let mut measure_str = name.to_owned();
            if let Some(ref description) = measure.description {
                measure_str.push_str(&format!(" -  {})", description));
            }
            description.push_str(&format!("Measure: {}\n", measure_str));
        }
        description
    }
}

impl ExecuteOmniTool {
    pub fn load_semantic_model(&self) -> Result<OmniSemanticModel, OxyError> {
        // check if model path exists
        if !self.model_path.exists() {
            return Err(OxyError::AgentError(format!(
                "Model path {} does not exist",
                self.model_path.display()
            )));
        }
        let mut views = HashMap::new();
        let mut topics = HashMap::new();

        let view_paths = list_by_sub_extension(&self.model_path, ".view.yaml");
        let topic_paths = list_by_sub_extension(&self.model_path, ".topic.yaml");

        for view_path in view_paths {
            let entry = view_path;
            let file_bytes = fs::read(entry.clone())
                .map_err(|e| OxyError::AgentError(format!("Failed to read model path: {}", e)))?;
            let view: OmniView = serde_yaml::from_slice(&file_bytes).map_err(|e| {
                OxyError::AgentError(format!(
                    "Failed to parse view: {} {}",
                    entry.to_string_lossy(),
                    e
                ))
            })?;
            match view.view_type.clone() {
                OmniViewType::Table(omni_table_view) => {
                    let view_name = entry
                        .strip_prefix(self.model_path.clone())
                        .unwrap()
                        .to_string_lossy()
                        .to_string()
                        // .replace(".query.view.yaml", "")
                        .replace(".view.yaml", "")
                        .replace("/", "__");
                    views.insert(view_name, view);
                }
                OmniViewType::Query(omni_query_view) => {
                    let view_name = entry
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .to_string()
                        .replace(".query.view.yaml", "");
                    views.insert(view_name, view);
                }
            }
        }

        for topic_path in topic_paths {
            let entry = topic_path;
            let file_bytes = fs::read(entry.clone())
                .map_err(|e| OxyError::AgentError(format!("Failed to read model path: {}", e)))?;
            let topic_name = entry
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
                .replace(".topic.yaml", "");
            let topic = serde_yaml::from_slice(&file_bytes);

            match topic {
                Ok(topic) => {
                    topics.insert(topic_name, topic);
                }
                Err(e) => {
                    tracing::warn!("Failed to parse topic: {} {}", &topic_name, e);
                }
            }
        }

        let relationships_file_path = self.model_path.join("relationships.yaml");

        let relationships: Vec<OmniRelationShip> = if relationships_file_path.exists() {
            let file_bytes = fs::read(relationships_file_path)
                .map_err(|e| OxyError::AgentError(format!("Failed to read model path: {}", e)))?;
            serde_yaml::from_slice(&file_bytes).map_err(|e| {
                OxyError::AgentError(format!("Failed to parse relationships: {}", e))
            })?
        } else {
            vec![]
        };

        Ok(OmniSemanticModel {
            views,
            relationships,
            topics,
        })
    }
    // pub fn get_description(&self) -> Result<String, OxyError> {
    //     if self.models.is_empty() {
    //         tracing::warn!("No semantic models for Omni");
    //         return Err(OxyError::AgentError(
    //             "No semantic models for Omni tool. Please add models to the config file."
    //                 .to_owned(),
    //         ));
    //     }
    //     let mut description =
    //         "Execute query on the database. Construct from Omni semantic model. Dimension/Measure must be full name: {table}.{dimension/measure name}".to_string();
    //     for omni_model in &self.load_semantic_model()? {
    //         let model_description = omni_model.get_model_description();
    //         description.push_str(&format!("{}\n\n", model_description));
    //     }

    //     Ok(description)
    // }
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
pub struct MarkdownDisplay {
    pub content: String,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct LineChartDisplay {
    pub x: String,
    pub y: String,
    pub x_axis_label: Option<String>,
    pub y_axis_label: Option<String>,
    #[schemars(description = "reference data output from a task using task name")]
    pub data: String,
    pub series: Option<String>,
    pub title: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct BarChartDisplay {
    pub x: String,
    pub y: String,
    pub title: Option<String>,
    pub data: String,
    pub series: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct PieChartDisplay {
    pub name: String,
    pub value: String,
    pub title: Option<String>,
    pub data: String,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct TableDisplay {
    pub data: String,
    pub title: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(tag = "type")]
pub enum Display {
    #[serde(rename = "markdown")]
    Markdown(MarkdownDisplay),
    #[serde(rename = "line_chart")]
    LineChart(LineChartDisplay),
    #[serde(rename = "pie_chart")]
    PieChart(PieChartDisplay),
    #[serde(rename = "bar_chart")]
    BarChart(BarChartDisplay),
    #[serde(rename = "table")]
    Table(TableDisplay),
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct AppConfig {
    #[schemars(description = "tasks to prepare the data for the app")]
    pub tasks: Vec<Task>,
    #[schemars(description = "display blocks to render the app")]
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
    #[serde(rename = "execute_omni")]
    ExecuteOmni(ExecuteOmniTool),
    #[serde(rename = "visualize")]
    Visualize(VisualizeTool),
    #[serde(rename = "workflow")]
    Workflow(WorkflowTool),
    #[serde(rename = "agent")]
    Agent(AgentTool),
    #[serde(rename = "omni_topic_info")]
    OmniTopicInfo(OmniTopicInfoTool),
    #[serde(rename = "create_data_app")]
    CreateDataApp(CreateDataAppTool),
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

fn default_embed_table() -> String {
    "documents".to_string()
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

fn default_omni_tool_description() -> String {
    "Execute query on the database. Construct from Omni semantic model.".to_string()
}

fn default_omni_info_description() -> String {
    "Get details a about a omni topic. Including available fields".to_string()
}

fn default_validate_sql_tool_description() -> String {
    "Validate the SQL query. If the query is invalid, fix it and run again.".to_string()
}

fn default_create_data_app_tool_description() -> String {
    "Create a data app/dashboard to visualize metrics.".to_string()
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
  2. If the task is not relevant to any tool, explain why.
  3. If the task is relevant to a tool, route it to the tool.
  Your task:"
    }
    .to_string()
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
