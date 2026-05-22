//! Workspace context trait.
//!
//! `agentic-workflow` needs workspace capabilities (file listing, database
//! access, integration access, path resolution) but does NOT depend on `oxy`.
//! The pipeline layer implements this trait for `oxy::WorkspaceManager`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use agentic_connector::DatabaseConnector;

use crate::refresh_key_cache::RefreshKeyCache;

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

    /// Return the shared in-process refresh key cache, if the server has one configured.
    ///
    /// Returns `None` in contexts without a long-lived cache (e.g. CLI, tests).
    fn refresh_key_cache(&self) -> Option<Arc<RwLock<RefreshKeyCache>>> {
        None
    }

    /// Renewal threshold (seconds) used on the query read-path to decide
    /// whether a cached refresh-key is still fresh. Must match the worker's
    /// `pre_aggregations.refresh_worker.renewal_threshold`. Defaults to 120s.
    fn preagg_renewal_threshold_secs(&self) -> u64 {
        120
    }

    /// List all `.airway.yml` pipeline files in the workspace. Default
    /// returns empty so existing impls (test fakes) need no change; the
    /// real host adapter overrides it.
    async fn list_airway_files(&self) -> Result<Vec<PathBuf>, String> {
        Ok(vec![])
    }
}
