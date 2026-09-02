//! `AppState` — the shared Axum application state.
//!
//! Lifted into `oxy-app-core` so the router that mounts routes (and future
//! per-surface crates that take `State<AppState>`) can depend on it **without**
//! depending on `oxy-app`. Every field is a lower-crate type or one of the
//! workspace caches that moved down with it, so no oxy-app back-edge is
//! introduced. Re-exported from the original `crate::server::router::AppState`
//! path in oxy-app, so existing call sites are unchanged.

use crate::serve_mode::ServeMode;
use crate::workspace_cache::{SemanticEngineCache, SemanticLayerCache};

#[derive(Clone)]
pub struct AppState {
    pub enterprise: bool,
    pub internal: bool,
    pub mode: ServeMode,
    pub observability: Option<std::sync::Arc<dyn oxy_observability::ObservabilityStore>>,
    /// The server's working directory at startup. In local mode, used as the
    /// target for `POST /{workspace_id}/setup/*`. In cloud/internal mode,
    /// unused — populated with `PathBuf::new()`.
    pub startup_cwd: std::path::PathBuf,
    /// Shared Layer-1 preagg refresh-key cache. Set when a background preagg
    /// worker is running (i.e. `startup_cwd` is non-empty). `None` in the
    /// internal API router and when no workspace path is configured.
    pub preagg_cache: Option<
        std::sync::Arc<std::sync::RwLock<agentic_semantic::refresh_key_cache::RefreshKeyCache>>,
    >,
    /// Renewal threshold (seconds) for the preagg refresh-key cache.
    /// Mirrors the worker's `pre_aggregations.refresh_worker.renewal_threshold`
    /// so the query read-path uses the operator-configured value, not a
    /// hardcoded default. `None` when no worker is running.
    pub preagg_renewal_threshold_secs: Option<u64>,
    /// Shared agentic state — runtime, schema cache, event registry,
    /// task router. Populated for the main API router so custom-app
    /// endpoints (useAsk, useProcedureRun, useAgentRun) can reach the
    /// pipeline. `None` for the internal API router (no agentic
    /// surface needed there). Handlers should 503 when this is
    /// `None` rather than panic.
    pub agentic_state: Option<std::sync::Arc<agentic_http::AgenticState>>,
    /// Shared per-workspace semantic model cache. Avoids re-reading and
    /// re-parsing all `.view.yml`/`.topic.yml` files on every request.
    /// Keyed by workspace UUID; TTL of 60 s with explicit invalidation on
    /// semantic file writes.
    pub semantic_layer_cache: std::sync::Arc<SemanticLayerCache>,
    /// Compiled SemanticEngine cache (join graph + evaluator).
    /// Avoids rebuilding the engine on every compilation request.
    pub semantic_engine_cache: std::sync::Arc<SemanticEngineCache>,
}
