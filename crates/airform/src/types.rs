use serde::{Deserialize, Serialize};

/// Summary of a dbt project.
#[derive(Debug, Serialize, Deserialize)]
pub struct DbtProjectInfo {
    /// The dbt project name (from dbt_project.yml `name:` field).
    pub name: String,
    /// The directory name under `modeling/`. Use this as the API path
    /// parameter — it may differ from `name` when the folder uses hyphens but
    /// dbt_project.yml uses underscores (e.g. folder `scale-1000` vs name `scale_1000`).
    pub folder_name: String,
    pub project_dir: String,
    pub model_paths: Vec<String>,
    pub seed_paths: Vec<String>,
}

/// Schema-defined column (from schema.yml).
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeColumnDef {
    pub name: String,
    pub description: Option<String>,
    pub data_type: Option<String>,
}

/// Summary of a single node (model, seed, source, test, snapshot).
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeSummary {
    pub unique_id: String,
    pub name: String,
    pub resource_type: String,
    pub path: String,
    pub materialization: Option<String>,
    pub description: Option<String>,
    pub depends_on: Vec<String>,
    pub tags: Vec<String>,
    pub raw_sql: Option<String>,
    pub compiled_sql: Option<String>,
    /// Schema-defined columns (from schema.yml); populated for seeds and sources.
    pub columns: Vec<NodeColumnDef>,
    /// Source database (sources only).
    pub database: Option<String>,
    /// Source schema (sources only).
    pub schema: Option<String>,
}

/// Result of compiling the project.
#[derive(Debug, Serialize, Deserialize)]
pub struct CompileOutput {
    pub models_compiled: usize,
    pub errors: Vec<CompileErrorEntry>,
    pub nodes: Vec<CompiledNodeInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CompileErrorEntry {
    pub node_id: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CompiledNodeInfo {
    pub unique_id: String,
    pub name: String,
    pub compiled_sql: String,
}

/// Result of running models.
#[derive(Debug, Serialize, Deserialize)]
pub struct RunOutput {
    pub status: String,
    pub results: Vec<NodeRunResult>,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeRunResult {
    pub unique_id: String,
    pub name: String,
    pub status: String,
    pub duration_ms: u64,
    pub rows_affected: Option<usize>,
    pub message: Option<String>,
}

/// Result of running tests.
#[derive(Debug, Serialize, Deserialize)]
pub struct TestOutput {
    pub tests_run: usize,
    pub passed: usize,
    pub failed: usize,
    pub results: Vec<TestResultEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TestResultEntry {
    pub test_name: String,
    pub model_name: String,
    pub column_name: String,
    pub status: String,
    pub failures: usize,
    pub duration_ms: u64,
    pub message: Option<String>,
}

/// Result of analyzing the project.
#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyzeOutput {
    pub models_analyzed: usize,
    pub cached_count: usize,
    pub diagnostics: Vec<DiagnosticEntry>,
    pub contract_violations: Vec<ContractViolationEntry>,
    pub schemas: Vec<SchemaEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiagnosticEntry {
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContractViolationEntry {
    pub model: String,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SchemaEntry {
    pub name: String,
    pub columns: Vec<ColumnInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

/// Model-level lineage DAG.
#[derive(Debug, Serialize, Deserialize)]
pub struct LineageOutput {
    pub nodes: Vec<LineageNode>,
    pub edges: Vec<LineageEdge>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LineageNode {
    pub unique_id: String,
    pub name: String,
    pub resource_type: String,
    pub description: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LineageEdge {
    pub source: String,
    pub target: String,
}

/// Column-level lineage.
#[derive(Debug, Serialize, Deserialize)]
pub struct ColumnLineageOutput {
    pub edges: Vec<ColumnLineageEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ColumnLineageEntry {
    pub source_node: String,
    pub source_column: String,
    pub target_node: String,
    pub target_column: String,
    pub dependency_type: String,
}

/// Result of parsing the project manifest.
#[derive(Debug, Serialize, Deserialize)]
pub struct ParseOutput {
    pub models: usize,
    pub seeds: usize,
    pub snapshots: usize,
    pub tests: usize,
    pub sources: usize,
    pub nodes: usize,
    pub edges: usize,
    pub duration_ms: u64,
}

/// Result of loading seed CSV files.
#[derive(Debug, Serialize, Deserialize)]
pub struct SeedOutput {
    pub seeds_loaded: usize,
    pub results: Vec<NodeRunResult>,
}

/// Result of the debug health-check.
#[derive(Debug, Serialize, Deserialize)]
pub struct DebugOutput {
    pub project_name: String,
    pub version: Option<String>,
    pub profile: Option<String>,
    pub has_profiles_yml: bool,
    pub model_paths: Vec<String>,
    pub seed_paths: Vec<String>,
    pub model_count: usize,
    pub seed_count: usize,
    pub source_count: usize,
    pub all_ok: bool,
    pub issues: Vec<String>,
}

/// Result of cleaning target directories.
#[derive(Debug, Serialize, Deserialize)]
pub struct CleanOutput {
    pub cleaned: Vec<String>,
}

/// Result of generating documentation.
#[derive(Debug, Serialize, Deserialize)]
pub struct DocsOutput {
    pub manifest_path: String,
    pub nodes: usize,
    pub sources: usize,
}

/// Result of formatting SQL model files.
#[derive(Debug, Serialize, Deserialize)]
pub struct FormatOutput {
    pub files_checked: usize,
    pub files_changed: usize,
    /// Files that were (or would be) reformatted.
    pub files: Vec<String>,
}

/// Result of initializing a new dbt project scaffold.
#[derive(Debug, Serialize, Deserialize)]
pub struct InitOutput {
    pub project_name: String,
    pub project_dir: String,
    /// Files created: `(relative_path, content, description)`.
    pub files: Vec<(String, String, String)>,
}

/// Streaming event emitted by run_streaming.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunStreamEvent {
    NodeStarted { unique_id: String, name: String },
    NodeCompleted(NodeRunResult),
    Done { status: String, duration_ms: u64 },
    Error { message: String },
}

/// Request body for run/test endpoints.
#[derive(Debug, Deserialize)]
pub struct RunRequest {
    pub selector: Option<String>,
    #[serde(default)]
    pub full_refresh: bool,
}
