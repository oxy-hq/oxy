//! /oxy bind command handler

use crate::integrations::slack::services::ChannelBindingService;
use oxy::adapters::workspace::builder::WorkspaceBuilder;
use oxy::adapters::workspace::resolve_workspace_path;
use oxy_shared::errors::OxyError;
use uuid::Uuid;

/// Handle `/oxy bind <agent_id>` command
///
/// Binds a Slack channel to a specific agent.
pub async fn handle_bind_command(
    team_id: &str,
    channel_id: &str,
    user_id: &str,
    args: &str,
) -> Result<String, OxyError> {
    // Parse arguments
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.is_empty() || parts[0].is_empty() {
        return Ok(
            "❌ Usage: `/oxy bind <agent_id>`\n\nExample: `/oxy bind agents/sales.agent.yml`"
                .to_string(),
        );
    }

    if parts.len() > 1 {
        return Ok("❌ Too many arguments. Usage: `/oxy bind <agent_id>`\n\n\
            Note: Agent paths cannot contain spaces."
            .to_string());
    }

    let agent_id = parts[0];

    // Use nil UUID for local/default project.  TODO: This only works for local deployments
    // that effectively have one project. For cloud deployments, we may need to add support
    // for workspace / project bindings.
    let workspace_id = Uuid::nil();

    // Validate that the agent exists in the project
    let workspace_path = resolve_workspace_path(workspace_id)
        .await
        .map_err(|e| OxyError::ValidationError(format!("Failed to load project: {}", e)))?;

    let workspace_manager = WorkspaceBuilder::new(workspace_id)
        .with_workspace_path_and_fallback_config(&workspace_path)
        .await
        .map_err(|e| OxyError::ValidationError(format!("Failed to load project config: {}", e)))?
        .try_with_intent_classifier()
        .await
        .build()
        .await
        .map_err(|e| OxyError::ValidationError(format!("Failed to build project: {}", e)))?;

    workspace_manager
        .config_manager
        .resolve_agent(agent_id)
        .await
        .map_err(|_| {
            OxyError::ValidationError(format!("Agent '{}' not found in project", agent_id))
        })?;

    // Create or update binding
    ChannelBindingService::bind_channel(
        team_id.to_string(),
        channel_id.to_string(),
        workspace_id,
        agent_id.to_string(),
        user_id.to_string(),
    )
    .await?;

    Ok(format!(
        "✅ This channel is now bound to agent `{}`.\n\n\
        This overrides the default agent configuration.\n\
        Mention @Oxy to interact with this agent.",
        agent_id
    ))
}
