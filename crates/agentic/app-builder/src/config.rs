//! Configuration layer for [`AppBuilderSolver`].
//!
//! Reuses the same YAML structure as `agentic-analytics` (`AgentConfig`),
//! and provides a `build_app_solver_with_context` function to construct an
//! `AppBuilderSolver` from project-supplied connectors via `BuildContext`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// Re-export config types for consumers of this crate.
pub use agentic_analytics::{
    AgentConfig as AppBuilderConfig, BuildContext, ConfigError, StateConfig, ThinkingConfigYaml,
};

use agentic_analytics::config::LlmVendor;
use agentic_analytics::{LlmClient, OpenAiProvider, SemanticCatalog, DEFAULT_MODEL};
use agentic_llm::OpenAiCompatProvider;

use crate::solver::AppBuilderSolver;

// ── Build function ────────────────────────────────────────────────────────────

/// Build a fully configured [`AppBuilderSolver`] using connectors supplied
/// by the HTTP layer via [`BuildContext`].
///
/// Mirrors the steps in `AgentConfig::build_solver_with_context` but constructs
/// an `AppBuilderSolver` instead of `AnalyticsSolver`.
pub async fn build_app_solver_with_context(
    config: &AppBuilderConfig,
    base_dir: &Path,
    build_ctx: BuildContext,
) -> Result<(AppBuilderSolver, Vec<PathBuf>), ConfigError> {
    // 1. Resolve context globs.
    let ctx = config.resolve_context(base_dir)?;

    // 2. Merge connectors injected by the HTTP layer.
    if build_ctx.extra_connectors.is_empty() {
        return Err(ConfigError::NoDatabases);
    }

    let mut connectors: HashMap<String, Arc<dyn agentic_connector::DatabaseConnector>> =
        HashMap::new();

    for (name, connector) in build_ctx.extra_connectors {
        connectors.insert(name, connector);
    }

    let default_connector = build_ctx
        .extra_default_connector
        .or_else(|| connectors.keys().next().cloned())
        .unwrap_or_default();

    // 3. Load semantic catalog (after connectors so we have dialect info).
    let dialect_map =
        agentic_analytics::airlayer_compat::build_dialect_map(&connectors, &default_connector);
    let semantic = if ctx.semantic_files.is_empty() {
        None
    } else {
        Some(
            SemanticCatalog::load_files(&ctx.semantic_files, dialect_map)
                .map_err(ConfigError::SemanticError)?,
        )
    };

    // 4. Build SemanticCatalog.
    let catalog = match semantic {
        Some(sem) => sem,
        None => SemanticCatalog::empty(),
    };

    // 5. Build LlmClient — same precedence rules as AgentConfig.
    let model = config
        .llm
        .model
        .as_deref()
        .or(build_ctx.project_model.as_deref())
        .unwrap_or(DEFAULT_MODEL)
        .to_string();

    let effective_vendor = if build_ctx.has_explicit_ref {
        build_ctx
            .project_vendor
            .as_ref()
            .unwrap_or(&config.llm.vendor)
    } else if config.llm.model.is_some() {
        &config.llm.vendor
    } else {
        build_ctx
            .project_vendor
            .as_ref()
            .unwrap_or(&config.llm.vendor)
    };

    let api_key = config
        .llm
        .api_key
        .clone()
        .or(build_ctx.project_api_key)
        .or_else(|| match effective_vendor {
            LlmVendor::Anthropic => std::env::var("ANTHROPIC_API_KEY").ok(),
            LlmVendor::OpenAi | LlmVendor::OpenAiCompat => std::env::var("OPENAI_API_KEY").ok(),
        })
        .unwrap_or_default();

    let effective_base_url = config
        .llm
        .base_url
        .as_deref()
        .or(build_ctx.project_base_url.as_deref());

    let client = match effective_vendor {
        LlmVendor::Anthropic => LlmClient::with_model(api_key, model),
        LlmVendor::OpenAi => {
            let provider = if let Some(base_url) = effective_base_url {
                OpenAiProvider::with_base_url(api_key, model, base_url)
            } else {
                OpenAiProvider::new(api_key, model)
            };
            LlmClient::with_provider(provider)
        }
        LlmVendor::OpenAiCompat => {
            let base_url = effective_base_url.unwrap_or("http://localhost:11434/v1");
            let provider = OpenAiCompatProvider::new(api_key, model, base_url);
            LlmClient::with_provider(provider)
        }
    };

    // 6. Build AppBuilderSolver.
    let mut solver = AppBuilderSolver::new_multi(client, catalog, connectors, default_connector)
        .with_instructions(config.instructions.clone())
        .with_state_configs(config.states.clone())
        .with_max_tokens(config.llm.max_tokens);

    if let Some(thinking_cfg) = &config.thinking {
        solver = solver.with_global_thinking(thinking_cfg.to_thinking_config());
    }

    Ok((solver, ctx.procedure_files))
}
