//! HTTP handlers for Slack integration
//!
//! This module contains the API handlers for:
//! - Slack Events API (POST /api/slack/events)
//! - Slack Slash Commands (POST /api/slack/commands)

use crate::adapters::secrets::SecretsManager;
use crate::errors::OxyError;
use crate::slack::commands::{handle_bind_command, handle_query_command, handle_unbind_command};
use crate::slack::events::{
    handle_app_mention, handle_assistant_thread_context_changed, handle_assistant_thread_started,
    handle_message_im, handle_url_verification, load_slack_settings,
};
use crate::slack::signature::verify_request;
use crate::slack::types::{Event, EventCallback, EventPayload, SlashCommand};
use crate::slack::utils::contains_bot_mention;
use axum::{
    body::Bytes,
    extract::Json,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde_json::json;

// ============================================================================
// Events Handler
// ============================================================================

/// Handle Slack Events API HTTP requests
///
/// POST /api/slack/events
pub async fn handle_events(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    // Load signing secret from config
    let signing_secret = match load_signing_secret().await {
        Ok(secret) => secret,
        Err(e) => {
            tracing::error!("Failed to load Slack signing secret: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "slack_not_configured" })),
            )
                .into_response();
        }
    };

    // Verify signature
    if let Err(e) = verify_request(&signing_secret, &headers, &body) {
        tracing::warn!("Slack signature verification failed: {}", e);
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid_signature" })),
        )
            .into_response();
    }

    // Parse payload using discriminated union
    let payload: EventPayload = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to parse Slack event payload: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "invalid_json" })),
            )
                .into_response();
        }
    };

    match payload {
        EventPayload::UrlVerification(verification) => {
            let response = handle_url_verification(&verification);
            (StatusCode::OK, Json(response)).into_response()
        }
        EventPayload::EventCallback(callback) => {
            // Dispatch event in background
            tokio::spawn(async move {
                if let Err(e) = dispatch_event(callback).await {
                    tracing::error!("Failed to handle Slack event: {}", e);
                }
            });

            // Respond immediately to Slack
            (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
        }
    }
}

/// Dispatch Slack event to appropriate handler
async fn dispatch_event(callback: EventCallback) -> Result<(), OxyError> {
    match callback.event {
        Event::AppMention {
            user,
            text,
            ts,
            channel,
            thread_ts,
        } => {
            handle_app_mention(
                &callback.team_id,
                &channel,
                &user,
                &text,
                thread_ts.as_deref(),
                &ts,
            )
            .await?;
        }
        Event::Message {
            user,
            text,
            ts,
            channel,
            thread_ts,
            channel_type,
            subtype,
            bot_id,
        } => {
            // Ignore bot messages to avoid echo loops
            if bot_id.is_some() {
                tracing::debug!("Ignoring bot message to avoid echo loop");
                return Ok(());
            }

            // Ignore messages without user or text
            if user.is_none() || text.is_none() {
                tracing::debug!("Ignoring message without user or text");
                return Ok(());
            }

            // Ignore bot_message and message_changed subtypes
            if let Some(st) = &subtype
                && (st == "bot_message" || st == "message_changed")
            {
                tracing::debug!("Ignoring message with subtype: {}", st);
                return Ok(());
            }

            let user = user.as_ref().unwrap();
            let text = text.as_ref().unwrap();

            // Check if this is a DM (message.im)
            if channel_type.as_deref() == Some("im") {
                tracing::info!("Processing DM message: channel={}, user={}", channel, user);

                handle_message_im(
                    &callback.team_id,
                    &channel,
                    user,
                    text,
                    thread_ts.as_deref(),
                    &ts,
                )
                .await?;
            } else if let Some(thread_ts_val) = &thread_ts {
                // This is a message in a thread
                if thread_ts_val != &ts {
                    // This is a reply, not the root message
                    //
                    // IMPORTANT: If the message contains a bot mention (@oxy), we skip it here
                    // because Slack will also send an `app_mention` event for the same message.
                    // The `app_mention` handler will process it, so we don't want to double-process.
                    //
                    // This means thread replies WITHOUT @oxy mentions are ignored (allowing
                    // users to have side conversations), and thread replies WITH @oxy mentions
                    // are handled by the `app_mention` event handler.
                    if contains_bot_mention(text) {
                        tracing::debug!(
                            "Skipping thread reply with bot mention (will be handled by app_mention): channel={}, thread_ts={}",
                            channel,
                            thread_ts_val
                        );
                        return Ok(());
                    }

                    // Thread reply without bot mention - ignore it
                    // (allows users to have side conversations in threads)
                    tracing::debug!(
                        "Ignoring thread reply without bot mention: channel={}, thread_ts={}",
                        channel,
                        thread_ts_val
                    );
                }
            } else {
                // Regular channel message, not in a thread and not a DM
                tracing::debug!(
                    "Ignoring regular message event: channel={}, channel_type={:?}",
                    channel,
                    channel_type
                );
            }
        }
        Event::AssistantThreadStarted {
            user,
            text,
            ts,
            channel,
            thread_ts,
            assistant_thread,
        } => {
            let ts_str = ts.as_deref().unwrap_or("unknown");
            let channel_str = channel.as_deref().unwrap_or("unknown");

            handle_assistant_thread_started(
                &callback.team_id,
                channel_str,
                user.as_deref(),
                text.as_deref(),
                thread_ts.as_deref(),
                ts_str,
                assistant_thread.as_ref(),
            )
            .await?;
        }
        Event::AssistantThreadContextChanged {
            user,
            ts,
            channel,
            thread_ts,
            assistant_thread,
        } => {
            let ts_str = ts.as_deref().unwrap_or("unknown");
            let channel_str = channel.as_deref().unwrap_or("unknown");

            handle_assistant_thread_context_changed(
                &callback.team_id,
                channel_str,
                user.as_deref(),
                thread_ts.as_deref(),
                ts_str,
                assistant_thread.as_ref(),
            )
            .await?;
        }
    }

    Ok(())
}

// ============================================================================
// Commands Handler
// ============================================================================

/// Handle Slack slash commands
///
/// POST /api/slack/commands
pub async fn handle_commands(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    // Load signing secret from config
    let signing_secret = match load_signing_secret().await {
        Ok(secret) => secret,
        Err(e) => {
            tracing::error!("Failed to load Slack signing secret: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Slack not configured".to_string(),
            )
                .into_response();
        }
    };

    // Verify signature
    if let Err(e) = verify_request(&signing_secret, &headers, &body) {
        tracing::warn!("Slack signature verification failed: {}", e);
        return (StatusCode::UNAUTHORIZED, "Invalid signature".to_string()).into_response();
    }

    // Parse form data
    let command: SlashCommand = match serde_urlencoded::from_bytes(&body) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to parse slash command: {}", e);
            return (StatusCode::BAD_REQUEST, "Invalid command format").into_response();
        }
    };

    // Ensure it's the /oxy command
    if command.command != "/oxy" {
        return (StatusCode::BAD_REQUEST, "Unknown command").into_response();
    }

    // Parse subcommand
    let text = command.text.trim();
    let result = if text.starts_with("bind") {
        let args = text.strip_prefix("bind").unwrap_or("").trim();
        handle_bind_command(
            &command.team_id,
            &command.channel_id,
            &command.user_id,
            args,
        )
        .await
    } else if text.starts_with("unbind") {
        handle_unbind_command(&command.team_id, &command.channel_id).await
    } else if !text.is_empty() {
        // Treat as query
        handle_query_command(
            &command.team_id,
            &command.channel_id,
            &command.user_id,
            text,
            &command.response_url,
        )
        .await
    } else {
        Ok("❌ Usage:\n\
            • `/oxy bind <project_id> <agent_id>` - Bind this channel to an Oxy project\n\
            • `/oxy unbind` - Unbind this channel\n\
            • `/oxy <your question>` - Ask Oxy a question"
            .to_string())
    };

    match result {
        Ok(message) => (StatusCode::OK, message).into_response(),
        Err(e) => {
            tracing::error!("Command handler error: {}", e);
            (StatusCode::OK, format!("❌ Error: {}", e)).into_response()
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Load signing secret from project config.yml
///
/// This is used to verify incoming Slack request signatures.
async fn load_signing_secret() -> Result<String, OxyError> {
    let settings = load_slack_settings().await?;
    let secret_manager = SecretsManager::from_environment()?;
    settings.get_signing_secret(&secret_manager).await
}
