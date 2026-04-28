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
    BuilderSemanticCompiler,
};
use agentic_connector::ConnectorConfig;
use agentic_llm::{LlmClient, OpenAiCompatProvider, OpenAiProvider};
use agentic_workflow::WorkspaceContext;
use async_trait::async_trait;

/// Project config access — connectors, models, secrets.
///
/// Returns agentic-owned types. The adapter is responsible for translating
/// host-specific config into these shapes and for `tracing::warn!`-ing when
/// something is missing.
#[async_trait]
pub trait ProjectContext: Send + Sync {
    async fn resolve_connector(&self, db_name: &str) -> Option<ConnectorConfig>;

    async fn resolve_model(
        &self,
        model_ref: Option<&str>,
        has_explicit_model: bool,
    ) -> Option<ResolvedModelInfo>;

    async fn resolve_secret(&self, var_name: &str) -> Option<String>;

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
}

/// Resolve a batch of database names to connector configs, skipping any
/// that fail to resolve.
pub async fn resolve_connectors(
    db_names: &[String],
    ctx: &dyn ProjectContext,
) -> Vec<(String, ConnectorConfig)> {
    let mut configs = Vec::with_capacity(db_names.len());
    for name in db_names {
        if let Some(cfg) = ctx.resolve_connector(name).await {
            configs.push((name.clone(), cfg));
        }
    }
    configs
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
