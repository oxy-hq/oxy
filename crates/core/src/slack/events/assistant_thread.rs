//! Assistant thread event handlers for Slack AI/Agent experience
//!
//! This module handles Slack's assistant-related events:
//! - `assistant_thread_started`
//! - `assistant_thread_context_changed`
//!
//! For v1, we provide minimal support: logging these events and optionally
//! routing them through the Oxy chat pipeline if they contain user queries.

use crate::errors::OxyError;
use crate::slack::events::execution::{
    SlackChatRequest, execute_oxy_chat_for_slack, load_slack_settings,
};
use uuid::Uuid;

/// Handle `assistant_thread_started` event
///
/// This event is fired when a user starts a conversation with the assistant
/// in Slack's agent/DM experience. If the event contains user query text,
/// we treat it as a user message and route it through Oxy chat.
pub async fn handle_assistant_thread_started(
    team_id: &str,
    channel_id: &str,
    user_id: Option<&str>,
    text: Option<&str>,
    thread_ts: Option<&str>,
    event_ts: &str,
    assistant_thread: Option<&serde_json::Value>,
) -> Result<(), OxyError> {
    tracing::info!(
        "Assistant thread started: team={}, channel={}, user={:?}, has_text={}",
        team_id,
        channel_id,
        user_id,
        text.is_some()
    );

    // Log assistant thread context if present
    if let Some(thread_info) = assistant_thread {
        tracing::debug!("Assistant thread context: {:?}", thread_info);
    }

    // If we have both user and text, treat this as a user message
    if let (Some(user), Some(message_text)) = (user_id, text) {
        if !message_text.trim().is_empty() {
            tracing::info!("Routing assistant thread start with user query through Oxy chat");

            let slack_settings = load_slack_settings().await?;

            execute_oxy_chat_for_slack(SlackChatRequest {
                team_id: team_id.to_string(),
                channel_id: channel_id.to_string(),
                user_id: user.to_string(),
                text: message_text.to_string(),
                thread_ts: thread_ts.map(|s| s.to_string()),
                event_ts: event_ts.to_string(),
                project_id: Uuid::nil(),
                agent_id: slack_settings.default_agent.clone(),
                slack_settings,
                is_dm: true,
            })
            .await?;
        } else {
            tracing::debug!("Assistant thread started with empty text, ignoring");
        }
    } else {
        tracing::debug!(
            "Assistant thread started without user/text (user={:?}, text={:?}), logging only",
            user_id,
            text
        );
    }

    Ok(())
}

/// Handle `assistant_thread_context_changed` event
///
/// This event is fired when the context of an assistant thread changes
/// (e.g., user switches between threads). For v1, we simply log this event.
pub async fn handle_assistant_thread_context_changed(
    team_id: &str,
    channel_id: &str,
    user_id: Option<&str>,
    thread_ts: Option<&str>,
    _event_ts: &str,
    assistant_thread: Option<&serde_json::Value>,
) -> Result<(), OxyError> {
    tracing::info!(
        "Assistant thread context changed: team={}, channel={}, user={:?}, thread_ts={:?}",
        team_id,
        channel_id,
        user_id,
        thread_ts
    );

    // Log assistant thread context if present
    if let Some(thread_info) = assistant_thread {
        tracing::debug!("New assistant thread context: {:?}", thread_info);
    }

    // For v1, we don't take action on context changes, just log them
    // Future enhancement: Could use this to switch Oxy sessions when user
    // switches between conversation threads

    Ok(())
}
