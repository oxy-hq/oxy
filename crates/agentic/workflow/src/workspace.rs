//! Workspace context trait.
//!
//! `agentic-workflow` needs workspace capabilities (file listing, database
//! access, integration access, path resolution) but does NOT depend on `oxy`.
//! The pipeline layer implements this trait for `oxy::WorkspaceManager`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentic_connector::DatabaseConnector;

/// Resolved integration credentials ready for use.
///
/// The pipeline layer resolves secrets and passes plain values.
#[derive(Debug, Clone)]
pub enum IntegrationConfig {
    Omni {
        base_url: String,
        api_key: String,
    },
    Looker {
        base_url: String,
        client_id: String,
        client_secret: String,
    },
}

/// Minimal workspace interface needed by the workflow engine.
///
/// Implemented by the pipeline layer for `oxy::adapters::workspace::WorkspaceManager`.
#[async_trait::async_trait]
pub trait WorkspaceContext: Send + Sync {
    /// Root path of the workspace/project.
    fn workspace_path(&self) -> &Path;

    /// Database configurations for dialect mapping.
    fn database_configs(&self) -> Vec<airlayer::DatabaseConfig>;

    /// Get a pre-built database connector by name.
    async fn get_connector(&self, name: &str) -> Result<Arc<dyn DatabaseConnector>, String>;

    /// Get resolved integration credentials by name.
    async fn get_integration(&self, name: &str) -> Result<IntegrationConfig, String>;

    /// List all workflow/procedure files in the workspace.
    async fn list_workflow_files(&self) -> Result<Vec<PathBuf>, String>;

    /// Read the raw YAML content of a workflow file.
    async fn resolve_workflow_yaml(&self, workflow_ref: &str) -> Result<String, String>;
}
