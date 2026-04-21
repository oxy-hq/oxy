//! Workflow configuration types.
//!
//! These types parse the same YAML format as `oxy::config::model` but are
//! self-contained — no dependency on the oxy core crate. Task types that the
//! orchestrator doesn't inspect (ExecuteSQL, OmniQuery, etc.) are represented
//! as opaque `serde_json::Value`.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── Workflow ────────────────────────────────────────────────────────────────

/// Top-level workflow configuration parsed from `.workflow.yml` / `.procedure.yml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConfig {
    #[serde(default)]
    pub name: String,
    pub tasks: Vec<TaskConfig>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub variables: Option<HashMap<String, Value>>,
    pub consistency_prompt: Option<String>,
    /// Model reference for the consistency evaluator (e.g. `"claude-haiku-4-5"`).
    /// Resolved via project `config.yml` model definitions.
    pub consistency_model: Option<String>,
}

// ── Task ────────────────────────────────────────────────────────────────────

/// A single task within a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConfig {
    pub name: String,
    #[serde(flatten)]
    pub task_type: TaskType,
}

/// Workflow task types. Variants the orchestrator inspects have typed configs;
/// delegated variants are opaque JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TaskType {
    // ── Orchestrator-inspected types ────────────────────────────────────
    #[serde(rename = "agent")]
    Agent(AgentTaskConfig),
    #[serde(rename = "formatter")]
    Formatter(FormatterConfig),
    #[serde(rename = "conditional")]
    Conditional(ConditionalConfig),
    #[serde(rename = "loop_sequential")]
    LoopSequential(LoopConfig),
    #[serde(rename = "workflow")]
    SubWorkflow(SubWorkflowConfig),

    // ── Delegated types (opaque to orchestrator) ────────────────────────
    #[serde(rename = "execute_sql")]
    ExecuteSql(Value),
    #[serde(rename = "semantic_query")]
    SemanticQuery(Value),
    #[serde(rename = "omni_query")]
    OmniQuery(Value),
    #[serde(rename = "looker_query")]
    LookerQuery(Value),
    #[serde(rename = "visualize")]
    Visualize(Value),

    #[serde(other)]
    Unknown,
}

// ── Inner task configs ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskConfig {
    pub agent_ref: String,
    pub prompt: String,
    #[serde(default = "default_one")]
    pub consistency_run: usize,
    #[serde(default)]
    pub retry: usize,
    pub variables: Option<HashMap<String, Value>>,
    pub consistency_prompt: Option<String>,
    /// Model reference for the consistency evaluator (overrides workflow-level).
    pub consistency_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatterConfig {
    pub template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubWorkflowConfig {
    pub src: PathBuf,
    pub variables: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopConfig {
    /// Loop values: either a JSON array or a Jinja2 template string.
    pub values: Value,
    pub tasks: Vec<TaskConfig>,
    #[serde(default = "default_one")]
    pub concurrency: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalConfig {
    pub conditions: Vec<ConditionBranch>,
    #[serde(default, rename = "else")]
    pub else_tasks: Option<Vec<TaskConfig>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionBranch {
    #[serde(rename = "if")]
    pub condition: String,
    pub tasks: Vec<TaskConfig>,
}

// ── Semantic query config (used by semantic.rs) ─────────────────────────────

/// Semantic query parameters — the subset of fields that the semantic compiler
/// needs. Mirrors `oxy::types::SemanticQueryParams` but self-contained.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticQueryConfig {
    pub topic: Option<String>,
    #[serde(default)]
    pub measures: Vec<String>,
    #[serde(default)]
    pub dimensions: Vec<String>,
    #[serde(default)]
    pub time_dimensions: Vec<TimeDimensionConfig>,
    #[serde(default)]
    pub filters: Vec<SemanticFilter>,
    #[serde(default, alias = "order")]
    pub orders: Vec<SemanticOrder>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeDimensionConfig {
    pub dimension: String,
    pub granularity: Option<TimeGranularity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticFilter {
    pub field: String,
    #[serde(flatten)]
    pub filter_type: SemanticFilterType,
}

/// Filter operators for semantic queries. Mirrors `oxy::config::model::SemanticFilterType`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum SemanticFilterType {
    #[serde(rename = "eq")]
    Eq(ScalarFilter),
    #[serde(rename = "neq")]
    Neq(ScalarFilter),
    #[serde(rename = "gt")]
    Gt(ScalarFilter),
    #[serde(rename = "gte")]
    Gte(ScalarFilter),
    #[serde(rename = "lt")]
    Lt(ScalarFilter),
    #[serde(rename = "lte")]
    Lte(ScalarFilter),
    #[serde(rename = "in")]
    In(ArrayFilter),
    #[serde(rename = "not_in")]
    NotIn(ArrayFilter),
    #[serde(rename = "in_date_range")]
    InDateRange(DateRangeFilter),
    #[serde(rename = "not_in_date_range")]
    NotInDateRange(DateRangeFilter),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalarFilter {
    pub value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArrayFilter {
    pub values: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRangeFilter {
    pub from: Value,
    pub to: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticOrder {
    pub field: String,
    #[serde(default = "default_asc")]
    pub direction: String,
}

/// Time granularity for semantic time dimensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimeGranularity {
    Year,
    Quarter,
    Month,
    Week,
    Day,
    Hour,
    Minute,
    Second,
}

// ── Defaults ────────────────────────────────────────────────────────────────

fn default_one() -> usize {
    1
}

fn default_asc() -> String {
    "asc".to_string()
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_workflow() {
        let yaml = r#"
name: test_workflow
tasks:
  - name: query_data
    type: execute_sql
    database: my_db
    sql_query: "SELECT * FROM orders"
  - name: summarize
    type: formatter
    template: "Total: {{ query_data }}"
"#;
        let config: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.name, "test_workflow");
        assert_eq!(config.tasks.len(), 2);
        assert_eq!(config.tasks[0].name, "query_data");
        assert!(matches!(config.tasks[0].task_type, TaskType::ExecuteSql(_)));
        assert_eq!(config.tasks[1].name, "summarize");
        assert!(matches!(config.tasks[1].task_type, TaskType::Formatter(_)));
    }

    #[test]
    fn test_parse_agent_task() {
        let yaml = r#"
name: agent_step
tasks:
  - name: analyze
    type: agent
    agent_ref: agents/default.agent.yml
    prompt: "Analyze the data"
    consistency_run: 3
"#;
        let config: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        let TaskType::Agent(agent) = &config.tasks[0].task_type else {
            panic!("expected Agent");
        };
        assert_eq!(agent.agent_ref, "agents/default.agent.yml");
        assert_eq!(agent.prompt, "Analyze the data");
        assert_eq!(agent.consistency_run, 3);
    }

    #[test]
    fn test_parse_loop_task() {
        let yaml = r#"
name: loop_test
tasks:
  - name: per_item
    type: loop_sequential
    values: [apple, banana, cherry]
    concurrency: 2
    tasks:
      - name: detail
        type: execute_sql
        database: db
        sql_query: "SELECT 1"
"#;
        let config: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        let TaskType::LoopSequential(loop_cfg) = &config.tasks[0].task_type else {
            panic!("expected LoopSequential");
        };
        assert_eq!(loop_cfg.concurrency, 2);
        assert_eq!(loop_cfg.tasks.len(), 1);
        assert!(loop_cfg.values.is_array());
    }

    #[test]
    fn test_parse_sub_workflow() {
        let yaml = r#"
name: parent
tasks:
  - name: child
    type: workflow
    src: procedures/child.procedure.yml
    variables:
      fruit: apple
"#;
        let config: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        let TaskType::SubWorkflow(wf) = &config.tasks[0].task_type else {
            panic!("expected SubWorkflow");
        };
        assert_eq!(wf.src.to_str().unwrap(), "procedures/child.procedure.yml");
        assert!(wf.variables.is_some());
    }

    #[test]
    fn test_unknown_task_type() {
        let yaml = r#"
name: test
tasks:
  - name: mystery
    type: future_task_type
"#;
        let config: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(config.tasks[0].task_type, TaskType::Unknown));
    }
}
