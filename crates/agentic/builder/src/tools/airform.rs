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

use agentic_core::tools::{ToolDef, ToolError};
use oxy::adapters::secrets::SecretsManager;
use oxy_airform::service::{self, AirformService};
use serde_json::{Value, json};

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

/// Resolve `<workspace_root>/modeling/<project_name>`.
fn project_dir(workspace_root: &Path, project_name: &str) -> Result<PathBuf, ToolError> {
    validate_project_name(project_name)?;
    Ok(workspace_root.join("modeling").join(project_name))
}

/// Build an `AirformService` with optional Oxy context (config + secrets).
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

fn airform_err(e: impl std::fmt::Display) -> Value {
    json!({ "ok": false, "error": e.to_string() })
}

// ── Tool definitions ──────────────────────────────────────────────────────────

pub fn list_dbt_projects_def() -> ToolDef {
    ToolDef {
        name: "list_dbt_projects",
        description: "List all dbt/airform data transformation projects in this Oxy workspace. \
                      Projects live under `modeling/` and each contains a \
                      `dbt_project.yml`. Returns {ok, projects: [{name, project_dir, \
                      model_paths, seed_paths}]}.",
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
        description: "List all nodes (models, seeds, tests, sources) in a dbt project. \
                      Returns {ok, nodes: [{unique_id, name, resource_type, path, \
                      materialization, description, depends_on, tags, raw_sql, compiled_sql, \
                      columns}]}.",
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
                      Provide `model` to compile a single model; omit it to compile all. \
                      Returns {ok, compiled_sql} for a single model, or \
                      {ok, models_compiled, errors, nodes: [{name, compiled_sql}]} for all.",
        parameters: json!({
            "type": "object",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Name of the dbt project directory under modeling/"
                },
                "model": {
                    "type": "string",
                    "description": "Model name to compile. Empty string or omit to compile the whole project."
                }
            },
            "required": ["project"],
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
                      DatabaseTypeMismatch error. \
                      Returns {ok, status, results: [{name, status, duration_ms, rows_affected, \
                      message}], duration_ms}.",
        parameters: json!({
            "type": "object",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Name of the dbt project directory under modeling/"
                },
                "selector": {
                    "type": "string",
                    "description": "Optional model selector (matches against unique_id or model name). Omit or use empty string to run all models."
                }
            },
            "required": ["project"],
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
                      run_dbt_models). \
                      Returns {ok, tests_run, passed, failed, results: [{test_name, \
                      model_name, column_name, status, failures, duration_ms, message}]}.",
        parameters: json!({
            "type": "object",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Name of the dbt project directory under modeling/"
                },
                "selector": {
                    "type": ["string", "null"],
                    "description": "Optional selector to filter which models are run before testing. Null runs all."
                }
            },
            "required": ["project"],
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
                      directed edges. Useful for understanding dependencies before modifying \
                      a model. Returns {ok, nodes: [{unique_id, name, resource_type, \
                      description, path}], edges: [{source, target}]}.",
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

// ── Executors ─────────────────────────────────────────────────────────────────

pub fn execute_list_dbt_projects(
    workspace_root: &Path,
    _params: &Value,
) -> Result<Value, ToolError> {
    let projects = service::list_projects(workspace_root);
    let project_json: Vec<Value> = projects
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
    Ok(json!({
        "ok": true,
        "projects": project_json,
        "count": project_json.len(),
    }))
}

pub fn execute_list_dbt_nodes(workspace_root: &Path, params: &Value) -> Result<Value, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;

    let svc = AirformService::new(project_dir(workspace_root, project_name)?);
    match svc.list_nodes() {
        Ok(nodes) => {
            let nodes_json: Vec<Value> = nodes
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
            Ok(json!({
                "ok": true,
                "count": nodes_json.len(),
                "nodes": nodes_json,
            }))
        }
        Err(e) => Ok(airform_err(e)),
    }
}

pub fn execute_compile_dbt_model(
    workspace_root: &Path,
    params: &Value,
) -> Result<Value, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;

    let svc = AirformService::new(project_dir(workspace_root, project_name)?);

    if let Some(model_name) = params["model"].as_str().filter(|s| !s.is_empty()) {
        match svc.compile_model(model_name) {
            Ok(sql) => Ok(json!({
                "ok": true,
                "model": model_name,
                "compiled_sql": sql,
            })),
            Err(e) => Ok(airform_err(e)),
        }
    } else {
        match svc.compile_project() {
            Ok(output) => {
                let nodes: Vec<Value> = output
                    .nodes
                    .iter()
                    .map(|n| {
                        json!({
                            "name": n.name,
                            "unique_id": n.unique_id,
                            "compiled_sql": n.compiled_sql,
                        })
                    })
                    .collect();
                let errors: Vec<Value> = output
                    .errors
                    .iter()
                    .map(|e| json!({ "node_id": e.node_id, "message": e.message }))
                    .collect();
                Ok(json!({
                    "ok": true,
                    "models_compiled": output.models_compiled,
                    "errors": errors,
                    "nodes": nodes,
                }))
            }
            Err(e) => Ok(airform_err(e)),
        }
    }
}

pub async fn execute_run_dbt_models(
    workspace_root: &Path,
    params: &Value,
    secrets_manager: Option<&SecretsManager>,
) -> Result<Value, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;

    let selector = params["selector"].as_str().filter(|s| !s.is_empty());

    let svc = make_service(workspace_root, project_name, secrets_manager).await?;

    match svc.run(selector).await {
        Ok(output) => {
            let results: Vec<Value> = output
                .results
                .iter()
                .map(|r| {
                    json!({
                        "unique_id": r.unique_id,
                        "name": r.name,
                        "status": r.status,
                        "duration_ms": r.duration_ms,
                        "rows_affected": r.rows_affected,
                        "message": r.message,
                    })
                })
                .collect();
            Ok(json!({
                "ok": true,
                "status": output.status,
                "duration_ms": output.duration_ms,
                "results": results,
            }))
        }
        Err(e) => Ok(airform_err(e)),
    }
}

pub async fn execute_test_dbt_models(
    workspace_root: &Path,
    params: &Value,
    secrets_manager: Option<&SecretsManager>,
) -> Result<Value, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;

    let selector = params["selector"].as_str().filter(|s| !s.is_empty());

    let svc = make_service(workspace_root, project_name, secrets_manager).await?;

    match svc.test(selector).await {
        Ok(output) => {
            let results: Vec<Value> = output
                .results
                .iter()
                .map(|r| {
                    json!({
                        "test_name": r.test_name,
                        "model_name": r.model_name,
                        "column_name": r.column_name,
                        "status": r.status,
                        "failures": r.failures,
                        "duration_ms": r.duration_ms,
                        "message": r.message,
                    })
                })
                .collect();
            Ok(json!({
                "ok": true,
                "tests_run": output.tests_run,
                "passed": output.passed,
                "failed": output.failed,
                "results": results,
            }))
        }
        Err(e) => Ok(airform_err(e)),
    }
}

pub fn analyze_dbt_project_def() -> ToolDef {
    ToolDef {
        name: "analyze_dbt_project",
        description: "Analyze a dbt project by compiling all models and inferring their output \
                      schemas using DataFusion. Reports SQL errors, schema-unavailable warnings, \
                      and contract violations (columns declared in schema.yml that are missing \
                      or type-mismatched in the actual output). \
                      Returns {ok, models_analyzed, cached_count, \
                      diagnostics: [{kind, message}], \
                      contract_violations: [{model, kind, message}], \
                      schemas: [{name, columns: [{name, data_type, nullable}]}]}.",
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
                      describes how a column in one node derives from a column in another. \
                      Returns {ok, edges: [{source_node, source_column, target_node, \
                      target_column, dependency_type}]}.",
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

pub fn execute_get_dbt_lineage(workspace_root: &Path, params: &Value) -> Result<Value, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;

    let svc = AirformService::new(project_dir(workspace_root, project_name)?);

    match svc.get_lineage() {
        Ok(output) => {
            let nodes: Vec<Value> = output
                .nodes
                .iter()
                .map(|n| {
                    json!({
                        "unique_id": n.unique_id,
                        "name": n.name,
                        "resource_type": n.resource_type,
                        "description": n.description,
                        "path": n.path,
                    })
                })
                .collect();
            let edges: Vec<Value> = output
                .edges
                .iter()
                .map(|e| json!({ "source": e.source, "target": e.target }))
                .collect();
            Ok(json!({
                "ok": true,
                "nodes": nodes,
                "edges": edges,
            }))
        }
        Err(e) => Ok(airform_err(e)),
    }
}

pub async fn execute_analyze_dbt_project(
    workspace_root: &Path,
    params: &Value,
) -> Result<Value, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;

    let svc = AirformService::new(project_dir(workspace_root, project_name)?);

    match svc.analyze().await {
        Ok(output) => {
            let diagnostics: Vec<Value> = output
                .diagnostics
                .iter()
                .map(|d| json!({ "kind": d.kind, "message": d.message }))
                .collect();
            let contract_violations: Vec<Value> = output
                .contract_violations
                .iter()
                .map(|v| json!({ "model": v.model, "kind": v.kind, "message": v.message }))
                .collect();
            let schemas: Vec<Value> = output
                .schemas
                .iter()
                .map(|s| {
                    json!({
                        "name": s.name,
                        "columns": s.columns.iter().map(|c| json!({
                            "name": c.name,
                            "data_type": c.data_type,
                            "nullable": c.nullable,
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect();
            Ok(json!({
                "ok": true,
                "models_analyzed": output.models_analyzed,
                "cached_count": output.cached_count,
                "diagnostics": diagnostics,
                "contract_violations": contract_violations,
                "schemas": schemas,
            }))
        }
        Err(e) => Ok(airform_err(e)),
    }
}

pub fn execute_get_dbt_column_lineage(
    workspace_root: &Path,
    params: &Value,
) -> Result<Value, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;

    let svc = AirformService::new(project_dir(workspace_root, project_name)?);

    match svc.get_column_lineage() {
        Ok(output) => {
            let edges: Vec<Value> = output
                .edges
                .iter()
                .map(|e| {
                    json!({
                        "source_node": e.source_node,
                        "source_column": e.source_column,
                        "target_node": e.target_node,
                        "target_column": e.target_column,
                        "dependency_type": e.dependency_type,
                    })
                })
                .collect();
            Ok(json!({
                "ok": true,
                "edges": edges,
            }))
        }
        Err(e) => Ok(airform_err(e)),
    }
}

// ── New v2 tools ──────────────────────────────────────────────────────────────

pub fn parse_dbt_project_def() -> ToolDef {
    ToolDef {
        name: "parse_dbt_project",
        description: "Parse the dbt project manifest and validate the dependency DAG. \
                      Reports model, seed, snapshot, test, and source counts along with \
                      DAG node/edge counts and parse duration. \
                      Returns {ok, models, seeds, snapshots, tests, sources, \
                      nodes, edges, duration_ms}.",
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
                      execution context. Returns {ok, seeds_loaded, \
                      results: [{unique_id, name, status, duration_ms, rows_affected, message}]}.",
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
                      profiles.yml, and attempts a compilation pass. Reports any issues found. \
                      Returns {ok, project_name, version, profile, has_profiles_yml, \
                      model_paths, seed_paths, model_count, seed_count, source_count, \
                      all_ok, issues: [string]}.",
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
                      `dbt_packages/`) from the dbt project. \
                      Returns {ok, cleaned: [path]}.",
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
                      project's target directory. \
                      Returns {ok, manifest_path, nodes, sources}.",
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
                      (defaults to false — files are reformatted in place). \
                      Returns {ok, files_checked, files_changed, files: [path]}.",
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
            "required": ["project"],
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
                      Returns {ok, project_name, project_dir}.",
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

// ── New v2 executors ──────────────────────────────────────────────────────────

pub fn execute_parse_dbt_project(
    workspace_root: &Path,
    params: &Value,
) -> Result<Value, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;

    let svc = AirformService::new(project_dir(workspace_root, project_name)?);
    match svc.parse() {
        Ok(o) => Ok(json!({
            "ok": true,
            "models": o.models,
            "seeds": o.seeds,
            "snapshots": o.snapshots,
            "tests": o.tests,
            "sources": o.sources,
            "nodes": o.nodes,
            "edges": o.edges,
            "duration_ms": o.duration_ms,
        })),
        Err(e) => Ok(airform_err(e)),
    }
}

pub async fn execute_seed_dbt_project(
    workspace_root: &Path,
    params: &Value,
    secrets_manager: Option<&SecretsManager>,
) -> Result<Value, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;

    let svc = make_service(workspace_root, project_name, secrets_manager).await?;
    match svc.seed().await {
        Ok(o) => {
            let results: Vec<Value> = o
                .results
                .iter()
                .map(|r| {
                    json!({
                        "unique_id": r.unique_id,
                        "name": r.name,
                        "status": r.status,
                        "duration_ms": r.duration_ms,
                        "rows_affected": r.rows_affected,
                        "message": r.message,
                    })
                })
                .collect();
            Ok(json!({
                "ok": true,
                "seeds_loaded": o.seeds_loaded,
                "results": results,
            }))
        }
        Err(e) => Ok(airform_err(e)),
    }
}

pub fn execute_debug_dbt_project(
    workspace_root: &Path,
    params: &Value,
) -> Result<Value, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;

    let svc = AirformService::new(project_dir(workspace_root, project_name)?);
    match svc.debug_info() {
        Ok(o) => Ok(json!({
            "ok": true,
            "project_name": o.project_name,
            "version": o.version,
            "profile": o.profile,
            "has_profiles_yml": o.has_profiles_yml,
            "model_paths": o.model_paths,
            "seed_paths": o.seed_paths,
            "model_count": o.model_count,
            "seed_count": o.seed_count,
            "source_count": o.source_count,
            "all_ok": o.all_ok,
            "issues": o.issues,
        })),
        Err(e) => Ok(airform_err(e)),
    }
}

pub fn execute_clean_dbt_project(
    workspace_root: &Path,
    params: &Value,
) -> Result<Value, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;

    let svc = AirformService::new(project_dir(workspace_root, project_name)?);
    match svc.clean() {
        Ok(o) => Ok(json!({ "ok": true, "cleaned": o.cleaned })),
        Err(e) => Ok(airform_err(e)),
    }
}

pub fn execute_docs_generate_dbt(
    workspace_root: &Path,
    params: &Value,
) -> Result<Value, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;

    let svc = AirformService::new(project_dir(workspace_root, project_name)?);
    match svc.docs_generate() {
        Ok(o) => Ok(json!({
            "ok": true,
            "manifest_path": o.manifest_path,
            "nodes": o.nodes,
            "sources": o.sources,
        })),
        Err(e) => Ok(airform_err(e)),
    }
}

pub fn execute_format_dbt_sql(workspace_root: &Path, params: &Value) -> Result<Value, ToolError> {
    let project_name = params["project"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'project'".into()))?;
    let check = params["check"].as_bool().unwrap_or(false);

    let svc = AirformService::new(project_dir(workspace_root, project_name)?);
    match svc.format_models(check) {
        Ok(o) => Ok(json!({
            "ok": true,
            "files_checked": o.files_checked,
            "files_changed": o.files_changed,
            "files": o.files,
        })),
        Err(e) => Ok(airform_err(e)),
    }
}

pub fn execute_init_dbt_project(workspace_root: &Path, params: &Value) -> Result<Value, ToolError> {
    let name = params["name"]
        .as_str()
        .ok_or_else(|| ToolError::BadParams("missing 'name'".into()))?;
    validate_project_name(name)?;

    let target = workspace_root.join("modeling").join(name);
    let svc = AirformService::new(target);
    match svc.init(name) {
        Ok(o) => {
            let files: Vec<_> = o
                .files
                .into_iter()
                .map(|(path, content, desc)| json!([path, content, desc]))
                .collect();
            Ok(json!({
                "ok": true,
                "project_name": o.project_name,
                "project_dir": o.project_dir,
                "files": files,
            }))
        }
        Err(e) => Ok(airform_err(e)),
    }
}
