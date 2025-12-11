//! Slack API type definitions
//!
//! Contains types shared across the Slack module for events, commands, etc.
//! Response types used only by the client are defined in client.rs.

use serde::{Deserialize, Serialize};

/// Slack event callback wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventCallback {
    pub token: String,
    pub team_id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub event: Event,
    pub event_id: String,
    pub event_time: i64,
}

/// Slack URL verification challenge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlVerification {
    pub token: String,
    pub challenge: String,
    #[serde(rename = "type")]
    pub event_type: String,
}

/// Slack event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    #[serde(rename = "app_mention")]
    AppMention {
        user: String,
        text: String,
        ts: String,
        channel: String,
        #[serde(default)]
        thread_ts: Option<String>,
    },
    #[serde(rename = "message")]
    Message {
        #[serde(default)]
        user: Option<String>,
        #[serde(default)]
        text: Option<String>,
        ts: String,
        channel: String,
        #[serde(default)]
        thread_ts: Option<String>,
        #[serde(default)]
        channel_type: Option<String>,
        #[serde(default)]
        subtype: Option<String>,
        #[serde(default)]
        bot_id: Option<String>,
    },
    #[serde(rename = "assistant_thread_started")]
    AssistantThreadStarted {
        #[serde(default)]
        user: Option<String>,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        ts: Option<String>,
        #[serde(default)]
        channel: Option<String>,
        #[serde(default)]
        thread_ts: Option<String>,
        #[serde(default)]
        assistant_thread: Option<serde_json::Value>,
    },
    #[serde(rename = "assistant_thread_context_changed")]
    AssistantThreadContextChanged {
        #[serde(default)]
        user: Option<String>,
        #[serde(default)]
        ts: Option<String>,
        #[serde(default)]
        channel: Option<String>,
        #[serde(default)]
        thread_ts: Option<String>,
        #[serde(default)]
        assistant_thread: Option<serde_json::Value>,
    },
}

/// Slack slash command payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashCommand {
    pub token: String,
    pub team_id: String,
    pub team_domain: String,
    pub channel_id: String,
    pub channel_name: String,
    pub user_id: String,
    pub user_name: String,
    pub command: String,
    pub text: String,
    pub response_url: String,
    pub trigger_id: String,
}

/// Discriminated union for Slack event payloads
///
/// Used for parsing incoming Slack events which can be either
/// URL verification challenges or actual event callbacks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EventPayload {
    UrlVerification(UrlVerification),
    EventCallback(EventCallback),
}
