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
use lazy_static::lazy_static;
use schemars::JsonSchema;
use std::sync::Mutex;

lazy_static! {
    static ref PROJECT_PATH: Mutex<PathBuf> = Mutex::new(PathBuf::new());
}

#[derive(Serialize, Deserialize, Validate, Debug, Clone, JsonSchema)]
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
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct Context {
    pub name: String,
    pub src: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct AgentConfig {
    pub model: String,
    pub context: Option<Vec<Context>>,
    pub retrieval: Option<String>,
    pub system_instructions: String,
    #[serde(default = "default_tools")]
    pub tools: Option<Vec<ToolConfig>>,
    #[serde(default)]
    pub output_format: OutputFormat,
    pub anonymize: Option<AnonymizerConfig>,
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
pub struct DuckDB {
    #[garde(skip)]
    #[schemars(skip)]
    pub key_path: PathBuf,
}

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

#[derive(Deserialize, Debug, Clone, Validate, Serialize, JsonSchema)]
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

#[derive(Debug, JsonSchema)]
pub struct ParsedConfig {
    pub agent_config: AgentConfig,
    pub model: Model,
    pub warehouse: Warehouse,
    pub retrieval: Retrieval,
}

#[derive(Serialize, Deserialize, Debug, Validate, JsonSchema)]
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

#[derive(Serialize, Deserialize, Debug, Validate, JsonSchema)]
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

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(untagged)]
pub enum LoopValues {
    Template(String),
    Array(Vec<String>),
}

#[derive(Serialize, Deserialize, Debug, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct LoopSequentialStep {
    #[garde(skip)]
    pub values: LoopValues,
    #[garde(dive)]
    pub steps: Vec<Step>,
}

#[derive(Serialize, Deserialize, Debug, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct FormatterStep {
    #[garde(length(min = 1))]
    pub template: String,
}

#[derive(Serialize, Deserialize, Debug, Validate, JsonSchema)]
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

#[derive(Serialize, Deserialize, Debug, Validate, JsonSchema)]
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

#[derive(Serialize, Deserialize, Debug, Validate, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct Workflow {
    #[garde(length(min = 1))]
    pub name: String,
    #[garde(dive)]
    pub steps: Vec<Step>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
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

fn default_tools() -> Option<Vec<ToolConfig>> {
    Some(vec![])
}

pub struct ProjectPath;

impl ProjectPath {
    fn init() -> PathBuf {
        let mut current_dir = std::env::current_dir().expect("Could not get current directory");

        for _ in 0..10 {
            let config_path = current_dir.join("config.yml");
            if config_path.exists() {
                let mut project_path = PROJECT_PATH.lock().unwrap();
                *project_path = current_dir.clone();

                return current_dir;
            }

            if !current_dir.pop() {
                break;
            }
        }

        panic!("Could not find config.yml");
    }

    pub fn get() -> PathBuf {
        let project_path = PROJECT_PATH.lock().unwrap().clone();
        if project_path.as_os_str().is_empty() {
            return Self::init();
        }

        project_path
    }

    pub fn get_path(relative_path: &str) -> PathBuf {
        let project_root = Self::get();
        project_root.join(relative_path)
    }
}
