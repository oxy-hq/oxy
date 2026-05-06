//! Airform / dbt data-transformation tools for the builder copilot.
//!
//! - `list_dbt_projects`     — list all dbt projects under `modeling/`
//! - `list_dbt_nodes`        — list models, seeds, tests, and sources in a project
//! - `compile_dbt_model`     — compile one or all models to SQL
//! - `run_dbt_models`        — execute models and write Parquet outputs
//! - `test_dbt_models`       — run dbt data tests
//! - `get_dbt_lineage`       — return the model-level DAG
//! - `analyze_dbt_project`   — analyze SQL correctness, schemas, and contract violations
//! - `get_dbt_column_lineage`— return the column-level lineage DAG
//! - `parse_dbt_project`     — parse the manifest and validate the DAG
//! - `seed_dbt_project`      — load seed CSV files into the execution context
//! - `debug_dbt_project`     — health-check the project config and compilation
//! - `clean_dbt_project`     — remove target/ and other clean-target directories
//! - `docs_generate_dbt`     — write manifest.json documentation artifact
//! - `format_dbt_sql`        — uppercase SQL keywords in model files
//! - `init_dbt_project`      — scaffold a new dbt project under modeling/

use std::path::{Path, PathBuf};

use agentic_core::tools::{ToolDef, ToolError, ToolOutput};
use oxy::adapters::secrets::SecretsManager;
use oxy_airform::service::{self, AirformService};
use serde_json::{Value, json};

// ── Output structs ────────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct ProjectSummary {
    pub name: String,
    pub project_dir: String,
    pub model_paths: Vec<String>,
    pub seed_paths: Vec<String>,
}

pub struct ListProjectsOutput {
    pub projects: Vec<ProjectSummary>,
}

impl ToolOutput for ListProjectsOutput {
    fn to_agent_text(&self) -> String {
        if self.projects.is_empty() {
            return "No dbt projects found under modeling/.".to_string();
        }
        let mut lines = format!("Found {} dbt project(s):\n", self.projects.len());
        for p in &self.projects {
            lines.push_str(&format!("  - {} ({})\n", p.name, p.project_dir));
        }
        lines.trim_end().to_string()
    }
    fn to_value(&self) -> Value {
        let projects: Vec<Value> = self
            .projects
            .iter()
            .map(|p| {
                json!({
                    "name": p.name,
                    "project_dir": p.project_dir,
                    "model_paths": p.model_paths,
                    "seed_paths": p.seed_paths,
                })
            })
            .collect();
        json!({ "ok": true, "projects": projects, "count": self.projects.len() })
    }
}

#[derive(serde::Serialize)]
pub struct ColumnSummary {
    pub name: String,
    pub description: Option<String>,
    pub data_type: Option<String>,
}

#[derive(serde::Serialize)]
pub struct NodeSummary {
    pub unique_id: String,
    pub name: String,
    pub resource_type: String,
    pub path: Option<String>,
    pub materialization: Option<String>,
    pub description: Option<String>,
    pub depends_on: Vec<String>,
    pub tags: Vec<String>,
    pub raw_sql: Option<String>,
    pub compiled_sql: Option<String>,
    pub columns: Vec<ColumnSummary>,
}

pub struct ListNodesOutput {
    pub project: String,
    pub nodes: Vec<NodeSummary>,
}

impl ToolOutput for ListNodesOutput {
    fn to_agent_text(&self) -> String {
        if self.nodes.is_empty() {
            return format!("Project '{}' has no nodes.", self.project);
        }
        let mut models: Vec<&NodeSummary> = Vec::new();
        let mut seeds: Vec<&NodeSummary> = Vec::new();
        let mut tests: Vec<&NodeSummary> = Vec::new();
        let mut sources: Vec<&NodeSummary> = Vec::new();
        let mut other: Vec<&NodeSummary> = Vec::new();
        for n in &self.nodes {
            match n.resource_type.as_str() {
                "model" => models.push(n),
                "seed" => seeds.push(n),
                "test" => tests.push(n),
                "source" => sources.push(n),
                _ => other.push(n),
            }
        }
        let mut out = format!(
            "Project '{}' — {} node(s)\n",
            self.project,
            self.nodes.len()
        );
        let write_group = |out: &mut String, label: &str, nodes: &[&NodeSummary]| {
            if nodes.is_empty() {
                return;
            }
            out.push_str(&format!("\n{} ({}):\n", label, nodes.len()));
            for n in nodes {
                let mat = n
                    .materialization
                    .as_deref()
                    .map(|m| format!(", {m}"))
                    .unwrap_or_default();
                let path = n
                    .path
                    .as_deref()
                    .map(|p| format!(" [{}]", p))
                    .unwrap_or_default();
                let deps = if n.depends_on.is_empty() {
                    String::new()
                } else {
                    format!(" — depends_on: {}", n.depends_on.join(", "))
                };
                out.push_str(&format!("  {}{}{}{}\n", n.name, path, mat, deps));
            }
        };
        write_group(&mut out, "Models", &models);
        write_group(&mut out, "Seeds", &seeds);
        write_group(&mut out, "Tests", &tests);
        write_group(&mut out, "Sources", &sources);
        write_group(&mut out, "Other", &other);
        out.trim_end().to_string()
    }
    fn to_value(&self) -> Value {
        let nodes: Vec<Value> = self
            .nodes
            .iter()
            .map(|n| {
                json!({
                    "unique_id": n.unique_id,
                    "name": n.name,
                    "resource_type": n.resource_type,
                    "path": n.path,
                    "materialization": n.materialization,
                    "description": n.description,
                    "depends_on": n.depends_on,
                    "tags": n.tags,
                    "raw_sql": n.raw_sql,
                    "compiled_sql": n.compiled_sql,
                    "columns": n.columns.iter().map(|c| json!({
                        "name": c.name,
                        "description": c.description,
                        "data_type": c.data_type,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        json!({ "ok": true, "count": self.nodes.len(), "nodes": nodes })
    }
}

pub struct CompileSingleOutput {
    pub project: String,
    pub model: String,
    pub compiled_sql: String,
}

impl ToolOutput for CompileSingleOutput {
    fn to_agent_text(&self) -> String {
        format!(
            "Compiled model '{}' in project '{}':\n\n```sql\n{}\n```",
            self.model, self.project, self.compiled_sql
        )
    }
    fn to_value(&self) -> Value {
        json!({
            "ok": true,
            "model": self.model,
            "compiled_sql": self.compiled_sql,
        })
    }
}

#[derive(serde::Serialize)]
pub struct CompileError {
    pub node_id: String,
    pub message: String,
}

#[derive(serde::Serialize)]
pub struct CompiledNode {
    pub name: String,
    pub unique_id: String,
    pub compiled_sql: Option<String>,
}

pub struct CompileAllOutput {
    pub project: String,
    pub models_compiled: usize,
    pub errors: Vec<CompileError>,
    pub nodes: Vec<CompiledNode>,
}

impl ToolOutput for CompileAllOutput {
    fn to_agent_text(&self) -> String {
        let mut out = format!(
            "Compiled {} model(s) in '{}', {} error(s).\n",
            self.models_compiled,
            self.project,
            self.errors.len()
        );
        if !self.errors.is_empty() {
            out.push_str("\nErrors:\n");
            for e in &self.errors {
                out.push_str(&format!("  [{}] {}\n", e.node_id, e.message));
            }
        }
        out.trim_end().to_string()
    }
    fn to_value(&self) -> Value {
        json!({
            "ok": true,
            "models_compiled": self.models_compiled,
            "errors": self.errors.iter().map(|e| json!({ "node_id": e.node_id, "message": e.message })).collect::<Vec<_>>(),
            "nodes": self.nodes.iter().map(|n| json!({ "name": n.name, "unique_id": n.unique_id, "compiled_sql": n.compiled_sql })).collect::<Vec<_>>(),
        })
    }
}

#[derive(serde::Serialize)]
pub struct ModelRunResult {
    pub unique_id: String,
    pub name: String,
    pub status: String,
    pub duration_ms: u64,
    pub rows_affected: Option<u64>,
    pub message: Option<String>,
}

pub struct RunDbtModelsOutput {
    pub project: String,
    pub status: String,
    pub duration_ms: u64,
    pub results: Vec<ModelRunResult>,
}

impl ToolOutput for RunDbtModelsOutput {
    fn to_agent_text(&self) -> String {
        let mut out = format!(
            "Run completed in {}ms — status: {}\n",
            self.duration_ms, self.status
        );
        if !self.results.is_empty() {
            out.push_str("\nResults:\n");
            for r in &self.results {
                let tick = if r.status == "success" { "✓" } else { "✗" };
                let rows = r
                    .rows_affected
                    .map(|n| format!(", {n} rows"))
                    .unwrap_or_default();
                let msg = r
                    .message
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .map(|s| format!(" — {s}"))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "  {} {}  ({}ms{}){}\n",
                    tick, r.name, r.duration_ms, rows, msg
                ));
            }
        }
        out.trim_end().to_string()
    }
    fn to_value(&self) -> Value {
        json!({
            "ok": true,
            "status": self.status,
            "duration_ms": self.duration_ms,
            "results": self.results.iter().map(|r| json!({
                "unique_id": r.unique_id,
                "name": r.name,
                "status": r.status,
                "duration_ms": r.duration_ms,
                "rows_affected": r.rows_affected,
                "message": r.message,
            })).collect::<Vec<_>>(),
        })
    }
}

#[derive(serde::Serialize)]
pub struct TestResult {
    pub test_name: String,
    pub model_name: Option<String>,
    pub column_name: Option<String>,
    pub status: String,
    pub failures: Option<u64>,
    pub duration_ms: u64,
    pub message: Option<String>,
}

pub struct TestDbtModelsOutput {
    pub project: String,
    pub tests_run: usize,
    pub passed: usize,
    pub failed: usize,
    pub results: Vec<TestResult>,
}

impl ToolOutput for TestDbtModelsOutput {
    fn to_agent_text(&self) -> String {
        let mut out = format!(
            "Tests for '{}': {} run, {} passed, {} failed.\n",
            self.project, self.tests_run, self.passed, self.failed
        );
        if !self.results.is_empty() {
            out.push_str("\nResults:\n");
            for r in &self.results {
                let tick = if r.status == "pass" { "✓" } else { "✗" };
                let col = r
                    .column_name
                    .as_deref()
                    .map(|c| format!(", column: {c}"))
                    .unwrap_or_default();
                let model = r
                    .model_name
                    .as_deref()
                    .map(|m| format!(" ({m}{col})"))
                    .unwrap_or_default();
                let failures = r
                    .failures
                    .filter(|&f| f > 0)
                    .map(|f| format!(" — {f} failure(s)"))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "  {} {}{}{}\n",
                    tick, r.test_name, model, failures
                ));
            }
        }
        out.trim_end().to_string()
    }
    fn to_value(&self) -> Value {
        json!({
            "ok": true,
            "tests_run": self.tests_run,
            "passed": self.passed,
            "failed": self.failed,
            "results": self.results.iter().map(|r| json!({
                "test_name": r.test_name,
                "model_name": r.model_name,
                "column_name": r.column_name,
                "status": r.status,
                "failures": r.failures,
                "duration_ms": r.duration_ms,
                "message": r.message,
            })).collect::<Vec<_>>(),
        })
    }
}

#[derive(serde::Serialize)]
pub struct LineageNode {
    pub unique_id: String,
    pub name: String,
    pub resource_type: String,
    pub description: Option<String>,
    pub path: Option<String>,
}

#[derive(serde::Serialize)]
pub struct Edge {
    pub source: String,
    pub target: String,
}

pub struct DbtLineageOutput {
    pub project: String,
    pub nodes: Vec<LineageNode>,
    pub edges: Vec<Edge>,
}

impl ToolOutput for DbtLineageOutput {
    fn to_agent_text(&self) -> String {
        let mut out = format!(
            "DAG for '{}' — {} node(s), {} edge(s)\n",
            self.project,
            self.nodes.len(),
            self.edges.len()
        );
        if !self.nodes.is_empty() {
            out.push_str("\nNodes:\n");
            for n in &self.nodes {
                out.push_str(&format!("  {} ({})\n", n.name, n.resource_type));
            }
        }
        if !self.edges.is_empty() {
            out.push_str("\nDependencies:\n");
            for e in &self.edges {
                out.push_str(&format!("  {} → {}\n", e.source, e.target));
            }
        }
        out.trim_end().to_string()
    }
    fn to_value(&self) -> Value {
        json!({
            "ok": true,
            "nodes": self.nodes.iter().map(|n| json!({
                "unique_id": n.unique_id,
                "name": n.name,
                "resource_type": n.resource_type,
                "description": n.description,
                "path": n.path,
            })).collect::<Vec<_>>(),
            "edges": self.edges.iter().map(|e| json!({ "source": e.source, "target": e.target })).collect::<Vec<_>>(),
        })
    }
}

#[derive(serde::Serialize)]
pub struct Diagnostic {
    pub kind: String,
    pub message: String,
}

#[derive(serde::Serialize)]
pub struct ContractViolation {
    pub model: String,
    pub kind: String,
    pub message: String,
}

#[derive(serde::Serialize)]
pub struct SchemaColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

#[derive(serde::Serialize)]
pub struct ModelSchema {
    pub name: String,
    pub columns: Vec<SchemaColumn>,
}

pub struct AnalyzeDbtProjectOutput {
    pub project: String,
    pub models_analyzed: usize,
    pub cached_count: usize,
    pub diagnostics: Vec<Diagnostic>,
    pub contract_violations: Vec<ContractViolation>,
    pub schemas: Vec<ModelSchema>,
}

impl ToolOutput for AnalyzeDbtProjectOutput {
    fn to_agent_text(&self) -> String {
        let mut out = format!(
            "Analysis of '{}' — {} model(s) analyzed ({} cached)\n",
            self.project, self.models_analyzed, self.cached_count
        );
        if !self.diagnostics.is_empty() {
            out.push_str(&format!("\nDiagnostics ({}):\n", self.diagnostics.len()));
            for d in &self.diagnostics {
                out.push_str(&format!("  {:8}  {}\n", d.kind.to_uppercase(), d.message));
            }
        }
        if !self.contract_violations.is_empty() {
            out.push_str(&format!(
                "\nContract violations ({}):\n",
                self.contract_violations.len()
            ));
            for v in &self.contract_violations {
                out.push_str(&format!("  [{}: {}] {}\n", v.model, v.kind, v.message));
            }
        }
        if !self.schemas.is_empty() {
            out.push_str("\nSchemas:\n");
            for s in &self.schemas {
                let cols: Vec<String> = s
                    .columns
                    .iter()
                    .map(|c| {
                        let null = if c.nullable { ", nullable" } else { "" };
                        format!("{} ({}{})", c.name, c.data_type, null)
                    })
                    .collect();
                out.push_str(&format!("  {}: {}\n", s.name, cols.join(", ")));
            }
        }
        out.trim_end().to_string()
    }
    fn to_value(&self) -> Value {
        json!({
            "ok": true,
            "models_analyzed": self.models_analyzed,
            "cached_count": self.cached_count,
            "diagnostics": self.diagnostics.iter().map(|d| json!({ "kind": d.kind, "message": d.message })).collect::<Vec<_>>(),
            "contract_violations": self.contract_violations.iter().map(|v| json!({ "model": v.model, "kind": v.kind, "message": v.message })).collect::<Vec<_>>(),
            "schemas": self.schemas.iter().map(|s| json!({
                "name": s.name,
                "columns": s.columns.iter().map(|c| json!({ "name": c.name, "data_type": c.data_type, "nullable": c.nullable })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })
    }
}

#[derive(serde::Serialize)]
pub struct ColumnEdge {
    pub source_node: String,
    pub source_column: String,
    pub target_node: String,
    pub target_column: String,
    pub dependency_type: String,
}

pub struct DbtColumnLineageOutput {
    pub project: String,
    pub edges: Vec<ColumnEdge>,
}

impl ToolOutput for DbtColumnLineageOutput {
    fn to_agent_text(&self) -> String {
        let mut out = format!(
            "Column lineage for '{}' — {} edge(s)\n",
            self.project,
            self.edges.len()
        );
        if !self.edges.is_empty() {
            out.push_str("\n");
            for e in &self.edges {
                out.push_str(&format!(
                    "  {}.{} → {}.{} ({})\n",
                    e.source_node,
                    e.source_column,
                    e.target_node,
                    e.target_column,
                    e.dependency_type
                ));
            }
        }
        out.trim_end().to_string()
    }
    fn to_value(&self) -> Value {
        json!({
            "ok": true,
            "edges": self.edges.iter().map(|e| json!({
                "source_node": e.source_node,
                "source_column": e.source_column,
                "target_node": e.target_node,
                "target_column": e.target_column,
                "dependency_type": e.dependency_type,
            })).collect::<Vec<_>>(),
        })
    }
}

pub struct ParseDbtProjectOutput {
    pub project: String,
    pub models: u64,
    pub seeds: u64,
    pub snapshots: u64,
    pub tests: u64,
    pub sources: u64,
    pub nodes: u64,
    pub edges: u64,
    pub duration_ms: u64,
}

impl ToolOutput for ParseDbtProjectOutput {
    fn to_agent_text(&self) -> String {
        format!(
            "Parsed '{}' in {}ms\n\n  Models: {}, Seeds: {}, Sources: {}, Tests: {}, Snapshots: {}\n  DAG: {} nodes, {} edges",
            self.project,
            self.duration_ms,
            self.models,
            self.seeds,
            self.sources,
            self.tests,
            self.snapshots,
            self.nodes,
            self.edges,
        )
    }
    fn to_value(&self) -> Value {
        json!({
            "ok": true,
            "models": self.models,
            "seeds": self.seeds,
            "snapshots": self.snapshots,
            "tests": self.tests,
            "sources": self.sources,
            "nodes": self.nodes,
            "edges": self.edges,
            "duration_ms": self.duration_ms,
        })
    }
}

pub struct SeedDbtProjectOutput {
    pub project: String,
    pub seeds_loaded: usize,
    pub results: Vec<ModelRunResult>,
}

impl ToolOutput for SeedDbtProjectOutput {
    fn to_agent_text(&self) -> String {
        let mut out = format!(
            "Seeded {} file(s) in '{}'.\n",
            self.seeds_loaded, self.project
        );
        if !self.results.is_empty() {
            out.push_str("\n");
            for r in &self.results {
                let tick = if r.status == "success" { "✓" } else { "✗" };
                let rows = r
                    .rows_affected
                    .map(|n| format!(", {n} rows"))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "  {} {}  ({}ms{})\n",
                    tick, r.name, r.duration_ms, rows
                ));
            }
        }
        out.trim_end().to_string()
    }
    fn to_value(&self) -> Value {
        json!({
            "ok": true,
            "seeds_loaded": self.seeds_loaded,
            "results": self.results.iter().map(|r| json!({
                "unique_id": r.unique_id,
                "name": r.name,
                "status": r.status,
                "duration_ms": r.duration_ms,
                "rows_affected": r.rows_affected,
                "message": r.message,
            })).collect::<Vec<_>>(),
        })
    }
}

pub struct DebugDbtProjectOutput {
    pub project_name: String,
    pub version: String,
    pub profile: String,
    pub has_profiles_yml: bool,
    pub model_paths: Vec<String>,
    pub seed_paths: Vec<String>,
    pub model_count: u64,
    pub seed_count: u64,
    pub source_count: u64,
    pub all_ok: bool,
    pub issues: Vec<String>,
}

impl ToolOutput for DebugDbtProjectOutput {
    fn to_agent_text(&self) -> String {
        let status = if self.all_ok {
            "all checks passed"
        } else {
            "issues found"
        };
        let profiles = if self.has_profiles_yml {
            "profiles.yml present"
        } else {
            "profiles.yml MISSING"
        };
        let mut out = format!(
            "Health check for '{}' (dbt {}, profile: {}) — {}\n\n  {}\n  Models: {}, Seeds: {}, Sources: {}",
            self.project_name,
            self.version,
            self.profile,
            status,
            profiles,
            self.model_count,
            self.seed_count,
            self.source_count,
        );
        if !self.issues.is_empty() {
            out.push_str(&format!("\n\nIssues ({}):\n", self.issues.len()));
            for issue in &self.issues {
                out.push_str(&format!("  - {issue}\n"));
            }
        }
        out.trim_end().to_string()
    }
    fn to_value(&self) -> Value {
        json!({
            "ok": true,
            "project_name": self.project_name,
            "version": self.version,
            "profile": self.profile,
            "has_profiles_yml": self.has_profiles_yml,
            "model_paths": self.model_paths,
            "seed_paths": self.seed_paths,
            "model_count": self.model_count,
            "seed_count": self.seed_count,
            "source_count": self.source_count,
            "all_ok": self.all_ok,
            "issues": self.issues,
        })
    }
}

pub struct CleanDbtProjectOutput {
    pub project: String,
    pub cleaned: Vec<String>,
}

impl ToolOutput for CleanDbtProjectOutput {
    fn to_agent_text(&self) -> String {
        if self.cleaned.is_empty() {
            return format!("Nothing to clean in '{}'.", self.project);
        }
        let mut out = format!(
            "Cleaned {} director(y/ies) in '{}':\n",
            self.cleaned.len(),
            self.project
        );
        for path in &self.cleaned {
            out.push_str(&format!("  - {path}\n"));
        }
        out.trim_end().to_string()
    }
    fn to_value(&self) -> Value {
        json!({ "ok": true, "cleaned": self.cleaned })
    }
}

pub struct DocsGenerateDbtOutput {
    pub project: String,
    pub manifest_path: String,
    pub nodes: u64,
    pub sources: u64,
}

impl ToolOutput for DocsGenerateDbtOutput {
    fn to_agent_text(&self) -> String {
        format!(
            "Documentation generated for '{}': {}\n  {} node(s), {} source(s)",
            self.project, self.manifest_path, self.nodes, self.sources
        )
    }
    fn to_value(&self) -> Value {
        json!({
            "ok": true,
            "manifest_path": self.manifest_path,
            "nodes": self.nodes,
            "sources": self.sources,
        })
    }
}

pub struct FormatDbtSqlOutput {
    pub project: String,
    pub check_only: bool,
    pub files_checked: u64,
    pub files_changed: u64,
    pub files: Vec<String>,
}

impl ToolOutput for FormatDbtSqlOutput {
    fn to_agent_text(&self) -> String {
        if self.check_only {
            let mut out = format!(
                "{} of {} file(s) would be reformatted in '{}' (check mode — no files modified).",
                self.files_changed, self.files_checked, self.project
            );
            if !self.files.is_empty() {
                out.push_str("\n\nFiles that would change:\n");
                for f in &self.files {
                    out.push_str(&format!("  {f}\n"));
                }
            }
            out.trim_end().to_string()
        } else {
            let mut out = format!(
                "Formatted {} of {} file(s) in '{}'.",
                self.files_changed, self.files_checked, self.project
            );
            if !self.files.is_empty() {
                out.push_str("\n\nFiles changed:\n");
                for f in &self.files {
                    out.push_str(&format!("  {f}\n"));
                }
            }
            out.trim_end().to_string()
        }
    }
    fn to_value(&self) -> Value {
        json!({
            "ok": true,
            "files_checked": self.files_checked,
            "files_changed": self.files_changed,
            "files": self.files,
        })
    }
}

pub struct InitDbtProjectOutput {
    pub project_name: String,
    pub project_dir: String,
    /// (relative_path, content, description)
    pub files: Vec<(String, String, String)>,
}

impl ToolOutput for InitDbtProjectOutput {
    fn to_agent_text(&self) -> String {
        let mut out = format!(
            "Initialized dbt project '{}' at {}.\n\nFiles created ({}):\n",
            self.project_name,
            self.project_dir,
            self.files.len()
        );
        for (path, _, desc) in &self.files {
            let desc_part = if desc.is_empty() {
                String::new()
            } else {
                format!(" — {desc}")
            };
            out.push_str(&format!("  {path}{desc_part}\n"));
        }
        out.trim_end().to_string()
    }
    fn to_value(&self) -> Value {
        let files: Vec<Value> = self
            .files
            .iter()
            .map(|(path, content, desc)| json!([path, content, desc]))
            .collect();
        json!({
            "ok": true,
            "project_name": self.project_name,
            "project_dir": self.project_dir,
            "files": files,
        })
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn validate_project_name(name: &str) -> Result<(), ToolError> {
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        Err(ToolError::BadParams(format!(
            "invalid project name: {name}"
        )))
    } else {
        Ok(())
    }
}

fn project_dir(workspace_root: &Path, project_name: &str) -> Result<PathBuf, ToolError> {
    validate_project_name(project_name)?;
    Ok(workspace_root.join("modeling").join(project_name))
}

async fn make_service(
    workspace_root: &Path,
    project_name: &str,
    secrets_manager: Option<&SecretsManager>,
) -> Result<AirformService, ToolError> {
    let dir = project_dir(workspace_root, project_name)?;
    let svc = AirformService::new(dir);

    let svc = if let Some(sm) = secrets_manager {
        let config_manager = oxy::config::ConfigBuilder::new()
            .with_workspace_path(workspace_root)
            .map_err(|e| ToolError::Execution(format!("config error: {e}")))?
            .build()
            .await
            .map_err(|e| ToolError::Execution(format!("config error: {e}")))?;
        svc.with_oxy_context(config_manager, sm.clone())
    } else {
        svc
    };

    Ok(svc)
}

// ── Tool definitions ──────────────────────────────────────────────────────────

pub fn list_dbt_projects_def() -> ToolDef {
    ToolDef {
        name: "list_dbt_projects",
        description: "List all dbt/airform data transformation projects in this Oxy workspace. \
                      Projects live under `modeling/` and each contains a `dbt_project.yml`.",
        parameters: json!({
            "type": "object",
            "properties": {},
            "required": [],
            "additionalProperties": false
        }),
        strict: false,
        ..Default::default()
    }
}

pub fn list_dbt_nodes_def() -> ToolDef {
    ToolDef {
        name: "list_dbt_nodes",
        description: "List all nodes (models, seeds, tests, sources) in a dbt project.",
        parameters: json!({
            "type": "object",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Name of the dbt project directory under modeling/"
                }
            },
            "required": ["project"],
            "additionalProperties": false
        }),
        strict: false,
        ..Default::default()
    }
}

pub fn compile_dbt_model_def() -> ToolDef {
    ToolDef {
        name: "compile_dbt_model",
        description: "Compile one or all dbt models in a project to their final SQL. \
                      Resolves `{{ ref() }}` / `{{ source() }}` Jinja macros. \
                      Provide `model` to compile a single model; omit it to compile all.",
        parameters: json!({
            "type": "object",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Name of the dbt project directory under modeling/"
                },
                "model": {
                    "type": ["string", "null"],
                    "description": "Model name to compile. Pass null to compile the whole project."
                }
            },
            "required": ["project", "model"],
            "additionalProperties": false
        }),
        strict: false,
        ..Default::default()
    }
}

pub fn run_dbt_models_def() -> ToolDef {
    ToolDef {
        name: "run_dbt_models",
        description: "Execute dbt models, materializing results as Parquet files in the \
                      configured output directory. Optionally filter by a selector string \
                      (matched against unique_id or model name). Loads Oxy source databases \
                      automatically when configured. \
                      Requires an `oxy.yml` in the project directory mapping each dbt target \
                      name to an Oxy database name; the dbt target type (profiles.yml `type:`) \
                      must match the Oxy database type (config.yml) — e.g. a `snowflake` target \
                      must map to a snowflake database. A type mismatch returns a \
                      DatabaseTypeMismatch error.",
        parameters: json!({
            "type": "object",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Name of the dbt project directory under modeling/"
                },
                "selector": {
                    "type": ["string", "null"],
                    "description": "Model selector (matches against unique_id or model name). Pass null to run all models."
                }
            },
            "required": ["project", "selector"],
            "additionalProperties": false
        }),
        strict: false,
        ..Default::default()
    }
}

pub fn test_dbt_models_def() -> ToolDef {
    ToolDef {
        name: "test_dbt_models",
        description: "Run dbt data-quality tests (not_null, unique, accepted_values, \
                      relationships, and custom tests) for a project. \
                      Requires an `oxy.yml` mapping dbt target names to Oxy database names; \
                      the dbt target type must match the Oxy database type (same rules as \
                      run_dbt_models).",
        parameters: json!({
            "type": "object",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Name of the dbt project directory under modeling/"
                },
                "selector": {
                    "type": ["string", "null"],
                    "description": "Selector to filter which models are run before testing. Pass null to run all."
                }
            },
            "required": ["project", "selector"],
            "additionalProperties": false
        }),
        strict: false,
        ..Default::default()
    }
}

pub fn get_dbt_lineage_def() -> ToolDef {
    ToolDef {
        name: "get_dbt_lineage",
        description: "Return the model-level DAG for a dbt project as a list of nodes and \
                      directed edges. Useful for understanding dependencies before modifying a model.",
        parameters: json!({
            "type": "object",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Name of the dbt project directory under modeling/"
                }
            },
            "required": ["project"],
            "additionalProperties": false
        }),
        strict: false,
        ..Default::default()
    }
}

pub fn analyze_dbt_project_def() -> ToolDef {
    ToolDef {
        name: "analyze_dbt_project",
        description: "Analyze a dbt project by compiling all models and inferring their output \
                      schemas using DataFusion. Reports SQL errors, schema-unavailable warnings, \
                      and contract violations (columns declared in schema.yml that are missing \
                      or type-mismatched in the actual output).",
        parameters: json!({
            "type": "object",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Name of the dbt project directory under modeling/"
                }
            },
            "required": ["project"],
            "additionalProperties": false
        }),
        strict: false,
        ..Default::default()
    }
}

pub fn get_dbt_column_lineage_def() -> ToolDef {
    ToolDef {
        name: "get_dbt_column_lineage",
        description: "Return the column-level lineage DAG for a dbt project. Each edge \
                      describes how a column in one node derives from a column in another.",
        parameters: json!({
            "type": "object",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Name of the dbt project directory under modeling/"
                }
            },
            "required": ["project"],
            "additionalProperties": false
        }),
        strict: false,
        ..Default::default()
    }
}

pub fn parse_dbt_project_def() -> ToolDef {
    ToolDef {
        name: "parse_dbt_project",
        description: "Parse the dbt project manifest and validate the dependency DAG. \
                      Reports model, seed, snapshot, test, and source counts along with \
                      DAG node/edge counts and parse duration.",
        parameters: json!({
            "type": "object",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Name of the dbt project directory under modeling/"
                }
            },
            "required": ["project"],
            "additionalProperties": false
        }),
        strict: false,
        ..Default::default()
    }
}

pub fn seed_dbt_project_def() -> ToolDef {
    ToolDef {
        name: "seed_dbt_project",
        description: "Load seed CSV files defined in the dbt project into the DataFusion \
                      execution context.",
        parameters: json!({
            "type": "object",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Name of the dbt project directory under modeling/"
                }
            },
            "required": ["project"],
            "additionalProperties": false
        }),
        strict: false,
        ..Default::default()
    }
}

pub fn debug_dbt_project_def() -> ToolDef {
    ToolDef {
        name: "debug_dbt_project",
        description: "Run a health-check on a dbt project: validates dbt_project.yml, \
                      profiles.yml, and attempts a compilation pass. Reports any issues found.",
        parameters: json!({
            "type": "object",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Name of the dbt project directory under modeling/"
                }
            },
            "required": ["project"],
            "additionalProperties": false
        }),
        strict: false,
        ..Default::default()
    }
}

pub fn clean_dbt_project_def() -> ToolDef {
    ToolDef {
        name: "clean_dbt_project",
        description: "Remove directories listed in `clean-targets` (typically `target/` and \
                      `dbt_packages/`) from the dbt project.",
        parameters: json!({
            "type": "object",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Name of the dbt project directory under modeling/"
                }
            },
            "required": ["project"],
            "additionalProperties": false
        }),
        strict: false,
        ..Default::default()
    }
}

pub fn docs_generate_dbt_def() -> ToolDef {
    ToolDef {
        name: "docs_generate_dbt",
        description: "Generate project documentation by writing `manifest.json` into the \
                      project's target directory.",
        parameters: json!({
            "type": "object",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Name of the dbt project directory under modeling/"
                }
            },
            "required": ["project"],
            "additionalProperties": false
        }),
        strict: false,
        ..Default::default()
    }
}

pub fn format_dbt_sql_def() -> ToolDef {
    ToolDef {
        name: "format_dbt_sql",
        description: "Uppercase SQL keywords in all model `.sql` files of a dbt project. \
                      Use `check: true` to preview which files would change without modifying them \
                      (defaults to false — files are reformatted in place).",
        parameters: json!({
            "type": "object",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Name of the dbt project directory under modeling/"
                },
                "check": {
                    "type": "boolean",
                    "description": "When true, report which files would change without modifying them. Defaults to false."
                }
            },
            "required": ["project", "check"],
            "additionalProperties": false
        }),
        strict: false,
        ..Default::default()
    }
}

pub fn init_dbt_project_def() -> ToolDef {
    ToolDef {
        name: "init_dbt_project",
        description: "Scaffold a new dbt project under `modeling/<name>/`. \
                      Creates the standard directory layout (models/staging, models/marts, \
                      seeds, tests, macros, snapshots, analyses) and writes a default \
                      `dbt_project.yml` and `profiles.yml` (DuckDB in-memory). \
                      Fails if `modeling/<name>/` already exists.",
        parameters: json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name for the new dbt project (becomes the folder name under modeling/)"
                }
            },
            "required": ["name"],
            "additionalProperties": false
        }),
        strict: false,
        ..Default::default()
    }
}

// ── Executors ─────────────────────────────────────────────────────────────────

pub fn execute_list_dbt_projects(
    workspace_root: &Path,
    _params: &Value,
) -> Result<ListProjectsOutput, ToolError> {
    let projects = service::list_projects(workspace_root);
    let summaries = projects
        .into_iter()
        .map(|p| ProjectSummary {
            name: p.name,
            project_dir: p.project_dir,
            model_paths: p.model_paths,
            seed_paths: p.seed_paths,
        })
        .collect();
    Ok(ListProjectsOutput {
        projects: summaries,
    })
}

pub fn execute_list_dbt_nodes(
    workspace_root: &Path,
    params: &Value,
) -> Result<ListNodesOutput, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;

    let svc = AirformService::new(project_dir(workspace_root, project_name)?);
    svc.list_nodes()
        .map(|nodes| ListNodesOutput {
            project: project_name.to_string(),
            nodes: nodes
                .into_iter()
                .map(|n| NodeSummary {
                    unique_id: n.unique_id,
                    name: n.name,
                    resource_type: n.resource_type,
                    path: Some(n.path),
                    materialization: n.materialization,
                    description: n.description,
                    depends_on: n.depends_on,
                    tags: n.tags,
                    raw_sql: n.raw_sql,
                    compiled_sql: n.compiled_sql,
                    columns: n
                        .columns
                        .into_iter()
                        .map(|c| ColumnSummary {
                            name: c.name,
                            description: c.description,
                            data_type: c.data_type,
                        })
                        .collect(),
                })
                .collect(),
        })
        .map_err(|e| ToolError::Execution(e.to_string()))
}

pub fn execute_compile_dbt_model_single(
    workspace_root: &Path,
    project_name: &str,
    model_name: &str,
) -> Result<CompileSingleOutput, ToolError> {
    let svc = AirformService::new(project_dir(workspace_root, project_name)?);
    svc.compile_model(model_name)
        .map(|sql| CompileSingleOutput {
            project: project_name.to_string(),
            model: model_name.to_string(),
            compiled_sql: sql,
        })
        .map_err(|e| ToolError::Execution(e.to_string()))
}

pub fn execute_compile_dbt_model_all(
    workspace_root: &Path,
    project_name: &str,
) -> Result<CompileAllOutput, ToolError> {
    let svc = AirformService::new(project_dir(workspace_root, project_name)?);
    svc.compile_project()
        .map(|output| CompileAllOutput {
            project: project_name.to_string(),
            models_compiled: output.models_compiled as usize,
            errors: output
                .errors
                .into_iter()
                .map(|e| CompileError {
                    node_id: e.node_id,
                    message: e.message,
                })
                .collect(),
            nodes: output
                .nodes
                .into_iter()
                .map(|n| CompiledNode {
                    name: n.name,
                    unique_id: n.unique_id,
                    compiled_sql: Some(n.compiled_sql),
                })
                .collect(),
        })
        .map_err(|e| ToolError::Execution(e.to_string()))
}

pub async fn execute_run_dbt_models(
    workspace_root: &Path,
    params: &Value,
    secrets_manager: Option<&SecretsManager>,
) -> Result<RunDbtModelsOutput, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;
    let selector = params["selector"].as_str().filter(|s| !s.is_empty());
    let svc = make_service(workspace_root, project_name, secrets_manager).await?;
    svc.run(selector)
        .await
        .map(|output| RunDbtModelsOutput {
            project: project_name.to_string(),
            status: output.status,
            duration_ms: output.duration_ms,
            results: output
                .results
                .into_iter()
                .map(|r| ModelRunResult {
                    unique_id: r.unique_id,
                    name: r.name,
                    status: r.status,
                    duration_ms: r.duration_ms,
                    rows_affected: r.rows_affected.map(|n| n as u64),
                    message: r.message,
                })
                .collect(),
        })
        .map_err(|e| ToolError::Execution(e.to_string()))
}

pub async fn execute_test_dbt_models(
    workspace_root: &Path,
    params: &Value,
    secrets_manager: Option<&SecretsManager>,
) -> Result<TestDbtModelsOutput, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;
    let selector = params["selector"].as_str().filter(|s| !s.is_empty());
    let svc = make_service(workspace_root, project_name, secrets_manager).await?;
    svc.test(selector)
        .await
        .map(|output| TestDbtModelsOutput {
            project: project_name.to_string(),
            tests_run: output.tests_run as usize,
            passed: output.passed as usize,
            failed: output.failed as usize,
            results: output
                .results
                .into_iter()
                .map(|r| TestResult {
                    test_name: r.test_name,
                    model_name: Some(r.model_name),
                    column_name: Some(r.column_name),
                    status: r.status,
                    failures: Some(r.failures as u64),
                    duration_ms: r.duration_ms,
                    message: r.message,
                })
                .collect(),
        })
        .map_err(|e| ToolError::Execution(e.to_string()))
}

pub fn execute_get_dbt_lineage(
    workspace_root: &Path,
    params: &Value,
) -> Result<DbtLineageOutput, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;
    let svc = AirformService::new(project_dir(workspace_root, project_name)?);
    svc.get_lineage()
        .map(|output| DbtLineageOutput {
            project: project_name.to_string(),
            nodes: output
                .nodes
                .into_iter()
                .map(|n| LineageNode {
                    unique_id: n.unique_id,
                    name: n.name,
                    resource_type: n.resource_type,
                    description: n.description,
                    path: n.path,
                })
                .collect(),
            edges: output
                .edges
                .into_iter()
                .map(|e| Edge {
                    source: e.source,
                    target: e.target,
                })
                .collect(),
        })
        .map_err(|e| ToolError::Execution(e.to_string()))
}

pub async fn execute_analyze_dbt_project(
    workspace_root: &Path,
    params: &Value,
) -> Result<AnalyzeDbtProjectOutput, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;
    let svc = AirformService::new(project_dir(workspace_root, project_name)?);
    svc.analyze()
        .await
        .map(|output| AnalyzeDbtProjectOutput {
            project: project_name.to_string(),
            models_analyzed: output.models_analyzed as usize,
            cached_count: output.cached_count as usize,
            diagnostics: output
                .diagnostics
                .into_iter()
                .map(|d| Diagnostic {
                    kind: d.kind,
                    message: d.message,
                })
                .collect(),
            contract_violations: output
                .contract_violations
                .into_iter()
                .map(|v| ContractViolation {
                    model: v.model,
                    kind: v.kind,
                    message: v.message,
                })
                .collect(),
            schemas: output
                .schemas
                .into_iter()
                .map(|s| ModelSchema {
                    name: s.name,
                    columns: s
                        .columns
                        .into_iter()
                        .map(|c| SchemaColumn {
                            name: c.name,
                            data_type: c.data_type,
                            nullable: c.nullable,
                        })
                        .collect(),
                })
                .collect(),
        })
        .map_err(|e| ToolError::Execution(e.to_string()))
}

pub fn execute_get_dbt_column_lineage(
    workspace_root: &Path,
    params: &Value,
) -> Result<DbtColumnLineageOutput, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;
    let svc = AirformService::new(project_dir(workspace_root, project_name)?);
    svc.get_column_lineage()
        .map(|output| DbtColumnLineageOutput {
            project: project_name.to_string(),
            edges: output
                .edges
                .into_iter()
                .map(|e| ColumnEdge {
                    source_node: e.source_node,
                    source_column: e.source_column,
                    target_node: e.target_node,
                    target_column: e.target_column,
                    dependency_type: e.dependency_type,
                })
                .collect(),
        })
        .map_err(|e| ToolError::Execution(e.to_string()))
}

pub fn execute_parse_dbt_project(
    workspace_root: &Path,
    params: &Value,
) -> Result<ParseDbtProjectOutput, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;
    let svc = AirformService::new(project_dir(workspace_root, project_name)?);
    svc.parse()
        .map(|o| ParseDbtProjectOutput {
            project: project_name.to_string(),
            models: o.models as u64,
            seeds: o.seeds as u64,
            snapshots: o.snapshots as u64,
            tests: o.tests as u64,
            sources: o.sources as u64,
            nodes: o.nodes as u64,
            edges: o.edges as u64,
            duration_ms: o.duration_ms,
        })
        .map_err(|e| ToolError::Execution(e.to_string()))
}

pub async fn execute_seed_dbt_project(
    workspace_root: &Path,
    params: &Value,
    secrets_manager: Option<&SecretsManager>,
) -> Result<SeedDbtProjectOutput, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;
    let svc = make_service(workspace_root, project_name, secrets_manager).await?;
    svc.seed()
        .await
        .map(|o| SeedDbtProjectOutput {
            project: project_name.to_string(),
            seeds_loaded: o.seeds_loaded as usize,
            results: o
                .results
                .into_iter()
                .map(|r| ModelRunResult {
                    unique_id: r.unique_id,
                    name: r.name,
                    status: r.status,
                    duration_ms: r.duration_ms,
                    rows_affected: r.rows_affected.map(|n| n as u64),
                    message: r.message,
                })
                .collect(),
        })
        .map_err(|e| ToolError::Execution(e.to_string()))
}

pub fn execute_debug_dbt_project(
    workspace_root: &Path,
    params: &Value,
) -> Result<DebugDbtProjectOutput, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;
    let svc = AirformService::new(project_dir(workspace_root, project_name)?);
    svc.debug_info()
        .map(|o| DebugDbtProjectOutput {
            project_name: o.project_name,
            version: o.version.unwrap_or_default(),
            profile: o.profile.unwrap_or_default(),
            has_profiles_yml: o.has_profiles_yml,
            model_paths: o.model_paths,
            seed_paths: o.seed_paths,
            model_count: o.model_count as u64,
            seed_count: o.seed_count as u64,
            source_count: o.source_count as u64,
            all_ok: o.all_ok,
            issues: o.issues,
        })
        .map_err(|e| ToolError::Execution(e.to_string()))
}

pub fn execute_clean_dbt_project(
    workspace_root: &Path,
    params: &Value,
) -> Result<CleanDbtProjectOutput, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;
    let svc = AirformService::new(project_dir(workspace_root, project_name)?);
    svc.clean()
        .map(|o| CleanDbtProjectOutput {
            project: project_name.to_string(),
            cleaned: o.cleaned,
        })
        .map_err(|e| ToolError::Execution(e.to_string()))
}

pub fn execute_docs_generate_dbt(
    workspace_root: &Path,
    params: &Value,
) -> Result<DocsGenerateDbtOutput, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;
    let svc = AirformService::new(project_dir(workspace_root, project_name)?);
    svc.docs_generate()
        .map(|o| DocsGenerateDbtOutput {
            project: project_name.to_string(),
            manifest_path: o.manifest_path,
            nodes: o.nodes as u64,
            sources: o.sources as u64,
        })
        .map_err(|e| ToolError::Execution(e.to_string()))
}

pub fn execute_format_dbt_sql(
    workspace_root: &Path,
    params: &Value,
) -> Result<FormatDbtSqlOutput, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;
    let check = params["check"].as_bool().unwrap_or(false);
    let svc = AirformService::new(project_dir(workspace_root, project_name)?);
    svc.format_models(check)
        .map(|o| FormatDbtSqlOutput {
            project: project_name.to_string(),
            check_only: check,
            files_checked: o.files_checked as u64,
            files_changed: o.files_changed as u64,
            files: o.files,
        })
        .map_err(|e| ToolError::Execution(e.to_string()))
}

pub fn execute_init_dbt_project(
    workspace_root: &Path,
    params: &Value,
) -> Result<InitDbtProjectOutput, ToolError> {
    let name = params["name"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'name'".into()))?;
    validate_project_name(name)?;
    let target = workspace_root.join("modeling").join(name);
    let svc = AirformService::new(target);
    svc.init(name)
        .map(|o| InitDbtProjectOutput {
            project_name: o.project_name,
            project_dir: o.project_dir,
            files: o.files,
        })
        .map_err(|e| ToolError::Execution(e.to_string()))
}
