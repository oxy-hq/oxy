use garde::Validate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use crate::config::validate::validate_file_path;
use crate::config::validate::{
    validate_agent_exists, validate_env_var, validate_warehouse_exists, ValidationContext,
};
use schemars::JsonSchema;

use super::validate::validate_step;

#[derive(Serialize, Deserialize, Validate, Debug, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct Config {
    #[garde(dive)]
    pub defaults: Defaults,
    #[garde(dive)]
    pub models: Vec<Model>,
    #[garde(dive)]
    pub warehouses: Vec<Warehouse>,

    #[serde(skip)]
    #[garde(skip)]
    #[schemars(skip)]
    pub project_path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct SemanticModels {
    pub table: String,
    pub warehouse: String,
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

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct AgentConfig {
    pub model: String,
    pub system_instructions: String,
    #[serde(default = "default_tools")]
    pub tools: Vec<ToolConfig>,
    pub context: Option<Vec<AgentContext>>,
    #[serde(default)]
    pub output_format: OutputFormat,
    pub anonymize: Option<AnonymizerConfig>,
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
// #[garde(context(Config as ctx))]
pub struct Defaults {
    #[garde(length(min = 1))]
    #[garde(custom(validate_agent_exists))]
    pub agent: String,
    #[garde(length(min = 1))]
    #[garde(custom(|wh: &Option<String>, ctx: &ValidationContext| {
        match wh {
            Some(warehouse) => validate_warehouse_exists(warehouse.as_str(), ctx),
            None => Ok(()),
        }
    }))]
    pub warehouse: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Validate, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct BigQuery {
    #[garde(custom(validate_file_path))]
    pub key_path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, Validate, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct DuckDB {}

#[derive(Serialize, Deserialize, Debug, Validate, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
#[serde(tag = "type")]
pub enum WarehouseType {
    #[serde(rename = "bigquery")]
    Bigquery(#[garde(dive)] BigQuery),
    #[serde(rename = "duckdb")]
    DuckDB(#[garde(dive)] DuckDB),
}

impl fmt::Display for WarehouseType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WarehouseType::Bigquery(_) => write!(f, "bigquery"),
            WarehouseType::DuckDB(_) => write!(f, "duckdb"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Validate, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct Warehouse {
    #[garde(length(min = 1))]
    pub name: String,

    #[garde(length(min = 1))]
    pub dataset: String,

    #[serde(flatten)]
    #[garde(dive)]
    pub warehouse_type: WarehouseType,
}

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
}

fn default_openai_api_url() -> Option<String> {
    Some("https://api.openai.com/v1".to_string())
}

#[derive(Serialize, Deserialize, Default, Clone, Debug, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    #[default]
    Default,
    File,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
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

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
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

#[derive(Serialize, Deserialize, Debug, Clone, Default, JsonSchema)]
pub enum FileFormat {
    #[serde(rename = "json")]
    Json,
    #[serde(rename = "markdown")]
    #[default]
    Markdown,
}

#[derive(Debug, JsonSchema)]
pub struct ParsedConfig {
    pub agent_config: AgentConfig,
    pub model: Model,
    pub warehouse: Warehouse,
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct AgentStep {
    #[garde(length(min = 1))]
    pub prompt: String,
    #[garde(custom(validate_agent_exists))]
    pub agent_ref: String,
    #[serde(default = "default_retry")]
    #[garde(skip)]
    pub retry: usize,

    // #[garde(custom(validate_export_format(self)))]
    #[garde(dive)]
    pub export: Option<StepExport>,
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
pub struct StepExport {
    #[garde(length(min = 1))]
    pub path: String,
    #[garde(dive)]
    pub format: ExportFormat,
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct ExecuteSQLStep {
    #[garde(custom(validate_warehouse_exists))]
    pub warehouse: String,
    // #[garde(custom(validate_sql_file))]
    // Skipping validation for now to allow sql file templating
    #[garde(length(min = 1))]
    pub sql_file: String,
    #[serde(default)]
    #[garde(skip)]
    pub variables: Option<HashMap<String, String>>,

    #[garde(dive)]
    pub export: Option<StepExport>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct FormatterStep {
    #[garde(length(min = 1))]
    pub template: String,
    #[garde(dive)]
    pub export: Option<StepExport>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(untagged)]
pub enum LoopValues {
    Template(String),
    Array(Vec<String>),
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct LoopSequentialStep {
    #[garde(skip)]
    pub values: LoopValues,
    #[garde(dive)]
    pub steps: Vec<Step>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
#[serde(tag = "type")]
pub enum StepType {
    #[serde(rename = "agent")]
    Agent(#[garde(dive)] AgentStep),
    #[serde(rename = "execute_sql")]
    ExecuteSQL(#[garde(dive)] ExecuteSQLStep),
    #[serde(rename = "loop_sequential")]
    LoopSequential(#[garde(dive)] LoopSequentialStep),
    #[serde(rename = "formatter")]
    Formatter(#[garde(dive)] FormatterStep),
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize, JsonSchema)]
pub struct TempWorkflow {
    pub steps: Vec<Step>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct Step {
    #[garde(length(min = 1))]
    pub name: String,
    #[serde(flatten)]
    #[garde(dive)]
    #[garde(custom(validate_step))]
    pub step_type: StepType,
}

fn default_retry() -> usize {
    1
}

#[derive(Serialize, Deserialize, Debug, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct Workflow {
    #[garde(length(min = 1))]
    pub name: String,
    #[garde(dive)]
    pub steps: Vec<Step>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
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
    pub fn get_api_key(&self) -> String {
        match &self.api_key {
            Some(key) => key.to_string(),
            None => std::env::var(&self.key_var).unwrap_or_else(|_| {
                panic!(
                    "OpenAI key not found in environment variable {}",
                    self.key_var
                )
            }),
        }
    }
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

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct ExecuteSQLTool {
    pub name: String,
    #[serde(default = "default_sql_tool_description")]
    pub description: String,
    pub warehouse: String,
}

fn default_sql_tool_description() -> String {
    "Execute the SQL query. If the query is invalid, fix it and run again.".to_string()
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(tag = "type")]
pub enum ToolConfig {
    #[serde(rename = "execute_sql")]
    ExecuteSQL(ExecuteSQLTool),
    #[serde(rename = "retrieval")]
    Retrieval(RetrievalTool),
}

fn default_tools() -> Vec<ToolConfig> {
    vec![]
}
