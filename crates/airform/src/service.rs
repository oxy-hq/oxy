use std::path::PathBuf;

use airform_analyzer::Analyzer;
use airform_compiler::Compiler;
use airform_core::ManifestNode;
use airform_executor::Executor;
use airform_graph::{NodeSelector, build_column_lineage, build_graph, selector::parse_selection};
use airform_jinja::{DbtContext, JinjaEngine};
use airform_loader::LoadState;
use airform_parser;

use oxy::adapters::secrets::SecretsManager;
use oxy::config::ConfigManager;
use oxy::config::model::DatabaseType;

use crate::config::OxyProjectConfig;
use crate::error::AirformIntegrationError;
use crate::types::*;

/// Scan `<root>/modeling/` for directories that contain a `dbt_project.yml`.
pub fn list_projects(root: &std::path::Path) -> Vec<DbtProjectInfo> {
    let models_dir = root.join("modeling");
    let Ok(entries) = std::fs::read_dir(&models_dir) else {
        return vec![];
    };
    let mut projects = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && path.join("dbt_project.yml").exists() {
            let folder_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            let svc = AirformService::new(path.clone());
            if let Ok(mut info) = svc.get_project_info() {
                info.folder_name = folder_name;
                projects.push(info);
            }
        }
    }
    projects.sort_by(|a, b| a.name.cmp(&b.name));
    projects
}

/// Oxy connector context for bridging sources and outputs.
#[derive(Clone)]
struct OxyContext {
    config_manager: ConfigManager,
    secrets_manager: SecretsManager,
}

/// Stateless service that orchestrates airform operations for a given project directory.
pub struct AirformService {
    project_dir: PathBuf,
    oxy: Option<OxyContext>,
    /// Config loaded from `oxy.yml` in the project directory.
    oxy_config: OxyProjectConfig,
}

impl AirformService {
    pub fn new(project_dir: PathBuf) -> Self {
        let oxy_config = OxyProjectConfig::load(&project_dir);
        Self {
            project_dir,
            oxy: None,
            oxy_config,
        }
    }

    /// Attach Oxy config and secrets so the service can load sources from Oxy
    /// databases and register outputs back into the config manager.
    pub fn with_oxy_context(
        mut self,
        config_manager: ConfigManager,
        secrets_manager: SecretsManager,
    ) -> Self {
        self.oxy = Some(OxyContext {
            config_manager,
            secrets_manager,
        });
        self
    }

    pub fn has_dbt_project(&self) -> bool {
        self.project_dir.join("dbt_project.yml").exists()
    }

    pub fn get_project_info(&self) -> Result<DbtProjectInfo, AirformIntegrationError> {
        if !self.has_dbt_project() {
            return Err(AirformIntegrationError::NoDbtProject(
                self.project_dir.display().to_string(),
            ));
        }
        let project = airform_loader::load_project(&self.project_dir)?;
        let folder_name = self
            .project_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        Ok(DbtProjectInfo {
            name: project.name.clone(),
            folder_name,
            project_dir: self.project_dir.display().to_string(),
            model_paths: project.model_paths.clone(),
            seed_paths: project.seed_paths.clone(),
        })
    }

    fn load(&self) -> Result<LoadState, AirformIntegrationError> {
        if !self.has_dbt_project() {
            return Err(AirformIntegrationError::NoDbtProject(
                self.project_dir.display().to_string(),
            ));
        }
        Ok(airform_loader::load(&self.project_dir)?)
    }

    pub fn list_nodes(&self) -> Result<Vec<NodeSummary>, AirformIntegrationError> {
        let load_state = self.load()?;
        let engine = JinjaEngine::new();
        let manifest = airform_parser::parse(&load_state, &engine)?;

        let mut nodes = Vec::new();

        for (id, node) in &manifest.nodes {
            let summary = match node {
                ManifestNode::Model(m) => NodeSummary {
                    unique_id: id.clone(),
                    name: m.name.clone(),
                    resource_type: node.resource_type().to_string(),
                    path: m.path.display().to_string(),
                    materialization: Some(m.config.materialized.to_string()),
                    description: m.description.clone(),
                    depends_on: ref_ids(&m.depends_on),
                    tags: m.tags.clone(),
                    raw_sql: Some(m.raw_sql.trim().to_string()),
                    compiled_sql: m.compiled_sql.as_deref().map(str::trim).map(String::from),
                    columns: col_defs(&m.columns),
                    database: None,
                    schema: None,
                },
                ManifestNode::Seed(s) => NodeSummary {
                    unique_id: id.clone(),
                    name: s.name.clone(),
                    resource_type: node.resource_type().to_string(),
                    path: s.path.display().to_string(),
                    materialization: None,
                    description: s.description.clone(),
                    depends_on: Vec::new(),
                    tags: s.config.tags.clone(),
                    raw_sql: None,
                    compiled_sql: None,
                    columns: col_defs(&s.columns),
                    database: None,
                    schema: None,
                },
                ManifestNode::Test(t) => NodeSummary {
                    unique_id: id.clone(),
                    name: t.name.clone(),
                    resource_type: node.resource_type().to_string(),
                    path: String::new(),
                    materialization: None,
                    description: None,
                    depends_on: ref_ids(&t.depends_on),
                    tags: t.config.tags.clone(),
                    raw_sql: Some(t.raw_sql.clone()),
                    compiled_sql: t.compiled_sql.clone(),
                    columns: Vec::new(),
                    database: None,
                    schema: None,
                },
                ManifestNode::Snapshot(s) => NodeSummary {
                    unique_id: id.clone(),
                    name: s.name.clone(),
                    resource_type: node.resource_type().to_string(),
                    path: s.path.display().to_string(),
                    materialization: Some(s.config.materialized.to_string()),
                    description: None,
                    depends_on: ref_ids(&s.depends_on),
                    tags: s.config.tags.clone(),
                    raw_sql: Some(s.raw_sql.clone()),
                    compiled_sql: s.compiled_sql.clone(),
                    columns: Vec::new(),
                    database: None,
                    schema: None,
                },
                ManifestNode::Source(_) => continue,
            };
            nodes.push(summary);
        }

        for (id, source) in &manifest.sources {
            nodes.push(NodeSummary {
                unique_id: id.clone(),
                name: source.name.clone(),
                resource_type: "source".to_string(),
                path: String::new(),
                materialization: None,
                description: source.description.clone(),
                depends_on: Vec::new(),
                tags: source.tags.clone(),
                raw_sql: None,
                compiled_sql: None,
                columns: col_defs(&source.columns),
                database: source.database.clone(),
                schema: source.schema.clone(),
            });
        }

        Ok(nodes)
    }

    pub fn compile_project(&self) -> Result<CompileOutput, AirformIntegrationError> {
        let load_state = self.load()?;
        let engine = JinjaEngine::new();
        let mut manifest = airform_parser::parse(&load_state, &engine)?;
        let graph = build_graph(&manifest)?;

        let ctx = self.build_dbt_context(&load_state);
        let compiler = Compiler::new(JinjaEngine::new());
        let result = compiler.compile(&mut manifest, &graph, &ctx)?;

        let mut compiled_nodes = Vec::new();
        for (_id, node) in &manifest.nodes {
            if let ManifestNode::Model(m) = node {
                if let Some(sql) = &m.compiled_sql {
                    compiled_nodes.push(CompiledNodeInfo {
                        unique_id: m.unique_id.clone(),
                        name: m.name.clone(),
                        compiled_sql: sql.trim().to_string(),
                    });
                }
            }
        }

        Ok(CompileOutput {
            models_compiled: result.compiled_count,
            errors: result
                .errors
                .into_iter()
                .map(|e| CompileErrorEntry {
                    node_id: e.node_id,
                    message: e.message,
                })
                .collect(),
            nodes: compiled_nodes,
        })
    }

    pub fn compile_model(&self, model_name: &str) -> Result<String, AirformIntegrationError> {
        let output = self.compile_project()?;
        output
            .nodes
            .into_iter()
            .find(|n| n.name == model_name)
            .map(|n| n.compiled_sql)
            .ok_or_else(|| {
                AirformIntegrationError::Airform(airform_core::AirformError::ModelNotFound(
                    model_name.to_string(),
                ))
            })
    }

    pub async fn run(&self, selector: Option<&str>) -> Result<RunOutput, AirformIntegrationError> {
        if self.oxy.is_some() && !OxyProjectConfig::exists(&self.project_dir) {
            return Err(AirformIntegrationError::MissingOxyConfig(
                self.project_dir.display().to_string(),
            ));
        }
        let load_state = self.load()?;
        if self.oxy.is_some() {
            self.validate_db_mappings(&load_state)?;
        }
        let engine = JinjaEngine::new();
        let mut manifest = airform_parser::parse(&load_state, &engine)?;
        let graph = build_graph(&manifest)?;

        let ctx = self.build_dbt_context(&load_state);
        let compiler = Compiler::new(JinjaEngine::new());
        compiler.compile(&mut manifest, &graph, &ctx)?;

        let executor = self.build_executor(&load_state).await?;
        executor.load_seeds(&manifest).await?;

        let selected = selector.map(|s| {
            let criteria = parse_selection(s);
            NodeSelector::new(&manifest, &graph).select(&criteria)
        });

        let result = executor
            .execute(&manifest, &graph, selected.as_deref())
            .await?;

        let duration_ms = result.total_duration().as_millis() as u64;
        let status = if result.error_count() > 0 {
            "error"
        } else {
            "success"
        };

        Ok(RunOutput {
            status: status.to_string(),
            results: result
                .results
                .iter()
                .map(|r| NodeRunResult {
                    unique_id: r.unique_id.clone(),
                    name: r.name.clone(),
                    status: r.status.to_string(),
                    duration_ms: r.duration.as_millis() as u64,
                    rows_affected: r.rows_affected,
                    message: r.message.clone(),
                })
                .collect(),
            duration_ms,
        })
    }

    pub async fn run_streaming(
        &self,
        selector: Option<&str>,
        tx: tokio::sync::mpsc::Sender<RunStreamEvent>,
    ) -> Result<(), AirformIntegrationError> {
        if self.oxy.is_some() && !OxyProjectConfig::exists(&self.project_dir) {
            let _ = tx
                .send(RunStreamEvent::Error {
                    message: format!("Missing oxy config at {}", self.project_dir.display()),
                })
                .await;
            return Err(AirformIntegrationError::MissingOxyConfig(
                self.project_dir.display().to_string(),
            ));
        }
        let load_state = match self.load() {
            Ok(s) => s,
            Err(e) => {
                let _ = tx
                    .send(RunStreamEvent::Error {
                        message: e.to_string(),
                    })
                    .await;
                return Err(e);
            }
        };
        if self.oxy.is_some() {
            if let Err(e) = self.validate_db_mappings(&load_state) {
                let _ = tx
                    .send(RunStreamEvent::Error {
                        message: e.to_string(),
                    })
                    .await;
                return Err(e);
            }
        }
        let engine = JinjaEngine::new();
        let mut manifest = match airform_parser::parse(&load_state, &engine) {
            Ok(m) => m,
            Err(e) => {
                let _ = tx
                    .send(RunStreamEvent::Error {
                        message: e.to_string(),
                    })
                    .await;
                return Err(e.into());
            }
        };
        let graph = match build_graph(&manifest) {
            Ok(g) => g,
            Err(e) => {
                let _ = tx
                    .send(RunStreamEvent::Error {
                        message: e.to_string(),
                    })
                    .await;
                return Err(e.into());
            }
        };

        let ctx = self.build_dbt_context(&load_state);
        let compiler = Compiler::new(JinjaEngine::new());
        if let Err(e) = compiler.compile(&mut manifest, &graph, &ctx) {
            let _ = tx
                .send(RunStreamEvent::Error {
                    message: e.to_string(),
                })
                .await;
            return Err(e.into());
        }

        let executor = match self.build_executor(&load_state).await {
            Ok(e) => e,
            Err(e) => {
                let _ = tx
                    .send(RunStreamEvent::Error {
                        message: e.to_string(),
                    })
                    .await;
                return Err(e);
            }
        };
        if let Err(e) = executor.load_seeds(&manifest).await {
            let _ = tx
                .send(RunStreamEvent::Error {
                    message: e.to_string(),
                })
                .await;
            return Err(AirformIntegrationError::from(e));
        }

        let selected = selector.map(|s| {
            let criteria = parse_selection(s);
            NodeSelector::new(&manifest, &graph).select(&criteria)
        });

        let order = match graph.topological_sort() {
            Ok(o) => o,
            Err(e) => {
                let _ = tx
                    .send(RunStreamEvent::Error {
                        message: e.to_string(),
                    })
                    .await;
                return Err(AirformIntegrationError::from(e));
            }
        };

        let mut node_results: Vec<airform_executor::NodeResult> = Vec::new();

        for unique_id in &order {
            if manifest.sources.contains_key(unique_id) {
                continue;
            }
            if let Some(ref sel) = selected {
                if !sel.contains(unique_id) {
                    continue;
                }
            }
            let Some(node) = manifest.nodes.get(unique_id) else {
                continue;
            };

            // Only models are executed by dbt run; skip seeds, tests, snapshots, etc.
            if !matches!(node, ManifestNode::Model(_)) {
                continue;
            }

            let _ = tx
                .send(RunStreamEvent::NodeStarted {
                    unique_id: unique_id.clone(),
                    name: node.name().to_string(),
                })
                .await;

            let result = match executor
                .execute(&manifest, &graph, Some(std::slice::from_ref(unique_id)))
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx
                        .send(RunStreamEvent::Error {
                            message: e.to_string(),
                        })
                        .await;
                    return Err(AirformIntegrationError::from(e));
                }
            };

            for r in result.results {
                let _ = tx
                    .send(RunStreamEvent::NodeCompleted(NodeRunResult {
                        unique_id: r.unique_id.clone(),
                        name: r.name.clone(),
                        status: r.status.to_string(),
                        duration_ms: r.duration.as_millis() as u64,
                        rows_affected: r.rows_affected,
                        message: r.message.clone(),
                    }))
                    .await;
                node_results.push(r);
            }
        }

        let total_duration: std::time::Duration = node_results.iter().map(|r| r.duration).sum();
        let error_count = node_results
            .iter()
            .filter(|r| r.status == airform_executor::NodeStatus::Error)
            .count();
        let duration_ms = total_duration.as_millis() as u64;
        let status = if error_count > 0 { "error" } else { "success" };
        let _ = tx
            .send(RunStreamEvent::Done {
                status: status.to_string(),
                duration_ms,
            })
            .await;

        Ok(())
    }

    pub async fn test(
        &self,
        selector: Option<&str>,
    ) -> Result<TestOutput, AirformIntegrationError> {
        if self.oxy.is_some() && !OxyProjectConfig::exists(&self.project_dir) {
            return Err(AirformIntegrationError::MissingOxyConfig(
                self.project_dir.display().to_string(),
            ));
        }
        let load_state = self.load()?;
        if self.oxy.is_some() {
            self.validate_db_mappings(&load_state)?;
        }
        let engine = JinjaEngine::new();
        let mut manifest = airform_parser::parse(&load_state, &engine)?;
        let graph = build_graph(&manifest)?;

        let ctx = self.build_dbt_context(&load_state);
        let compiler = Compiler::new(JinjaEngine::new());
        compiler.compile(&mut manifest, &graph, &ctx)?;

        let executor = self.build_executor(&load_state).await?;
        executor.load_seeds(&manifest).await?;

        let selected = selector.map(|s| {
            let criteria = parse_selection(s);
            NodeSelector::new(&manifest, &graph).select(&criteria)
        });
        executor
            .execute(&manifest, &graph, selected.as_deref())
            .await?;

        let results = executor.execute_tests(&manifest).await?;

        let passed = results
            .iter()
            .filter(|r| r.status == airform_executor::TestStatus::Pass)
            .count();
        let failed = results
            .iter()
            .filter(|r| r.status == airform_executor::TestStatus::Fail)
            .count();

        Ok(TestOutput {
            tests_run: results.len(),
            passed,
            failed,
            results: results
                .iter()
                .map(|r| TestResultEntry {
                    test_name: r.test_name.clone(),
                    model_name: r.model_name.clone(),
                    column_name: r.column_name.clone(),
                    status: r.status.to_string(),
                    failures: r.failures,
                    duration_ms: r.duration.as_millis() as u64,
                    message: r.message.clone(),
                })
                .collect(),
        })
    }

    pub async fn analyze(&self) -> Result<AnalyzeOutput, AirformIntegrationError> {
        let load_state = self.load()?;
        let engine = JinjaEngine::new();
        let mut manifest = airform_parser::parse(&load_state, &engine)?;
        let graph = build_graph(&manifest)?;

        let ctx = self.build_dbt_context(&load_state);
        let compiler = Compiler::new(JinjaEngine::new());
        compiler.compile(&mut manifest, &graph, &ctx)?;

        let result =
            Analyzer::analyze(&manifest, &graph, Some(&self.project_dir), None, None).await?;

        let schemas = result
            .schemas
            .iter()
            .map(|(name, schema)| SchemaEntry {
                name: name.clone(),
                columns: schema
                    .fields()
                    .iter()
                    .map(|f| ColumnInfo {
                        name: f.name().clone(),
                        data_type: format!("{}", f.data_type()),
                        nullable: f.is_nullable(),
                    })
                    .collect(),
            })
            .collect();

        Ok(AnalyzeOutput {
            models_analyzed: result.schemas.len(),
            cached_count: result.cached_count,
            diagnostics: result
                .diagnostics
                .iter()
                .map(|d| {
                    let (kind, message) = match d {
                        airform_analyzer::AnalyzerDiagnostic::SqlError { model, message } => (
                            "sql_error".to_string(),
                            format!("Model '{}': {}", model, message),
                        ),
                        airform_analyzer::AnalyzerDiagnostic::SchemaUnavailable { node } => (
                            "schema_unavailable".to_string(),
                            format!("Schema unavailable for '{}'", node),
                        ),
                    };
                    DiagnosticEntry { kind, message }
                })
                .collect(),
            contract_violations: result
                .contract_violations
                .iter()
                .map(|v| ContractViolationEntry {
                    model: v.model.clone(),
                    kind: format!("{:?}", v.kind),
                    message: v.to_string(),
                })
                .collect(),
            schemas,
        })
    }

    pub fn get_lineage(&self) -> Result<LineageOutput, AirformIntegrationError> {
        let load_state = self.load()?;
        let engine = JinjaEngine::new();
        let manifest = airform_parser::parse(&load_state, &engine)?;
        let graph = build_graph(&manifest)?;

        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        for (id, node) in &manifest.nodes {
            let (description, path) = match node {
                ManifestNode::Model(m) => {
                    (m.description.clone(), Some(m.path.display().to_string()))
                }
                ManifestNode::Seed(s) => {
                    (s.description.clone(), Some(s.path.display().to_string()))
                }
                ManifestNode::Snapshot(s) => (None, Some(s.path.display().to_string())),
                _ => (None, None),
            };
            nodes.push(LineageNode {
                unique_id: id.clone(),
                name: node.name().to_string(),
                resource_type: node.resource_type().to_string(),
                description,
                path,
            });
        }

        for (id, source) in &manifest.sources {
            nodes.push(LineageNode {
                unique_id: id.clone(),
                name: source.name.clone(),
                resource_type: "source".to_string(),
                description: source.description.clone(),
                path: None,
            });
        }

        let mut edge_set = std::collections::HashSet::new();

        for (id, _) in &manifest.nodes {
            for parent_id in graph.parents(id) {
                if edge_set.insert((parent_id.clone(), id.clone())) {
                    edges.push(LineageEdge {
                        source: parent_id,
                        target: id.clone(),
                    });
                }
            }
        }

        // build_graph matches source IDs using ends_with("source.{source}.{table}"),
        // which fails when the dbt project name equals the source name because the
        // actual unique_id is "source.{project}.{source}.{table}" and the suffix
        // "source.{source}.{table}" does not align correctly. Compute source edges
        // directly from depends_on using the correct ".{source}.{table}" suffix.
        for (id, node) in &manifest.nodes {
            let deps = match node {
                ManifestNode::Model(m) => Some(&m.depends_on),
                ManifestNode::Test(t) => Some(&t.depends_on),
                ManifestNode::Snapshot(s) => Some(&s.depends_on),
                _ => None,
            };
            if let Some(d) = deps {
                for source_call in &d.sources {
                    let suffix = format!(".{}.{}", source_call.source_name, source_call.table_name);
                    for (source_id, _) in &manifest.sources {
                        if source_id.ends_with(&suffix)
                            && edge_set.insert((source_id.clone(), id.clone()))
                        {
                            edges.push(LineageEdge {
                                source: source_id.clone(),
                                target: id.clone(),
                            });
                        }
                    }
                }
            }
        }

        Ok(LineageOutput { nodes, edges })
    }

    pub fn get_column_lineage(&self) -> Result<ColumnLineageOutput, AirformIntegrationError> {
        let load_state = self.load()?;
        let engine = JinjaEngine::new();
        let mut manifest = airform_parser::parse(&load_state, &engine)?;
        let graph = build_graph(&manifest)?;

        let ctx = self.build_dbt_context(&load_state);
        let compiler = Compiler::new(JinjaEngine::new());
        compiler.compile(&mut manifest, &graph, &ctx)?;

        let lineage = build_column_lineage(&manifest);

        Ok(ColumnLineageOutput {
            edges: lineage
                .edges
                .iter()
                .map(|e| ColumnLineageEntry {
                    source_node: e.source_node.clone(),
                    source_column: e.source_column.clone(),
                    target_node: e.target_node.clone(),
                    target_column: e.target_column.clone(),
                    dependency_type: e.dependency_type.to_string(),
                })
                .collect(),
        })
    }

    /// Parse the project manifest and validate the dependency graph.
    pub fn parse(&self) -> Result<ParseOutput, AirformIntegrationError> {
        use std::time::Instant;
        let start = Instant::now();
        let load_state = self.load()?;
        let engine = JinjaEngine::new();
        let manifest = airform_parser::parse(&load_state, &engine)?;
        let graph = build_graph(&manifest)?;

        let models = manifest.models().count();
        let seeds = manifest
            .nodes
            .values()
            .filter(|n| matches!(n, ManifestNode::Seed(_)))
            .count();
        let snapshots = manifest
            .nodes
            .values()
            .filter(|n| matches!(n, ManifestNode::Snapshot(_)))
            .count();
        let tests = manifest
            .nodes
            .values()
            .filter(|n| matches!(n, ManifestNode::Test(_)))
            .count();
        let sources = manifest.sources.len();
        let nodes = graph.node_count();
        let edges = graph.edge_count();
        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(ParseOutput {
            models,
            seeds,
            snapshots,
            tests,
            sources,
            nodes,
            edges,
            duration_ms,
        })
    }

    /// Load seed CSV files into the local DataFusion session context.
    pub async fn seed(&self) -> Result<SeedOutput, AirformIntegrationError> {
        if self.oxy.is_some() && !OxyProjectConfig::exists(&self.project_dir) {
            return Err(AirformIntegrationError::MissingOxyConfig(
                self.project_dir.display().to_string(),
            ));
        }
        let load_state = self.load()?;
        let engine = JinjaEngine::new();
        let manifest = airform_parser::parse(&load_state, &engine)?;
        let executor = self.build_executor(&load_state).await?;
        let seed_results = executor
            .load_seeds(&manifest)
            .await
            .map_err(|e| AirformIntegrationError::Other(e.to_string()))?;

        let seeds_loaded = seed_results
            .iter()
            .filter(|r| r.status == airform_executor::NodeStatus::Success)
            .count();

        let results = seed_results
            .iter()
            .map(|r| NodeRunResult {
                unique_id: r.unique_id.clone(),
                name: r.name.clone(),
                status: r.status.to_string(),
                duration_ms: r.duration.as_millis() as u64,
                rows_affected: r.rows_affected,
                message: r.message.clone(),
            })
            .collect();

        Ok(SeedOutput {
            seeds_loaded,
            results,
        })
    }

    /// Run a health-check on the project: validates config, profiles, and compilation.
    pub fn debug_info(&self) -> Result<DebugOutput, AirformIntegrationError> {
        let mut issues = Vec::new();

        let project = airform_loader::load_project(&self.project_dir)
            .map_err(|e| AirformIntegrationError::Other(e.to_string()))?;

        // Check profiles.yml
        let local_profiles = self.project_dir.join("profiles.yml");
        let home_profiles = dirs::home_dir()
            .map(|h| h.join(".dbt").join("profiles.yml"))
            .unwrap_or_default();
        let has_profiles_yml = local_profiles.exists() || home_profiles.exists();
        if !has_profiles_yml {
            issues.push("profiles.yml not found (checked project dir and ~/.dbt/)".to_string());
        }

        // Try to parse and compile
        let (model_count, seed_count, source_count) = match self.load() {
            Ok(load_state) => {
                let engine = JinjaEngine::new();
                match airform_parser::parse(&load_state, &engine) {
                    Ok(manifest) => {
                        let models = manifest.models().count();
                        let seeds = manifest
                            .nodes
                            .values()
                            .filter(|n| matches!(n, ManifestNode::Seed(_)))
                            .count();
                        let sources = manifest.sources.len();
                        (models, seeds, sources)
                    }
                    Err(e) => {
                        issues.push(format!("Parse error: {e}"));
                        (0, 0, 0)
                    }
                }
            }
            Err(e) => {
                issues.push(format!("Load error: {e}"));
                (0, 0, 0)
            }
        };

        Ok(DebugOutput {
            project_name: project.name.clone(),
            version: project.version.clone(),
            profile: project.profile.clone(),
            has_profiles_yml,
            model_paths: project.model_paths.clone(),
            seed_paths: project.seed_paths.clone(),
            model_count,
            seed_count,
            source_count,
            all_ok: issues.is_empty(),
            issues,
        })
    }

    /// Remove directories listed in `clean-targets` (e.g. `target/`, `dbt_packages/`).
    pub fn clean(&self) -> Result<CleanOutput, AirformIntegrationError> {
        let load_state = self.load()?;
        let mut cleaned = Vec::new();

        for target in &load_state.project.clean_targets {
            let path = self.project_dir.join(target);
            if path.exists() {
                std::fs::remove_dir_all(&path)
                    .map_err(|e| AirformIntegrationError::Other(e.to_string()))?;
                cleaned.push(path.display().to_string());
            }
        }

        Ok(CleanOutput { cleaned })
    }

    /// Generate project documentation by writing `manifest.json` into the target directory.
    pub fn docs_generate(&self) -> Result<DocsOutput, AirformIntegrationError> {
        let load_state = self.load()?;
        let engine = JinjaEngine::new();
        let manifest = airform_parser::parse(&load_state, &engine)?;

        let target_dir = self.project_dir.join(&load_state.project.target_path);
        std::fs::create_dir_all(&target_dir)
            .map_err(|e| AirformIntegrationError::Other(e.to_string()))?;

        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| AirformIntegrationError::Other(e.to_string()))?;
        let manifest_path = target_dir.join("manifest.json");
        std::fs::write(&manifest_path, manifest_json)
            .map_err(|e| AirformIntegrationError::Other(e.to_string()))?;

        Ok(DocsOutput {
            manifest_path: manifest_path.display().to_string(),
            nodes: manifest.nodes.len(),
            sources: manifest.sources.len(),
        })
    }

    /// Uppercase SQL keywords in all model `.sql` files.
    ///
    /// When `check` is `true`, files are not modified — the return value
    /// lists which files *would* change (useful for CI).
    pub fn format_models(&self, check: bool) -> Result<FormatOutput, AirformIntegrationError> {
        let load_state = self.load()?;
        let mut files_checked = 0;
        let mut files_changed_count = 0;
        let mut files = Vec::new();

        for model_file in &load_state.model_files {
            let original = std::fs::read_to_string(&model_file.path)
                .map_err(|e| AirformIntegrationError::Other(e.to_string()))?;
            let formatted = format_sql_keywords(&original);
            files_checked += 1;

            if formatted != original {
                files_changed_count += 1;
                files.push(model_file.path.display().to_string());
                if !check {
                    std::fs::write(&model_file.path, &formatted)
                        .map_err(|e| AirformIntegrationError::Other(e.to_string()))?;
                }
            }
        }

        Ok(FormatOutput {
            files_checked,
            files_changed: files_changed_count,
            files,
        })
    }

    /// Create a new dbt project scaffold at `self.project_dir`.
    ///
    /// Fails if the directory already exists.
    pub fn init(&self, name: &str) -> Result<InitOutput, AirformIntegrationError> {
        let target = &self.project_dir;
        if target.exists() {
            return Err(AirformIntegrationError::Other(format!(
                "Directory '{}' already exists",
                target.display()
            )));
        }

        let mk = |sub: &str| -> Result<(), AirformIntegrationError> {
            std::fs::create_dir_all(target.join(sub))
                .map_err(|e| AirformIntegrationError::Other(e.to_string()))
        };
        mk("models/staging")?;
        mk("models/marts")?;
        mk("seeds")?;
        mk("tests")?;
        mk("macros")?;
        mk("snapshots")?;
        mk("analyses")?;

        let project_subdir = format!("modeling/{name}");
        let scaffold: Vec<(String, String, String)> = vec![
            (
                format!("{project_subdir}/dbt_project.yml"),
                format!(
                    "name: '{name}'\nversion: '1.0.0'\nconfig-version: 2\n\nprofile: '{name}'\n\nmodel-paths: [\"models\"]\nseed-paths: [\"seeds\"]\ntest-paths: [\"tests\"]\nmacro-paths: [\"macros\"]\nsnapshot-paths: [\"snapshots\"]\nanalysis-paths: [\"analyses\"]\n\ntarget-path: \"target\"\nclean-targets:\n  - \"target\"\n  - \"dbt_packages\"\n\nmodels:\n  {name}:\n    staging:\n      +materialized: view\n    marts:\n      +materialized: table\n"
                ),
                "dbt project configuration".into(),
            ),
            (
                format!("{project_subdir}/profiles.yml"),
                format!(
                    "{name}:\n  target: dev\n  outputs:\n    dev:\n      type: duckdb\n      path: ':memory:'\n      threads: 4\n"
                ),
                "dbt connection profiles (DuckDB)".into(),
            ),
            (
                format!("{project_subdir}/README.md"),
                format!(
                    "# {name}\n\nGenerated by airform. Edit `dbt_project.yml` to configure your project.\n"
                ),
                "project README".into(),
            ),
        ];

        std::fs::write(target.join("dbt_project.yml"), &scaffold[0].1)
            .map_err(|e| AirformIntegrationError::Other(e.to_string()))?;
        std::fs::write(target.join("profiles.yml"), &scaffold[1].1)
            .map_err(|e| AirformIntegrationError::Other(e.to_string()))?;
        std::fs::write(target.join("README.md"), &scaffold[2].1)
            .map_err(|e| AirformIntegrationError::Other(e.to_string()))?;

        Ok(InitOutput {
            project_name: name.to_string(),
            project_dir: target.display().to_string(),
            files: scaffold,
        })
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

impl AirformService {
    /// Build an `Executor` by resolving the Oxy database via `oxy.yml` and constructing
    /// the appropriate warehouse adapter (Snowflake / BigQuery).
    async fn build_executor(
        &self,
        load_state: &LoadState,
    ) -> Result<Executor, AirformIntegrationError> {
        let target_schema = load_state
            .target
            .as_ref()
            .and_then(|t| {
                t.schema
                    .as_deref()
                    .or_else(|| t.extra.get("dataset").and_then(|v| v.as_str()))
                    .or_else(|| t.extra.get("schema").and_then(|v| v.as_str()))
            })
            .unwrap_or("main");

        let Some(oxy_ctx) = self.oxy.as_ref() else {
            return Ok(Executor::new(target_schema));
        };

        let adapter = crate::adapter::build_adapter(
            load_state,
            &self.oxy_config,
            &oxy_ctx.config_manager,
            &oxy_ctx.secrets_manager,
        )
        .await?;
        Ok(Executor::with_adapter(adapter, target_schema))
    }

    fn project_name(&self) -> &str {
        self.project_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
    }

    /// Check that every output defined in `profiles.yml` has an explicit entry in `oxy.yml`,
    /// and that each mapped dbt target type is compatible with the Oxy database type.
    fn validate_db_mappings(&self, load_state: &LoadState) -> Result<(), AirformIntegrationError> {
        let Some(profile) = &load_state.profile else {
            return Ok(());
        };
        let unmapped = self
            .oxy_config
            .unmapped_outputs(profile.outputs.keys().map(String::as_str));
        if !unmapped.is_empty() {
            return Err(AirformIntegrationError::UnmappedDbtDatabases {
                unmapped: unmapped.join(", "),
                config_path: self.project_dir.join("oxy.yml").display().to_string(),
            });
        }

        if let Some(oxy_ctx) = self.oxy.as_ref() {
            let mut mismatches = Vec::new();
            for (output_name, dbt_target) in &profile.outputs {
                let Some(oxy_db_name) = self.oxy_config.resolve_profile_database(output_name)
                else {
                    continue;
                };
                let Ok(db) = oxy_ctx.config_manager.resolve_database(oxy_db_name) else {
                    continue;
                };
                if let oxy::config::model::DatabaseType::DuckDB(duckdb) = &db.database_type {
                    if matches!(
                        duckdb.options,
                        oxy::config::model::DuckDBOptions::Local { .. }
                    ) {
                        return Err(AirformIntegrationError::DuckDbLocalNotSupported {
                            db_name: oxy_db_name.to_string(),
                        });
                    }
                }
                let dbt_type = &dbt_target.adapter_type;
                if !dbt_type_compatible_with_oxy(dbt_type, &db.database_type) {
                    mismatches.push(format!(
                        "target '{}': dbt type '{}' is incompatible with Oxy database '{}' (type '{}')",
                        output_name,
                        dbt_type,
                        oxy_db_name,
                        db.database_type_name()
                    ));
                }
            }
            if !mismatches.is_empty() {
                return Err(AirformIntegrationError::DatabaseTypeMismatch {
                    mismatches: mismatches.join("; "),
                    config_path: self.project_dir.join("oxy.yml").display().to_string(),
                });
            }
        }

        Ok(())
    }

    /// Build a DbtContext from the loaded project and profile target.
    fn build_dbt_context(&self, load_state: &LoadState) -> DbtContext {
        let mut ctx = DbtContext::new(&load_state.project.name);

        if let Some(ref target) = load_state.target {
            let schema = target
                .schema
                .as_deref()
                .or_else(|| target.extra.get("dataset").and_then(|v| v.as_str()))
                .or_else(|| target.extra.get("schema").and_then(|v| v.as_str()));
            if let Some(schema) = schema {
                ctx.target_schema = schema.to_string();
            }
            if let Some(ref database) = target.database {
                ctx.target_database = database.clone();
            }
            ctx.target_type = target.adapter_type.clone();
        }

        for (key, value) in &load_state.project.vars {
            ctx.vars.insert(key.clone(), value.clone());
        }

        ctx
    }
}

/// Returns `true` when `dbt_type` (the `type:` field from `profiles.yml`) is compatible
/// with the given Oxy `DatabaseType`.
///
/// MotherDuck surfaces as `duckdb` in dbt, so both DuckDB and MotherDuck are accepted
/// for a `duckdb` dbt target.
fn dbt_type_compatible_with_oxy(dbt_type: &str, oxy_db_type: &DatabaseType) -> bool {
    match dbt_type {
        "snowflake" => matches!(oxy_db_type, DatabaseType::Snowflake(_)),
        "bigquery" => matches!(oxy_db_type, DatabaseType::Bigquery(_)),
        "duckdb" => matches!(
            oxy_db_type,
            DatabaseType::DuckDB(_) | DatabaseType::MotherDuck(_)
        ),
        "postgres" => matches!(oxy_db_type, DatabaseType::Postgres(_)),
        "redshift" => matches!(oxy_db_type, DatabaseType::Redshift(_)),
        "mysql" => matches!(oxy_db_type, DatabaseType::Mysql(_)),
        "clickhouse" => matches!(oxy_db_type, DatabaseType::ClickHouse(_)),
        _ => false,
    }
}

fn col_defs(columns: &[airform_core::ColumnDef]) -> Vec<NodeColumnDef> {
    columns
        .iter()
        .map(|c| NodeColumnDef {
            name: c.name.clone(),
            description: c.description.clone(),
            data_type: c.data_type.clone(),
        })
        .collect()
}

fn ref_ids(depends_on: &airform_core::DependsOn) -> Vec<String> {
    let mut ids: Vec<String> = depends_on
        .refs
        .iter()
        .map(|r| r.model_name.clone())
        .collect();
    ids.extend(
        depends_on
            .sources
            .iter()
            .map(|s| format!("{}.{}", s.source_name, s.table_name)),
    );
    ids
}

/// Uppercase SQL keywords in a SQL string (best-effort, word-boundary safe).
fn format_sql_keywords(sql: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "SELECT",
        "FROM",
        "WHERE",
        "JOIN",
        "LEFT",
        "RIGHT",
        "INNER",
        "OUTER",
        "FULL",
        "CROSS",
        "ON",
        "AND",
        "OR",
        "NOT",
        "IN",
        "EXISTS",
        "BETWEEN",
        "LIKE",
        "IS",
        "NULL",
        "AS",
        "GROUP",
        "BY",
        "ORDER",
        "HAVING",
        "LIMIT",
        "OFFSET",
        "UNION",
        "ALL",
        "DISTINCT",
        "INSERT",
        "INTO",
        "UPDATE",
        "DELETE",
        "CREATE",
        "DROP",
        "ALTER",
        "TABLE",
        "VIEW",
        "INDEX",
        "SET",
        "VALUES",
        "CASE",
        "WHEN",
        "THEN",
        "ELSE",
        "END",
        "WITH",
        "RECURSIVE",
        "OVER",
        "PARTITION",
        "ROWS",
        "RANGE",
        "UNBOUNDED",
        "PRECEDING",
        "FOLLOWING",
        "CURRENT",
        "ROW",
        "ASC",
        "DESC",
        "NULLS",
        "FIRST",
        "LAST",
        "TRUE",
        "FALSE",
        "CAST",
        "COALESCE",
        "IF",
        "EXCEPT",
        "INTERSECT",
        "LATERAL",
        "NATURAL",
        "USING",
        "WINDOW",
        "FILTER",
        "WITHIN",
        "FETCH",
        "NEXT",
        "ONLY",
        "FOR",
        "MATERIALIZED",
    ];

    let mut result = sql.to_string();
    for kw in KEYWORDS {
        // Replace whole-word occurrences (case-insensitive) with uppercase.
        // Operates at the char boundary level to preserve UTF-8 sequences.
        let lower = kw.to_lowercase();
        let kw_len = kw.len(); // byte length (all keywords are ASCII so char == byte here)
        let mut out = String::with_capacity(result.len());
        let mut remaining = result.as_str();
        while !remaining.is_empty() {
            if remaining.to_lowercase().starts_with(&lower) {
                let before_ok = out
                    .chars()
                    .next_back()
                    .map_or(true, |c| !c.is_alphanumeric() && c != '_');
                let after_ok = remaining[kw_len..]
                    .chars()
                    .next()
                    .map_or(true, |c| !c.is_alphanumeric() && c != '_');
                if before_ok && after_ok {
                    out.push_str(kw);
                    remaining = &remaining[kw_len..];
                    continue;
                }
            }
            // Advance by one char (not one byte) to preserve multibyte UTF-8 sequences.
            let c = remaining.chars().next().unwrap();
            out.push(c);
            remaining = &remaining[c.len_utf8()..];
        }
        result = out;
    }
    result
}
