// Agent configuration types
// These definitions must match the oxy-agent crate to ensure JSON schema compatibility
// and correct deserialization of agentic workflow files

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::model::{EmbeddingConfig, VectorDBConfig};

/// Configuration for an agentic workflow (FSM-based agent)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgenticConfig {
    /// Workflow name. Accepted in YAML for documentation but always overwritten
    /// by the filename.
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_public")]
    pub public: bool,
    pub model: String,
    #[serde(default)]
    pub instruction: String,
    #[serde(default = "default_auto_transition_prompt")]
    pub auto_transition_prompt: String,
    pub start: StartConfig,
    pub end: EndConfig,
    pub transitions: Vec<Transition>,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
}

fn default_public() -> bool {
    true
}

fn default_auto_transition_prompt() -> String {
    "Based on the conversation messages, select the next action to take from the list of available actions.".to_string()
}

fn default_max_iterations() -> usize {
    15
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct StartConfig {
    #[serde(flatten)]
    pub start: Start,
    pub next: TransitionMode,
    #[serde(default)]
    pub routing: Option<RoutingConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RoutingConfig {
    pub routes: Vec<String>,
    #[serde(default, flatten)]
    pub db_config: VectorDBConfig,
    #[serde(flatten)]
    pub embedding_config: EmbeddingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct Start {
    #[serde(default = "default_start_name")]
    pub name: String,
    #[serde(default = "default_start_description")]
    pub description: String,
    #[serde(flatten)]
    pub mode: StartMode,
}

fn default_start_name() -> String {
    "start".to_string()
}

fn default_start_description() -> String {
    "The starting point of the agent's workflow".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case", tag = "mode")]
pub enum StartMode {
    #[default]
    Default,
    Plan {
        model: Option<String>,
        #[serde(default = "default_plan_instruction")]
        instruction: String,
        #[serde(default = "default_plan_example")]
        example: String,
    },
}

fn default_plan_instruction() -> String {
    "You are an expert Data Analyst. Your task is to break down the user's query into clear, actionable steps using the available actions.".to_string()
}

fn default_plan_example() -> String {
    "".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct EndConfig {
    #[serde(flatten)]
    pub end: End,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct End {
    #[serde(default = "default_end_name")]
    pub name: String,
    #[serde(default = "default_end_description")]
    pub description: String,
    #[serde(flatten)]
    pub mode: EndMode,
    #[serde(default)]
    pub output_artifact: OutputArtifact,
}

fn default_end_name() -> String {
    "end".to_string()
}

fn default_end_description() -> String {
    "The ending point of the agent's workflow".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case", tag = "mode")]
pub enum EndMode {
    #[default]
    Default,
    Synthesize {
        model: Option<String>,
        #[serde(default = "default_synthesize_instruction")]
        instruction: String,
    },
}

fn default_synthesize_instruction() -> String {
    "Given the conversation so far, synthesize a final answer that addresses the original objective.".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputArtifact {
    #[default]
    None,
    App,
    Query,
    Visualization,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(untagged)]
pub enum TransitionMode {
    Always(String),
    Auto(Vec<String>),
    #[default]
    Plan,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Transition {
    #[serde(flatten)]
    pub trigger: TriggerType,
    #[serde(default = "TransitionMode::default")]
    pub next: TransitionMode,
}

/// Simplified trigger types for core (stub implementations)
/// Full implementations with all fields are in oxy-agent crate
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerType {
    Query(serde_json::Value),
    SemanticQuery(serde_json::Value),
    Visualize(serde_json::Value),
    Insight(serde_json::Value),
    Subflow(serde_json::Value),
    SaveAutomation(serde_json::Value),
    End(serde_json::Value),
}

/// Input types for agent and workflow execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInput {
    pub question: String,
    #[serde(default)]
    pub variables: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInput {
    pub variables: std::collections::HashMap<String, serde_json::Value>,
}
