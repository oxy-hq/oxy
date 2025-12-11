//! Direct message (DM) event handler for Slack AI/Agent experience
//!
//! This module handles `message.im` events (direct messages to the Oxy bot).
//! It reuses the same Oxy execution logic as `app_mention`, but without
//! channel bindings - DMs use a default or configured project/agent.

use crate::errors::OxyError;
use crate::slack::events::execution::{
    SlackChatRequest, execute_oxy_chat_for_slack, load_slack_settings,
};
use uuid::Uuid;

/// Handle Slack DM (message.im) event
///
/// This handles direct messages sent to the Oxy bot in the Slack agent/DM experience.
/// Unlike channel mentions, DMs don't require channel bindings and use a default
/// project/agent configuration.
///
/// Session management: Each DM channel acts as a continuous conversation thread.
/// When users click "Start new chat" in Slack's agent UI, Slack creates a new
/// DM channel, which naturally starts a new session.
pub async fn handle_message_im(
    team_id: &str,
    channel_id: &str,
    user_id: &str,
    text: &str,
    thread_ts: Option<&str>,
    event_ts: &str,
) -> Result<(), OxyError> {
    tracing::info!(
        "Handling DM message: team={}, channel={}, user={}",
        team_id,
        channel_id,
        user_id
    );

    // Load Slack settings from config.yml
    let slack_settings = load_slack_settings().await?;

    tracing::info!(
        "Using DM defaults: project=nil, agent={}",
        slack_settings.default_agent
    );

    // Execute Oxy chat using shared execution logic
    // Note: No bot mention stripping needed for DMs - user sent text directly
    execute_oxy_chat_for_slack(SlackChatRequest {
        team_id: team_id.to_string(),
        channel_id: channel_id.to_string(),
        user_id: user_id.to_string(),
        text: text.to_string(),
        thread_ts: thread_ts.map(|s| s.to_string()),
        event_ts: event_ts.to_string(),
        project_id: Uuid::nil(),
        agent_id: slack_settings.default_agent.clone(),
        slack_settings,
        is_dm: true,
    })
    .await
}
