//! /oxy unbind command handler

use crate::integrations::slack::services::ChannelBindingService;
use oxy_shared::errors::OxyError;

/// Handle `/oxy unbind` command
///
/// Removes the channel-to-project binding.
pub async fn handle_unbind_command(team_id: &str, channel_id: &str) -> Result<String, OxyError> {
    ChannelBindingService::unbind_channel(team_id, channel_id).await?;

    Ok("✅ This channel has been unbound.\n\nThe channel will now use the default agent from your config.yml.\n\nYou can re-bind it using `/oxy bind <project_id> <agent_id>`.".to_string())
}
