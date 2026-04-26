//! `BuilderProjectValidator` implementation backed by oxy's ConfigManager
//! and airlayer for semantic file validation.

use std::path::{Path, PathBuf};

use agentic_builder::validator::{BuilderProjectValidator, ValidatedFile, ValidationReport};
use agentic_core::tools::ToolError;
use async_trait::async_trait;
use oxy::adapters::workspace::manager::WorkspaceManager;

/// Bridges builder project validation to oxy's config validation (for agents,
/// workflows, apps) and airlayer parsing (for semantic views/topics).
pub struct OxyBuilderProjectValidator {
    workspace_manager: WorkspaceManager,
}

impl OxyBuilderProjectValidator {
    pub fn new(workspace_manager: WorkspaceManager) -> Self {
        Self { workspace_manager }
    }
}

#[async_trait]
impl BuilderProjectValidator for OxyBuilderProjectValidator {
    async fn validate_file(&self, abs_path: &Path) -> Result<(), String> {
        let config = oxy::config::ConfigBuilder::new()
            .with_workspace_path(self.workspace_manager.config_manager.workspace_path())
            .map_err(|e| format!("config error: {e}"))?
            .build()
            .await
            .map_err(|e| format!("config error: {e}"))?;

        let cfg = config.get_config();
        let file_name = abs_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        validate_single_file(abs_path, file_name, cfg)
    }

    async fn validate_all(&self) -> Result<ValidationReport, ToolError> {
        let project_path = self
            .workspace_manager
            .config_manager
            .workspace_path()
            .to_path_buf();

        let config = oxy::config::ConfigBuilder::new()
            .with_workspace_path(&project_path)
            .map_err(|e| ToolError::Execution(format!("config error: {e}")))?
            .build()
            .await
            .map_err(|e| ToolError::Execution(format!("config error: {e}")))?;

        let cfg = config.get_config();
        let mut errors: Vec<ValidatedFile> = Vec::new();
        let mut valid_count: usize = 0;

        for path in cfg.list_workflows(&cfg.workspace_path) {
            let rel = path
                .strip_prefix(&project_path)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            match cfg
                .load_workflow(&path)
                .and_then(|w| cfg.validate_workflow(&w).map_err(Into::into))
            {
                Ok(()) => valid_count += 1,
                Err(e) => errors.push(ValidatedFile {
                    relative_path: rel,
                    error: Some(e.to_string()),
                }),
            }
        }

        for path in cfg.list_agents(&cfg.workspace_path) {
            let rel = path
                .strip_prefix(&project_path)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            match cfg
                .load_agent_config(Some(&path))
                .and_then(|(agent, name)| cfg.validate_agent(&agent, name).map_err(Into::into))
            {
                Ok(()) => valid_count += 1,
                Err(e) => errors.push(ValidatedFile {
                    relative_path: rel,
                    error: Some(e.to_string()),
                }),
            }
        }

        for path in cfg.list_apps(&cfg.workspace_path) {
            let rel = path
                .strip_prefix(&project_path)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            match cfg
                .load_app(&path)
                .and_then(|app| cfg.validate_app(&app).map_err(Into::into))
            {
                Ok(()) => valid_count += 1,
                Err(e) => errors.push(ValidatedFile {
                    relative_path: rel,
                    error: Some(e.to_string()),
                }),
            }
        }

        for path in list_semantic_files(&cfg.workspace_path) {
            let rel = path
                .strip_prefix(&project_path)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            match validate_semantic_file(&path) {
                Ok(()) => valid_count += 1,
                Err(e) => errors.push(ValidatedFile {
                    relative_path: rel,
                    error: Some(e),
                }),
            }
        }

        Ok(ValidationReport {
            valid_count,
            errors,
        })
    }
}

fn validate_single_file(
    abs: &Path,
    file_name: &str,
    cfg: &oxy::config::model::Config,
) -> Result<(), String> {
    if file_name.ends_with(".procedure.yml")
        || file_name.ends_with(".workflow.yml")
        || file_name.ends_with(".automation.yml")
    {
        let w = cfg
            .load_workflow(&abs.to_path_buf())
            .map_err(|e| e.to_string())?;
        cfg.validate_workflow(&w).map_err(|e| e.to_string())
    } else if file_name.ends_with(".agent.yml") {
        let (agent, name) = cfg
            .load_agent_config(Some(&abs.to_path_buf()))
            .map_err(|e| e.to_string())?;
        cfg.validate_agent(&agent, name).map_err(|e| e.to_string())
    } else if file_name.ends_with(".app.yml") {
        let app = cfg
            .load_app(&abs.to_path_buf())
            .map_err(|e| e.to_string())?;
        cfg.validate_app(&app).map_err(|e| e.to_string())
    } else if file_name.ends_with(".view.yml") || file_name.ends_with(".topic.yml") {
        validate_semantic_file(abs)
    } else {
        Err(format!(
            "unsupported file type: {file_name}. Expected .workflow.yml, .procedure.yml, \
             .automation.yml, .agent.yml, .app.yml, .view.yml, or .topic.yml"
        ))
    }
}

// ── Semantic file validation via airlayer ───────────────────────────────────

/// Validate a semantic file by parsing it through airlayer's type system.
fn validate_semantic_file(abs: &Path) -> Result<(), String> {
    let content = std::fs::read_to_string(abs).map_err(|e| e.to_string())?;
    let file_name = abs.file_name().and_then(|n| n.to_str()).unwrap_or("");

    if file_name.ends_with(".view.yml") {
        parse_view_yaml(&content)
            .map(|_| ())
            .map_err(|e| e.to_string())
    } else {
        parse_topic_yaml(&content)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

// ── Airlayer YAML parsing shims ────────────────────────────────────────────
//
// Thin wrappers that handle differences between oxy's YAML format and
// airlayer's expected types (e.g. optional `description` field).

#[derive(serde::Deserialize)]
struct ViewShim {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default, alias = "data_source")]
    datasource: Option<String>,
    #[serde(default)]
    dialect: Option<String>,
    #[serde(default)]
    table: Option<String>,
    #[serde(default)]
    sql: Option<String>,
    #[serde(default)]
    entities: Vec<airlayer::Entity>,
    #[serde(default)]
    dimensions: Vec<airlayer::Dimension>,
    #[serde(default)]
    measures: Option<Vec<airlayer::Measure>>,
    #[serde(default)]
    segments: Vec<airlayer::schema::models::Segment>,
}

#[derive(serde::Deserialize)]
struct TopicShim {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    views: Vec<String>,
    #[serde(default)]
    base_view: Option<String>,
    #[serde(default)]
    retrieval: Option<airlayer::schema::models::TopicRetrievalConfig>,
    #[serde(default)]
    default_filters: Option<Vec<airlayer::schema::models::TopicFilter>>,
}

fn parse_view_yaml(yaml: &str) -> Result<airlayer::View, Box<dyn std::error::Error + Send + Sync>> {
    let shim: ViewShim = serde_yaml::from_str(yaml)?;
    Ok(airlayer::View {
        name: shim.name,
        description: shim.description,
        label: shim.label,
        datasource: shim.datasource,
        dialect: shim.dialect,
        table: shim.table,
        sql: shim.sql,
        entities: shim.entities,
        dimensions: shim.dimensions,
        measures: shim.measures,
        segments: shim.segments,
        pre_aggregations: None,
        meta: None,
    })
}

fn parse_topic_yaml(
    yaml: &str,
) -> Result<airlayer::Topic, Box<dyn std::error::Error + Send + Sync>> {
    let shim: TopicShim = serde_yaml::from_str(yaml)?;
    Ok(airlayer::Topic {
        name: shim.name,
        description: shim.description,
        views: shim.views,
        base_view: shim.base_view,
        retrieval: shim.retrieval,
        default_filters: shim.default_filters,
        meta: None,
    })
}

fn list_semantic_files(project_path: &Path) -> Vec<PathBuf> {
    let semantics_dir = project_path.join("semantics");
    let mut files = Vec::new();
    for sub in &["views", "topics"] {
        let dir = semantics_dir.join(sub);
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str())
                    && (name.ends_with(".view.yml") || name.ends_with(".topic.yml"))
                {
                    files.push(path);
                }
            }
        }
    }
    files
}
