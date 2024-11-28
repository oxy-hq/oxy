use dirs::home_dir;
use garde::Validate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use crate::config::validate::validate_file_path;
use crate::config::validate::{
    validate_agent_exists, validate_embed_model, validate_env_var, validate_rerank_model,
    validate_warehouse_exists, ValidationContext,
};

#[derive(Serialize, Deserialize, Validate, Debug, Clone)]
#[garde(context(ValidationContext))]
pub struct Config {
    #[garde(dive)]
    pub defaults: Defaults,
    #[garde(dive)]
    pub models: Vec<Model>,
    #[garde(dive)]
    pub warehouses: Vec<Warehouse>,
    #[garde(dive)]
    pub retrievals: Vec<Retrieval>,
    #[serde(skip)]
    #[garde(skip)]
    pub project_path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AgentConfig {
    pub model: String,
    pub retrieval: Option<String>,
    pub system_instructions: String,
    #[serde(default = "default_tools")]
    pub tools: Option<Vec<ToolConfig>>,
    #[serde(default)]
    pub output_format: OutputFormat,
}

// These are settings stored as strings derived from the config.yml file's defaults section
#[derive(Debug, Validate, Deserialize, Serialize, Clone)]
#[garde(context(ValidationContext))]
// #[garde(context(Config as ctx))]
pub struct Defaults {
    #[garde(length(min = 1))]
    #[garde(custom(validate_agent_exists))]
    pub agent: String,
}

#[derive(Serialize, Deserialize, Debug, Validate, Clone)]
#[garde(context(ValidationContext))]
pub struct BigQuery {
    #[garde(custom(validate_file_path))]
    pub key_path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, Validate, Clone)]
#[garde(context(ValidationContext))]
pub struct DuckDB {
    #[garde(skip)]
    pub key_path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, Validate, Clone)]
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

#[derive(Serialize, Deserialize, Debug, Validate, Clone)]
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

#[derive(Deserialize, Debug, Clone, Validate, Serialize)]
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

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    #[default]
    Default,
    File,
}

#[derive(Deserialize, Debug, Clone, Validate, Serialize)]
#[garde(context(ValidationContext))]
pub struct Retrieval {
    #[garde(length(min = 1))]
    pub name: String,
    #[garde(custom(validate_embed_model))]
    pub embed_model: String,
    #[garde(custom(validate_rerank_model))]
    pub rerank_model: String,
    #[garde(skip)]
    pub top_k: usize,
    #[garde(skip)]
    pub factor: usize,
}

pub fn get_config_path() -> PathBuf {
    home_dir()
        .expect("Could not find home directory")
        .join(".config")
        .join("onyx")
        .join("config.yml")
}

#[derive(Debug)]
pub struct ParsedConfig {
    pub agent_config: AgentConfig,
    pub model: Model,
    pub warehouse: Warehouse,
    pub retrieval: Retrieval,
}

#[derive(Serialize, Deserialize, Debug, Validate)]
#[garde(context(ValidationContext))]
pub struct AgentStep {
    #[garde(length(min = 1))]
    pub prompt: String,
    #[garde(custom(validate_agent_exists))]
    pub agent_ref: String,
    #[serde(default = "default_retry")]
    #[garde(skip)]
    pub retry: usize,
}

#[derive(Serialize, Deserialize, Debug, Validate)]
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
}

#[derive(Serialize, Deserialize, Debug, Validate)]
#[garde(context(ValidationContext))]
pub struct LoopSequentialStep {
    #[garde(length(min = 1))]
    pub values: Vec<String>,
    #[garde(dive)]
    pub steps: Vec<Step>,
}

#[derive(Serialize, Deserialize, Debug, Validate)]
#[garde(context(ValidationContext))]
pub struct FormatterStep {
    #[garde(length(min = 1))]
    pub template: String,
}

#[derive(Serialize, Deserialize, Debug, Validate)]
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

// Temporary workflow object that reads in from the yaml file before it's combined with the
// workflow name (filename-associated) into the `Workflow` struct
#[derive(Deserialize)]
pub struct TempWorkflow {
    pub steps: Vec<Step>,
}

#[derive(Serialize, Deserialize, Debug, Validate)]
#[garde(context(ValidationContext))]
pub struct Step {
    #[garde(length(min = 1))]
    pub name: String,
    #[serde(flatten)]
    #[garde(dive)]
    pub step_type: StepType,
}

fn default_retry() -> usize {
    1
}

#[derive(Serialize, Deserialize, Debug, Validate)]
#[garde(context(ValidationContext))]
pub struct Workflow {
    #[garde(length(min = 1))]
    pub name: String,
    #[garde(dive)]
    pub steps: Vec<Step>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum ToolConfig {
    #[serde(rename = "execute_sql")]
    ExecuteSQL {
        name: String,
        #[serde(default = "default_sql_tool_description")]
        description: String,
        warehouse: String,
    },
    #[serde(rename = "retrieval")]
    Retrieval {
        name: String,
        #[serde(default = "default_retrieval_tool_description")]
        description: String,
        data: Vec<String>,
    },
}

fn default_sql_tool_description() -> String {
    "Execute the SQL query. If the query is invalid, fix it and run again.".to_string()
}

fn default_retrieval_tool_description() -> String {
    "Retrieve the relevant SQL queries to support query generation.".to_string()
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum OutputType {
    #[default]
    Default,
    File,
}

fn default_tools() -> Option<Vec<ToolConfig>> {
    Some(vec![])
}
