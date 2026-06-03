//! Port traits for what the pipeline needs from the host project.
//!
//! Pipeline defines the contracts; adapters live in the host application
//! (`app::agentic_wiring`). This crate is platform-free — all `oxy::*`
//! imports are on the other side of these traits.
//!
//! # Traits
//!
//! - [`ProjectContext`] — connector, model, secret resolution.
//! - [`ThreadOwnerLookup`] — thread ownership query (used by HTTP for auth).
//! - [`PlatformContext`] — supertrait combining [`ProjectContext`] and
//!   [`agentic_workflow::WorkspaceContext`]. The full platform handle.
//!
//! # Bundles
//!
//! - [`BuilderBridges`] — the four [`agentic_builder`] port impls required to
//!   start a builder pipeline. Built by the host and passed to
//!   [`PipelineBuilder`](crate::PipelineBuilder).

use std::sync::Arc;

use agentic_analytics::SharedMetricSink;
use agentic_analytics::config::{LlmVendor, ResolvedModelInfo};
use agentic_builder::{
    BuilderDatabaseProvider, BuilderProjectValidator, BuilderSchemaProvider,
    BuilderSecretsProvider, BuilderSemanticCompiler,
};
use agentic_connector::{ConnectorConfig, DatabaseConnector};
use agentic_llm::{LlmClient, OpenAiCompatProvider, OpenAiProvider};
use agentic_workflow::WorkspaceContext;
use async_trait::async_trait;
use std::collections::HashMap;

/// A `config.yml` database resolved into an airway destination: the
/// destination `kind` airway should build, plus an already-credentialed
/// connection string (secrets resolved, `airhouse_managed` minted).
#[derive(Debug, Clone)]
pub struct ResolvedPipelineDestination {
    /// airway destination kind (`postgres`, `airhouse`, …).
    pub kind: String,
    /// Connection string with credentials already substituted in.
    pub connection_string: String,
}

/// Project config access — connectors, models, secrets.
///
/// Returns agentic-owned types. The adapter is responsible for translating
/// host-specific config into these shapes and for `tracing::warn!`-ing when
/// something is missing.
#[async_trait]
pub trait ProjectContext: Send + Sync {
    async fn resolve_connector(&self, db_name: &str) -> Option<ConnectorConfig>;

    /// Resolve a `config.yml` database name into an airway pipeline
    /// destination (kind + credentialed connection string). Handles
    /// secret substitution and per-subject `airhouse_managed` minting.
    /// Returns `None` when the database is unknown or its type has no
    /// airway destination mapping. Default `None` so adapters without
    /// airway support compile unchanged.
    async fn resolve_pipeline_destination(
        &self,
        _db_name: &str,
    ) -> Option<ResolvedPipelineDestination> {
        None
    }

    /// Return a connector instance the host built itself, when the database
    /// type isn't representable as a [`ConnectorConfig`] variant in
    /// `agentic-connector`. Hosts use this for backends whose driver lives in
    /// a separate crate (e.g. `airhouse`). Default impl returns `None` so
    /// existing adapters compile unchanged; resolution falls through to
    /// [`Self::resolve_connector`].
    async fn resolve_pre_built_connector(
        &self,
        _db_name: &str,
    ) -> Option<Arc<dyn DatabaseConnector>> {
        None
    }

    async fn resolve_model(
        &self,
        model_ref: Option<&str>,
        has_explicit_model: bool,
    ) -> Option<ResolvedModelInfo>;

    async fn resolve_secret(&self, var_name: &str) -> Option<String>;

    /// Workspace identifier stamped onto every run this context starts
    /// (`agentic_runs.workspace_id`). HTTP handlers + the scheduler tick
    /// pass this into `start_*_run` so out-of-process drivers (recovery
    /// loop, latency worker) can look the cached `PlatformContext` back
    /// up by id when they pick up a queued row.
    ///
    /// Defaults to the nil UUID so test fakes + the local single-workspace
    /// mode (where nil == `LOCAL_WORKSPACE_ID`) compile unchanged.
    fn workspace_id(&self) -> uuid::Uuid {
        uuid::Uuid::nil()
    }

    /// Optional sink for Tier 1 analytics metric usage. Hosts with an
    /// observability backend return an adapter that writes into it;
    /// hosts without one (tests, embedded use) return `None` and
    /// metric recording is a silent no-op.
    ///
    /// Default impl returns `None` so existing platform adapters keep
    /// compiling unchanged.
    fn metric_sink(&self) -> Option<SharedMetricSink> {
        None
    }
}

/// Thread-ownership lookup for transport-layer auth checks.
///
/// Implemented by the host against its threads table. Pipeline + HTTP call
/// into this trait instead of importing a `threads` entity directly.
///
/// Returns `Ok(None)` when the thread does not exist; `Ok(Some(None))` when
/// the thread exists but has no owner; `Ok(Some(Some(id)))` when owned.
#[async_trait]
pub trait ThreadOwnerLookup: Send + Sync {
    async fn thread_owner(
        &self,
        thread_id: uuid::Uuid,
    ) -> Result<Option<Option<uuid::Uuid>>, String>;

    /// Create a new conversation thread in `project_id` (the run's
    /// workspace) titled `title`, returning the new thread id.
    ///
    /// Used by the run handler to auto-provision a thread when a client
    /// starts a run without one — so the run's `thread_id` FK is satisfied
    /// and the client can reuse the returned id for follow-up questions.
    ///
    /// Default impl returns `Err` so fakes that only implement ownership
    /// lookups (tests, embedded use) compile unchanged; the host adapter
    /// (`OxyThreadOwnerLookup`) inserts a real `threads` row.
    async fn create_thread(
        &self,
        _project_id: uuid::Uuid,
        _title: &str,
    ) -> Result<uuid::Uuid, String> {
        Err("thread creation not supported by this ThreadOwnerLookup".to_string())
    }
}

/// Combined platform handle.
///
/// Pipeline uses this anywhere it needs both project config *and* workflow
/// workspace operations from the same object. The host provides a single
/// concrete type (e.g. `app::agentic_wiring::OxyProjectContext`) that
/// implements both of the component traits; the blanket impl below lifts
/// that into [`PlatformContext`] automatically.
pub trait PlatformContext: ProjectContext + WorkspaceContext {}

impl<T> PlatformContext for T where T: ProjectContext + WorkspaceContext + ?Sized {}

/// The four builder-domain port impls required to start a builder pipeline.
///
/// Cheap to clone — every field is an `Arc<dyn ...>`. Callers assemble this
/// once per workspace and pass it into [`PipelineBuilder::with_builder_bridges`](
/// crate::PipelineBuilder::with_builder_bridges) (and into
/// [`PipelineTaskExecutor`](crate::executor::PipelineTaskExecutor) for
/// delegation).
#[derive(Clone)]
pub struct BuilderBridges {
    pub db_provider: Arc<dyn BuilderDatabaseProvider>,
    pub project_validator: Arc<dyn BuilderProjectValidator>,
    pub schema_provider: Arc<dyn BuilderSchemaProvider>,
    pub semantic_compiler: Arc<dyn BuilderSemanticCompiler>,
    pub secrets_provider: Option<Arc<dyn BuilderSecretsProvider>>,
}

/// Result of resolving a batch of database names: configs that
/// `agentic-connector` knows how to dispatch, plus host-built connector
/// instances for backends whose drivers live outside `agentic-connector`.
pub struct ResolvedConnectors {
    pub configs: Vec<(String, ConnectorConfig)>,
    pub pre_built: HashMap<String, Arc<dyn DatabaseConnector>>,
}

/// Resolve a batch of database names. For each name, the host's pre-built
/// path is checked first (so a host can override the dispatch), then the
/// config path. Names that resolve to neither are silently skipped.
pub async fn resolve_connectors(
    db_names: &[String],
    ctx: &dyn ProjectContext,
) -> ResolvedConnectors {
    let mut configs = Vec::new();
    let mut pre_built = HashMap::new();
    for name in db_names {
        if let Some(conn) = ctx.resolve_pre_built_connector(name).await {
            pre_built.insert(name.clone(), conn);
        } else if let Some(cfg) = ctx.resolve_connector(name).await {
            configs.push((name.clone(), cfg));
        }
    }
    ResolvedConnectors { configs, pre_built }
}

/// Build an [`LlmClient`] from a [`ResolvedModelInfo`], dispatching on vendor.
///
/// Azure OpenAI models are detected via `azure_deployment_id` / `azure_api_version`
/// and routed to [`OpenAiCompatProvider`] (Chat Completions) with the correct
/// deployment URL, bypassing the Responses API used by [`OpenAiProvider`].
pub fn build_llm_client(info: &ResolvedModelInfo) -> LlmClient {
    let api_key = info.api_key.as_deref().unwrap_or("");
    if let (Some(deployment_id), Some(api_version), Some(base_url)) = (
        info.azure_deployment_id.as_deref(),
        info.azure_api_version.as_deref(),
        info.base_url.as_deref(),
    ) {
        return LlmClient::with_provider(OpenAiCompatProvider::for_azure(
            api_key,
            &info.model,
            base_url,
            deployment_id,
            api_version,
        ));
    }
    if info.azure_deployment_id.is_some()
        && info.azure_api_version.is_some()
        && info.base_url.is_none()
    {
        tracing::warn!(
            "Azure config has deployment_id and api_version set but no base_url; \
             falling back to standard OpenAI."
        );
    } else if info.azure_deployment_id.is_some() != info.azure_api_version.is_some() {
        tracing::warn!(
            "Azure config is incomplete: both azure_deployment_id and azure_api_version must \
             be set together. Falling back to standard OpenAI."
        );
    }
    match &info.vendor {
        LlmVendor::Anthropic => LlmClient::with_model(api_key, &info.model),
        LlmVendor::OpenAi => {
            let provider = if let Some(url) = &info.base_url {
                OpenAiProvider::with_base_url(api_key, &info.model, url)
            } else {
                OpenAiProvider::new(api_key, &info.model)
            };
            LlmClient::with_provider(provider)
        }
        LlmVendor::OpenAiCompat => {
            let url = info
                .base_url
                .as_deref()
                .unwrap_or("http://localhost:11434/v1");
            LlmClient::with_provider(OpenAiCompatProvider::new(api_key, &info.model, url))
        }
    }
}
