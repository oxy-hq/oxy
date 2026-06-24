//! Workspace context trait.
//!
//! `agentic-automation` needs workspace capabilities (file listing, database
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

/// The directory an agent's `context:` globs resolve against.
///
/// FS-backed hosts (the IDE working copy) return their workspace path with no
/// guard. A stateless host (the serve fleet, which has no working copy)
/// materialises the compiled context entities into a tempdir and returns a
/// guard that owns it. `path()` is valid only while the `ContextRoot` is alive,
/// so the caller MUST keep it in scope until context resolution (glob +
/// semantic-catalog load) has finished.
pub struct ContextRoot {
    path: PathBuf,
    // Opaque so this crate needn't depend on `tempfile`; the host boxes the
    // `TempDir` guard in here and it drops (cleaning up) with the `ContextRoot`.
    _guard: Option<Box<dyn std::any::Any + Send + Sync>>,
}

impl ContextRoot {
    /// Resolve context from an on-disk workspace path (IDE / shared-FS).
    pub fn fs(path: PathBuf) -> Self {
        Self { path, _guard: None }
    }

    /// Resolve context from a materialised tempdir; `guard` owns it for the
    /// `ContextRoot`'s lifetime (typically a `tempfile::TempDir`).
    pub fn materialised(path: PathBuf, guard: Box<dyn std::any::Any + Send + Sync>) -> Self {
        Self {
            path,
            _guard: Some(guard),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Minimal workspace interface needed by the automation engine.
///
/// Implemented by the pipeline layer for `oxy::adapters::workspace::WorkspaceManager`.
#[async_trait::async_trait]
pub trait WorkspaceContext: Send + Sync {
    /// Root path of the workspace/project.
    fn workspace_path(&self) -> &Path;

    /// Directory an agent's `context:` globs resolve against. Defaults to the
    /// on-disk workspace path; the host adapter overrides it to materialise the
    /// compiled context from the boundary on a stateless replica that has no
    /// working copy. The returned guard must outlive context resolution.
    async fn context_root(&self) -> ContextRoot {
        ContextRoot::fs(self.workspace_path().to_path_buf())
    }

    /// Database configurations for dialect mapping.
    fn database_configs(&self) -> Vec<airlayer::DatabaseConfig>;

    /// Get a pre-built database connector by name.
    async fn get_connector(&self, name: &str) -> Result<Arc<dyn DatabaseConnector>, String>;

    /// Get resolved integration credentials by name.
    async fn get_integration(&self, name: &str) -> Result<IntegrationConfig, String>;

    /// List all automation files in the workspace.
    async fn list_automation_files(&self) -> Result<Vec<PathBuf>, String>;

    /// Read the raw YAML content of an automation file.
    async fn resolve_automation_yaml(&self, automation_ref: &str) -> Result<String, String>;

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
