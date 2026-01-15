//! App mention event handler - core Slack→Oxy synchronization flow

use crate::integrations::slack::events::execution::{
    SlackChatRequest, execute_oxy_chat_for_slack, load_slack_settings,
};
use crate::integrations::slack::services::ChannelBindingService;
use crate::integrations::slack::utils::strip_bot_mention;
use oxy_shared::errors::OxyError;
use uuid::Uuid;

/// Handle Slack app mention event
///
/// This is the core synchronization flow: @oxy mention → Oxy chat → Slack response
pub async fn handle_app_mention(
    team_id: &str,
    channel_id: &str,
    user_id: &str,
    text: &str,
    thread_ts: Option<&str>,
    event_ts: &str,
) -> Result<(), OxyError> {
    tracing::info!(
        "Handling app mention: team={}, channel={}, user={}",
        team_id,
        channel_id,
        user_id
    );

    // Load Slack settings from config.yml
    let slack_settings = load_slack_settings().await?;

    // Check for channel binding (overrides defaults)
    let binding = ChannelBindingService::find_binding(team_id, channel_id).await?;

    let (project_id, agent_id) = if let Some(b) = binding {
        // Channel is explicitly bound - use the binding
        tracing::info!(
            "Channel binding found: project={}, agent={}",
            b.oxy_project_id,
            b.default_agent_id
        );
        (b.oxy_project_id, b.default_agent_id)
    } else {
        // No binding - use defaults from config.yml
        tracing::info!(
            "No channel binding, using defaults: project=nil, agent={}",
            slack_settings.default_agent
        );
        (Uuid::nil(), slack_settings.default_agent.clone())
    };

    // Strip bot mention from text and execute
    let cleaned_text = strip_bot_mention(text);

    execute_oxy_chat_for_slack(SlackChatRequest {
        team_id: team_id.to_string(),
        channel_id: channel_id.to_string(),
        user_id: user_id.to_string(),
        text: cleaned_text,
        thread_ts: thread_ts.map(|s| s.to_string()),
        event_ts: event_ts.to_string(),
        project_id,
        agent_id,
        slack_settings,
        is_dm: false,
    })
    .await
}
