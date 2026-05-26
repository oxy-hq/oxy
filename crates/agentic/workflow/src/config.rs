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
    /// Optional file export wrapper. When present, the task's result is
    /// written to disk after the inner step completes successfully. Mirrors
    /// the old `oxy-workflow::TaskExport` shape so existing `.workflow.yml`
    /// files keep working unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export: Option<TaskExport>,
    /// Optional **file-presence cache** distinct from the step-hash
    /// cache (which keys on YAML + render_context).
    ///
    /// Semantics — matches the legacy `oxy-workflow::TaskCache`:
    ///   - First run with `cache.enabled = true`: the step executes
    ///     normally, then its answer is written to `cache.path` (jinja-
    ///     rendered against the step's render_context).
    ///   - Any subsequent run: if the file at `cache.path` already
    ///     exists, **the step is skipped** and the file's contents are
    ///     used as the step's result.
    ///
    /// The whole point is to let a user manually edit the cached file
    /// (e.g. tweak an LLM-generated SQL query) and have those edits
    /// survive every subsequent run. Deleting the file is the
    /// invalidation; `cache.enabled = false` disables the mechanism
    /// without removing the field.
    ///
    /// Independent of `export:` — if both are set, the cache write
    /// fires on success in addition to whatever `export:` does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<CacheConfig>,
}

/// File export wrapper applied to a single task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskExport {
    /// Destination path, relative to the workspace root. May contain Jinja
    /// expressions (e.g. `"out/{{ today }}.csv"`) — they're rendered against
    /// the same render context the step itself sees.
    pub path: String,
    pub format: ExportFormat,
}

/// File-presence cache config. See [`TaskConfig::cache`] for semantics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheConfig {
    /// Off by default — a user adds this block *only* when they want
    /// the file-presence behavior, so unset means no caching even when
    /// the path field is present.
    #[serde(default = "default_cache_enabled")]
    pub enabled: bool,
    /// Destination path, relative to the workspace root. May contain
    /// Jinja expressions (e.g. `"out/{{ groupings.value }}.sql"`).
    pub path: String,
}

fn default_cache_enabled() -> bool {
    true
}

/// Supported export formats — kept aligned with the legacy
/// `oxy::config::model::ExportFormat` so existing YAML still parses.
///
/// `Csv` / `Json` / `Sql` cover tabular outputs (`execute_sql`,
/// `semantic_query`, `omni_query`, `looker_query`). `Txt` and `Docx`
/// cover agent text outputs in the legacy schema; the parse path
/// preserves them so existing `.workflow.yml` files round-trip, but
/// the agent-task export wiring isn't ported yet — see
/// [`crate::export`] for which formats execute today.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
// `docx` was a documented variant in the legacy `oxy-workflow` schema
// but never had a real writer — `export_formatter` just wrote raw
// bytes to a `.docx`-suffixed file that Word refused to open. Dropping
// the variant turns any leftover `format: docx` into a clear
// "unknown variant `docx`, expected one of `csv`, `json`, `sql`, `txt`"
// parse error instead of a runtime "not supported" message.
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Csv,
    Json,
    Sql,
    Txt,
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

    // `type: visualize` was a legacy workflow task that ran an LLM to
    // render a chart from the previous step's data. The chat agent's
    // `visualize` *tool* (different surface) covers the same need now,
    // so the task variant is retired. `#[serde(other)]` catches any
    // leftover usage as `Unknown`, which the executor surfaces with a
    // clear "unknown task type" error.
    #[serde(other)]
    Unknown,
}

impl TaskType {
    /// Canonical kebab-case identifier matching the YAML `type:` discriminator
    /// — also what the frontend keys off of when rendering per-task content.
    pub fn name(&self) -> &'static str {
        match self {
            TaskType::Agent(_) => "agent",
            TaskType::Formatter(_) => "formatter",
            TaskType::Conditional(_) => "conditional",
            TaskType::LoopSequential(_) => "loop_sequential",
            TaskType::SubWorkflow(_) => "workflow",
            TaskType::ExecuteSql(_) => "execute_sql",
            TaskType::SemanticQuery(_) => "semantic_query",
            TaskType::OmniQuery(_) => "omni_query",
            TaskType::LookerQuery(_) => "looker_query",
            TaskType::Unknown => "unknown",
        }
    }
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
    /// Output-shaping switch for the agent. Default is
    /// `AgentOutputMode::Answer` — the analytics agent runs the
    /// full pipeline and produces a natural-language answer.
    /// `AgentOutputMode::Sql` terminates after the agent generates
    /// SQL: pre-validated paths (semantic-layer, verified `.sql`
    /// files, vendor engines) skip execution entirely; LLM-generated
    /// SQL runs a `LIMIT 0` smoke check before terminating. The
    /// terminal answer is the SQL text, ready to be written to disk
    /// via the sibling `cache:` block and consumed by a downstream
    /// `execute_sql` task. Applies to **analytics agents**
    /// (`.agentic.yml`); ignored for the built-in builder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<AgentOutputConfig>,
}

/// Output shaping for an agent task.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentOutputConfig {
    #[serde(default)]
    pub mode: AgentOutputMode,
}

/// Output mode for an analytics agent task.
///
/// `Answer` (the default) runs the full analytics FSM
/// (clarifying -> specifying -> solving -> executing -> interpreting)
/// and emits a natural-language answer.
///
/// `Sql` shortcuts the FSM after the SQL is produced: pre-validated
/// paths (semantic-layer compile, verified `.sql` file match, vendor
/// engine) skip the executing state entirely; LLM-generated SQL runs
/// a `LIMIT 0` smoke check. Procedure delegation is incoherent with
/// SQL-gen mode (the SQL is only known after the procedure runs) and
/// is rejected at runtime when this mode is in effect.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentOutputMode {
    #[default]
    Answer,
    Sql,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatterConfig {
    pub template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubWorkflowConfig {
    pub src: PathBuf,
    pub variables: Option<HashMap<String, Value>>,
    /// Child workflow's tasks, pre-resolved at workflow load time.
    ///
    /// Populated by [`crate::resolve::resolve_subworkflows`] before the
    /// run starts so the decider can emit the full nested task DAG in
    /// `subrun_started` without doing async file IO at decide-time.
    /// Persisted as part of `WorkflowRunState.workflow` so resumes see
    /// the same tree without re-resolving. Empty when the child file
    /// is missing, fails to parse, or appears in a sub-workflow cycle.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolved_tasks: Vec<TaskConfig>,
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

// ── Semantic query config (re-exported from agentic-semantic) ──────────────
//
// These types now live in `agentic_semantic::config` so the analytics
// domain can construct them without taking a dep on `agentic-workflow`.
// Re-exported here to keep existing call sites working.

pub use agentic_semantic::config::{
    ArrayFilter, DateRangeFilter, ScalarFilter, SemanticFilter, SemanticFilterType, SemanticOrder,
    SemanticQueryConfig, TimeDimensionConfig, TimeGranularity,
};

// ── Defaults ────────────────────────────────────────────────────────────────

fn default_one() -> usize {
    1
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
    agent_ref: agents/default.agentic.yml
    prompt: "Analyze the data"
    consistency_run: 3
"#;
        let config: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        let TaskType::Agent(agent) = &config.tasks[0].task_type else {
            panic!("expected Agent");
        };
        assert_eq!(agent.agent_ref, "agents/default.agentic.yml");
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

    /// Legacy `cache: { enabled, path }` block round-trips through
    /// the new schema unchanged — existing customer YAML keeps
    /// parsing without modification.
    #[test]
    fn test_parse_legacy_cache_block() {
        let yaml = r#"
name: test
tasks:
  - name: sql
    type: agent
    agent_ref: agents/sql.agentic.yml
    prompt: "generate SQL"
    cache:
      enabled: true
      path: "out/cache/{{ groupings.value }}.sql"
"#;
        let config: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        let cache = config.tasks[0].cache.as_ref().expect("cache block parsed");
        assert!(cache.enabled);
        assert_eq!(cache.path, "out/cache/{{ groupings.value }}.sql");
    }

    /// `output: { mode: sql }` round-trips through the agent task
    /// config so the analytics pipeline can shortcut the FSM after
    /// SQL is produced.
    #[test]
    fn test_parse_agent_output_sql_mode() {
        let yaml = r#"
name: test
tasks:
  - name: gen_sql
    type: agent
    agent_ref: agents/sales.agentic.yml
    prompt: "{{ question }}"
    output:
      mode: sql
    cache:
      path: "out/{{ question | slugify }}.sql"
"#;
        let config: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        let TaskType::Agent(agent) = &config.tasks[0].task_type else {
            panic!("expected agent task");
        };
        let output = agent.output.as_ref().expect("output parsed");
        assert_eq!(output.mode, AgentOutputMode::Sql);
    }

    /// Default output mode is `Answer` — existing workflows without an
    /// `output:` block keep their natural-language interpretation.
    #[test]
    fn test_agent_output_mode_defaults_to_answer() {
        let yaml = r#"
name: test
tasks:
  - name: ask
    type: agent
    agent_ref: a.yml
    prompt: "x"
"#;
        let config: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        let TaskType::Agent(agent) = &config.tasks[0].task_type else {
            panic!("expected agent task");
        };
        assert!(agent.output.is_none());
    }

    /// `enabled` defaults to true when only the path is given. Matches
    /// the legacy `default_cache_enabled` behavior.
    #[test]
    fn test_cache_enabled_defaults_to_true() {
        let yaml = r#"
name: test
tasks:
  - name: sql
    type: agent
    agent_ref: a.yml
    prompt: "x"
    cache:
      path: "out/x.sql"
"#;
        let config: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.tasks[0].cache.as_ref().unwrap().enabled);
    }
}
