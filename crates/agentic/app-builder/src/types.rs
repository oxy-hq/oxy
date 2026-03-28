//! Domain types for the app builder FSM.

use agentic_analytics::ConversationTurn;
use agentic_analytics::SemanticCatalog;
use agentic_core::domain::Domain;
use serde::{Deserialize, Serialize};

// Re-export ResultShape from analytics for consistency.
pub use agentic_analytics::ResultShape;

// ---------------------------------------------------------------------------
// Intent
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppIntent {
    pub raw_request: String,
    pub app_name: Option<String>,
    pub desired_metrics: Vec<String>,
    pub desired_controls: Vec<String>,
    pub mentioned_tables: Vec<String>,
    pub history: Vec<ConversationTurn>,
    /// Key findings from the grounding phase: brief bullet-points of what the
    /// LLM discovered while exploring the catalog (column structures, value
    /// distributions, join paths, etc.). Passed to specifying so it can start
    /// with prior knowledge and skip redundant tool calls.
    #[serde(default)]
    pub key_findings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Spec
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSpec {
    pub intent: AppIntent,
    pub app_name: String,
    pub description: String,
    pub tasks: Vec<TaskPlan>,
    pub controls: Vec<ControlPlan>,
    pub layout: Vec<LayoutNode>,
    pub connector_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskPlan {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub expected_shape: ResultShape,
    #[serde(default)]
    pub expected_columns: Vec<String>,
    #[serde(default)]
    pub control_deps: Vec<String>,
    #[serde(default)]
    pub is_control_source: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlPlan {
    pub name: String,
    pub label: String,
    pub control_type: ControlType,
    #[serde(default)]
    pub source_task: Option<String>,
    /// Static options for select controls without a source_task.
    #[serde(default)]
    pub options: Vec<String>,
    pub default: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlType {
    Select,
    Date,
    Toggle,
}

impl std::fmt::Display for ControlType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ControlType::Select => write!(f, "select"),
            ControlType::Date => write!(f, "date"),
            ControlType::Toggle => write!(f, "toggle"),
        }
    }
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LayoutNode {
    Chart {
        task: String,
        preferred: ChartPreference,
    },
    Table {
        task: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    Row {
        columns: u32,
        children: Vec<LayoutNode>,
    },
    Markdown {
        content: String,
    },
    Insight {
        tasks: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        focus: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChartPreference {
    Bar,
    Line,
    Pie,
    Table,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChartType {
    Bar,
    Line,
    Pie,
    Table,
}

impl std::fmt::Display for ChartType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChartType::Bar => write!(f, "bar_chart"),
            ChartType::Line => write!(f, "line_chart"),
            ChartType::Pie => write!(f, "pie_chart"),
            ChartType::Table => write!(f, "table"),
        }
    }
}

// ---------------------------------------------------------------------------
// Solution
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AppSolution {
    pub tasks: Vec<ResolvedTask>,
    pub controls: Vec<ControlPlan>,
    pub layout: Vec<LayoutNode>,
    pub connector_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedTask {
    pub name: String,
    pub sql: String,
    pub is_control_source: bool,
    pub expected_shape: ResultShape,
    pub expected_columns: Vec<String>,
}

// ---------------------------------------------------------------------------
// Result
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AppResult {
    pub task_results: Vec<TaskResult>,
    pub controls: Vec<ControlPlan>,
    pub layout: Vec<LayoutNode>,
    pub connector_name: String,
}

#[derive(Debug, Clone)]
pub struct TaskResult {
    pub name: String,
    pub sql: String,
    pub columns: Vec<String>,
    /// Database-native type for each column (e.g. "INTEGER", "VARCHAR", "DATE").
    /// Aligned with `columns` by index. Empty when the connector cannot provide types.
    pub column_types: Vec<Option<String>>,
    pub row_count: usize,
    pub is_control_source: bool,
    pub expected_shape: ResultShape,
    pub expected_columns: Vec<String>,
    pub sample: agentic_core::result::QueryResult,
}

// ---------------------------------------------------------------------------
// Answer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppAnswer {
    pub yaml: String,
    pub summary: String,
    pub task_count: usize,
    pub control_count: usize,
}

// ---------------------------------------------------------------------------
// Validator
// ---------------------------------------------------------------------------

/// Trait for validating generated app YAML.
///
/// Implementations can perform lightweight parsing or full execution (run the
/// app's SQL tasks) depending on the layer.  The app-builder solver holds an
/// optional validator; when present the interpreting phase calls it after YAML
/// assembly and uses LLM patching to fix any reported errors.
#[async_trait::async_trait]
pub trait AppValidator: Send + Sync {
    /// Validate the generated app YAML.
    /// Returns `Ok(())` on success, `Err(errors)` with actionable messages.
    async fn validate(&self, yaml: &str) -> Result<(), Vec<String>>;
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum AppBuilderError {
    UnresolvedTable {
        table: String,
    },
    UnresolvedColumn {
        column: String,
    },
    SyntaxError {
        query: String,
        message: String,
    },
    EmptyResults {
        task_name: String,
    },
    ShapeMismatch {
        task_name: String,
        expected: String,
        actual: String,
    },
    InvalidSpec {
        errors: Vec<String>,
    },
    InvalidChartConfig {
        errors: Vec<String>,
    },
    NeedsUserInput {
        prompt: String,
    },
}

impl std::fmt::Display for AppBuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppBuilderError::UnresolvedTable { table } => {
                write!(f, "unresolved table: '{table}'")
            }
            AppBuilderError::UnresolvedColumn { column } => {
                write!(f, "unresolved column: '{column}'")
            }
            AppBuilderError::SyntaxError { message, .. } => {
                write!(f, "SQL syntax error: {message}")
            }
            AppBuilderError::EmptyResults { task_name } => {
                write!(f, "task '{task_name}' returned no rows")
            }
            AppBuilderError::ShapeMismatch {
                task_name,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "task '{task_name}' shape mismatch: expected {expected}, got {actual}"
                )
            }
            AppBuilderError::InvalidSpec { errors } => {
                write!(f, "invalid spec: {}", errors.join("; "))
            }
            AppBuilderError::InvalidChartConfig { errors } => {
                write!(f, "invalid chart config: {}", errors.join("; "))
            }
            AppBuilderError::NeedsUserInput { prompt } => {
                write!(f, "needs user input: {prompt}")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Domain
// ---------------------------------------------------------------------------

pub struct AppBuilderDomain;

impl Domain for AppBuilderDomain {
    type Intent = AppIntent;
    type Spec = AppSpec;
    type Solution = AppSolution;
    type Result = AppResult;
    type Answer = AppAnswer;
    type Catalog = SemanticCatalog;
    type Error = AppBuilderError;
}
