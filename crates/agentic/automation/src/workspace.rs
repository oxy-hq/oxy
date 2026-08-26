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

    async fn get_connector(&self, name: &str) -> Result<Arc<dyn DatabaseConnector>, String>;

    async fn get_integration(&self, name: &str) -> Result<IntegrationConfig, String>;

    /// Fetch a project secret by name (Oxy Secrets, falling back to env var).
    ///
    /// Used by the `http_request` task to inject credentials into headers/body.
    /// Defaults to `None` so existing impls (test fakes) need no change; the real
    /// host adapter overrides it to forward to the `SecretsManager`. (Named
    /// distinctly from the pipeline `ProjectContext::resolve_secret` so a type
    /// implementing both traits has no ambiguous call.)
    async fn fetch_secret(&self, _name: &str) -> Option<String> {
        None
    }

    /// Store (upsert) a project secret — used by `http_request`'s
    /// `persist_to_secret` to write a rotated OAuth refresh token back to the
    /// secret store (mirrors the Airway Intuit source). Defaults to an error so
    /// only hosts with real secret storage allow it.
    async fn store_secret(&self, _name: &str, _value: &str) -> Result<(), String> {
        Err("secret persistence is not supported in this context".to_string())
    }

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

    /// The workspace this context belongs to — the pre-aggregation cache key.
    /// `None` in contexts with no workspace row (CLI, tests), which disables
    /// the local-rollup short-circuit rather than guessing a key.
    fn preagg_workspace_id(&self) -> Option<uuid::Uuid> {
        None
    }

    /// Where to read a rollup this node did not build. `None` keeps single-node
    /// behavior: the local file is the only copy.
    fn preagg_blob(&self) -> Option<agentic_semantic::BlobConfig> {
        None
    }

    /// The assembled local-rollup short-circuit, or `None` to compile every
    /// query to warehouse SQL. Assembled here, from one set of accessors, so
    /// no caller has to remember which pieces belong together.
    fn preagg_context(&self) -> Option<agentic_semantic::PreaggContext> {
        Some(agentic_semantic::PreaggContext {
            workspace_id: self.preagg_workspace_id()?,
            cache: self.refresh_key_cache()?,
            renewal_threshold_secs: self.preagg_renewal_threshold_secs(),
            blob: self.preagg_blob(),
        })
    }

    /// List all `.airway.yml` pipeline files in the workspace. Default
    /// returns empty so existing impls (test fakes) need no change; the
    /// real host adapter overrides it.
    async fn list_airway_files(&self) -> Result<Vec<PathBuf>, String> {
        Ok(vec![])
    }

    /// Compile-boundary hook for an airway pipeline (`.airway.yml`) body,
    /// keyed by its workspace-relative `pipeline_ref`.
    ///
    /// `Ok(Some(yaml))` — the host served the pipeline from its compiled
    /// `airway_pipelines` rows; the caller parses that string and never
    /// touches the filesystem. This is what lets the durable worker fleet
    /// (stateless, no working copy) run a pipeline at all.
    ///
    /// `Ok(None)` (the default) — "read the workspace filesystem". Covers hosts
    /// that don't participate in the compile boundary (test fakes, CLI) *and*
    /// the host's own fall-through cases (unpromoted workspace, draft branch,
    /// no matching row). The FS read + its containment guard live in one place
    /// on the caller side:
    /// [`agentic_pipeline::pipeline_ref::load_pipeline_yaml`].
    ///
    /// `Err` — the boundary could not be *asked*: a lookup error, not an
    /// answer. This is a `Result` rather than an `Option` for exactly that
    /// distinction. While it was one, a host had no way to say "I don't know"
    /// and had to report a database blip as `None`, which reads as "nothing is
    /// compiled here" — so a retryable condition became a terminal run on a
    /// replica with no working copy to fall back to. An `Option` cannot carry
    /// the difference between *absent* and *unknown*, and every caller that
    /// has to guess gets it wrong in the direction of the cheerful answer.
    ///
    /// Mirrors `ProjectContext::resolve_agent_yaml`, the same hook shape for
    /// `.agentic.yml`.
    async fn resolve_pipeline_yaml(&self, _pipeline_ref: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
}
