use garde::Validate;
use indoc::indoc;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use std::collections::HashMap;
use std::path::PathBuf;
use std::{env, fmt};

use crate::config::validate::validate_file_path;
use crate::config::validate::{
    ValidationContext, validate_agent_exists, validate_database_exists, validate_env_var,
};
use crate::errors::OxyError;
use schemars::JsonSchema;

use super::validate::{
    AgentValidationContext, validate_model, validate_output_format, validate_task,
};

#[derive(Serialize, Deserialize, Validate, Debug, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct Config {
    #[garde(dive)]
    pub defaults: Option<Defaults>,
    #[garde(dive)]
    pub models: Vec<Model>,
    #[garde(dive)]
    pub databases: Vec<Database>,

    #[serde(skip)]
    #[garde(skip)]
    #[schemars(skip)]
    pub project_path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct SemanticModels {
    pub table: String,
    pub database: String,
    pub description: String,
    pub entities: Vec<Entity>,
    pub dimensions: Vec<Dimension>,
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
    pub sample: Vec<String>,
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
    #[garde(skip)]
    pub name: String,
    #[garde(custom(validate_model))]
    pub model: String,
    #[garde(length(min = 1))]
    pub system_instructions: String,
    #[serde(default = "default_tools")]
    #[garde(skip)]
    pub tools: Vec<ToolConfig>,
    #[garde(skip)]
    pub context: Option<Vec<AgentContext>>,
    #[serde(default)]
    #[garde(custom(validate_output_format))]
    pub output_format: OutputFormat,
    #[garde(skip)]
    pub anonymize: Option<AnonymizerConfig>,
    #[serde(default)]
    #[garde(skip)]
    pub tests: Vec<Eval>,

    #[garde(skip)]
    #[serde(default)]
    pub description: String,
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
    pub key_path: Option<PathBuf>,
    #[garde(length(min = 1))]
    pub dataset: String,
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

impl fmt::Display for DatabaseType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    pub fn db_name(&self) -> String {
        match &self.database_type {
            DatabaseType::Bigquery(bq) => bq.dataset.clone(),
            DatabaseType::DuckDB(ddb) => ddb.file_search_path.clone(),
            DatabaseType::Postgres(pg) => pg.database.clone().unwrap_or_default(),
            DatabaseType::Redshift(rs) => rs.database.clone().unwrap_or_default(),
            DatabaseType::Mysql(my) => my.database.clone().unwrap_or_default(),
            DatabaseType::ClickHouse(ch) => ch.database.clone(),
            DatabaseType::Snowflake(sn) => sn.warehouse.clone(),
        }
    }

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

    pub fn protocol(&self) -> String {
        match &self.database_type {
            DatabaseType::Bigquery(_) => "binary".to_owned(),
            DatabaseType::Postgres(_) => "binary".to_owned(),
            DatabaseType::Redshift(_) => "cursor".to_owned(),
            DatabaseType::Mysql(_) => "binary".to_owned(),
            _ => "".to_owned(),
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
        #[garde(skip)]
        azure_deployment_id: Option<String>,
        #[garde(skip)]
        azure_api_version: Option<String>,
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
#[derive(Serialize, Deserialize, Default, Clone, Debug, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    #[default]
    Default,
    File,
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

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
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
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct FormatterTask {
    #[garde(length(min = 1))]
    pub template: String,
    #[garde(dive)]
    pub export: Option<TaskExport>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct WorkflowTask {
    #[garde(skip)]
    pub src: PathBuf,
    #[garde(skip)]
    pub variables: Option<HashMap<String, String>>,
    #[garde(dive)]
    pub export: Option<TaskExport>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
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

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
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

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
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
    pub variables: Option<HashMap<String, String>>,
    #[serde(default = "default_tests")]
    pub tests: Vec<Eval>,
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

#[derive(Serialize, Deserialize, Debug, Validate, JsonSchema, Clone)]
#[serde(tag = "type")]
#[garde(context(ValidationContext))]
pub enum Eval {
    #[serde(rename = "consistency")]
    Consistency(#[garde(dive)] Consistency),
}

#[derive(Serialize, Deserialize, Debug, Validate, JsonSchema, Clone)]
#[garde(context(ValidationContext))]
pub struct Consistency {
    #[garde(length(min = 1))]
    #[serde(default = "default_consistency_prompt")]
    pub prompt: String,
    #[garde(length(min = 1))]
    pub model_ref: Option<String>,
    #[garde(skip)]
    #[serde(default = "default_n")]
    pub n: usize,
    #[garde(length(min = 1))]
    pub task_description: Option<String>,
    #[garde(skip)]
    pub task_ref: Option<String>,
    #[garde(skip)]
    #[serde(default = "default_scores")]
    pub scores: HashMap<String, f32>,
    #[garde(skip)]
    #[serde(default = "default_consistency_concurrency")]
    pub concurrency: usize,
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
    pub tests: Vec<Eval>,
    #[garde(skip)]
    pub variables: Option<HashMap<String, String>>,
    #[garde(skip)]
    #[serde(default)]
    pub description: String,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct RetrievalTool {
    pub name: String,
    #[serde(default = "default_retrieval_tool_description")]
    pub description: String,
    pub src: Vec<String>,
    #[serde(default = "default_embed_model")]
    pub embed_model: String,
    #[serde(default = "default_api_url")]
    pub api_url: String,
    pub api_key: Option<String>,
    #[serde(default = "default_key_var")]
    pub key_var: String,
    #[serde(default = "default_retrieval_n_dims")]
    pub n_dims: usize,
    #[serde(default = "default_retrieval_top_k")]
    pub top_k: usize,
    #[serde(default = "default_retrieval_factor")]
    pub factor: usize,
}

impl RetrievalTool {
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
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct ValidateSQLTool {
    pub name: String,
    #[serde(default = "default_validate_sql_tool_description")]
    pub description: String,
    pub database: String,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(tag = "type")]
pub enum ToolConfig {
    #[serde(rename = "execute_sql")]
    ExecuteSQL(ExecuteSQLTool),
    #[serde(rename = "validate_sql")]
    ValidateSQL(ValidateSQLTool),
    #[serde(rename = "retrieval")]
    Retrieval(RetrievalTool),
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

fn default_validate_sql_tool_description() -> String {
    "Validate the SQL query. If the query is invalid, fix it and run again.".to_string()
}

fn default_tools() -> Vec<ToolConfig> {
    vec![]
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

fn default_tests() -> Vec<Eval> {
    vec![]
}

fn default_loop_concurrency() -> usize {
    1
}

fn default_consistency_concurrency() -> usize {
    10
}
