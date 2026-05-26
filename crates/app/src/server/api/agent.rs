//! Agent listing + builder-availability endpoints.
//!
//! The classic `.agent.yml` execution surface (preview/sync/test) has been
//! removed alongside the `oxy-agent` crate. Only listing — used by the chat
//! panel's agent selector — and the built-in builder readiness check remain
//! here, both serving the agentic-only world.

use crate::api::middlewares::workspace_context::WorkspaceManagerExtractor;
use axum::{extract, http::StatusCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize)]
pub struct BuilderAvailabilityResponse {
    pub available: bool,
    /// Always `None` now — the legacy path-based builder agent has been
    /// removed. Kept in the response shape so existing clients don't
    /// require a coordinated frontend redeploy.
    pub builder_path: Option<String>,
    /// `true` when the builder is the built-in copilot.
    pub builtin: bool,
    /// Model name for the built-in copilot; `None` when no builder is configured.
    pub model: Option<String>,
}

pub async fn check_builder_availability(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
) -> Result<extract::Json<BuilderAvailabilityResponse>, StatusCode> {
    use oxy::config::model::BuilderAgentConfig;

    match workspace_manager.config_manager.get_builder_config() {
        Some(BuilderAgentConfig::Builtin { model }) => {
            Ok(extract::Json(BuilderAvailabilityResponse {
                available: true,
                builder_path: None,
                builtin: true,
                model: Some(model.clone()),
            }))
        }
        None => Ok(extract::Json(BuilderAvailabilityResponse {
            available: false,
            builder_path: None,
            builtin: false,
            model: None,
        })),
    }
}

/// Minimal subset of the agentic.yml schema — we only need `llm.ref` to
/// populate `AgentConfigResponse.model`. Avoids importing the full
/// `agentic_analytics::AgentConfig` (which pulls solver build types) into
/// this listing endpoint.
#[derive(Deserialize)]
struct AgenticAgentSnippet {
    #[serde(default)]
    llm: Option<AgenticAgentLlmSnippet>,
}

#[derive(Deserialize)]
struct AgenticAgentLlmSnippet {
    #[serde(default)]
    r#ref: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentConfigResponse {
    pub name: String,
    pub public: bool,
    pub path: String,
    /// Model ref this agent resolves through. Lets the home page tie a
    /// readiness gap to the *agent the chat will actually use* rather than
    /// to "any LLM key is missing".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl AgentConfigResponse {
    pub fn new(name: String, path: String, public: bool) -> Self {
        Self {
            name,
            path,
            public,
            model: None,
        }
    }
}

/// List analytics agents (`.agentic.yml`) in a workspace.
#[utoipa::path(
    method(get),
    path = "/{workspace_id}/agents",
    params(
        ("workspace_id" = Uuid, Path, description = "Workspace UUID")
    ),
    responses(
        (status = OK, description = "Success", body = Vec<String>, content_type = "application/json")
    ),
    security(
        ("ApiKey" = [])
    )
)]
pub async fn get_agents(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
) -> Result<extract::Json<Vec<AgentConfigResponse>>, StatusCode> {
    let config_manager = &workspace_manager.config_manager;
    let workspace_path = config_manager.workspace_path();

    let analytics_paths = config_manager.list_analytics_agents().await.map_err(|e| {
        tracing::error!("Failed to list analytics agents: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let agent_relative_paths: Vec<String> = analytics_paths
        .iter()
        .filter_map(|agent| {
            agent
                .strip_prefix(workspace_path)
                .ok()
                .map(|path| path.to_string_lossy().to_string())
        })
        .collect();

    let agent_futures = agent_relative_paths
        .into_iter()
        .map(|path| async move {
            // `.agentic.yml` or `.agentic.yaml` — parse the file directly for
            // `llm.ref` to avoid pulling the analytics crate's solver-build
            // dependencies into the listing path.
            let agent_id = std::path::Path::new(&path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(&path)
                .trim_end_matches(".agentic.yaml")
                .trim_end_matches(".agentic.yml")
                .to_string();
            let abs_path = workspace_path.join(&path);
            let model_ref = tokio::fs::read_to_string(&abs_path)
                .await
                .ok()
                .and_then(|content| serde_yaml::from_str::<AgenticAgentSnippet>(&content).ok())
                .and_then(|snippet| snippet.llm.and_then(|l| l.r#ref));
            let mut resp = AgentConfigResponse::new(agent_id, path, true);
            resp.model = model_ref;
            Ok::<AgentConfigResponse, anyhow::Error>(resp)
        })
        .collect::<Vec<_>>();

    let agents: Vec<AgentConfigResponse> = futures::future::try_join_all(agent_futures)
        .await
        .map_err(|e| {
            tracing::error!("Failed to resolve agent configs: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(extract::Json(agents))
}
