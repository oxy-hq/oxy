//! Slack Web API client wrapper
//!
//! Provides methods for interacting with Slack's Web API:
//! - Posting messages
//! - Native streaming (chat.startStream, appendStream, stopStream)
//! - Getting user info

use oxy_shared::errors::OxyError;
use reqwest::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

const SLACK_API_BASE: &str = "https://slack.com/api";

// ============================================================================
// Response types (internal to client)
// ============================================================================

#[derive(Debug, Deserialize)]
struct PostMessageResponse {
    ok: bool,
    ts: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StartStreamResponse {
    ok: bool,
    stream_id: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AppendStreamResponse {
    ok: bool,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StopStreamResponse {
    ok: bool,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SetStatusResponse {
    ok: bool,
    error: Option<String>,
}

/// Slack API response for getting user info (users.info)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfoResponse {
    pub ok: bool,
    pub user: Option<UserInfo>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub name: String,
    pub profile: Option<UserProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub email: Option<String>,
    pub real_name: Option<String>,
}

/// Trait for Slack API responses that have ok/error fields
trait SlackResponse {
    fn is_ok(&self) -> bool;
    fn error_message(&self) -> Option<String>;
}

macro_rules! impl_slack_response {
    ($($t:ty),*) => {
        $(
            impl SlackResponse for $t {
                fn is_ok(&self) -> bool { self.ok }
                fn error_message(&self) -> Option<String> { self.error.clone() }
            }
        )*
    };
}

impl_slack_response!(
    PostMessageResponse,
    StartStreamResponse,
    AppendStreamResponse,
    StopStreamResponse,
    UserInfoResponse,
    SetStatusResponse
);

// ============================================================================
// Client implementation
// ============================================================================

#[derive(Clone)]
pub struct SlackClient {
    client: Client,
}

impl SlackClient {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// Make a POST request to a Slack API endpoint
    async fn post<T: DeserializeOwned + SlackResponse>(
        &self,
        endpoint: &str,
        token: &str,
        payload: Value,
    ) -> Result<T, OxyError> {
        let url = format!("{}/{}", SLACK_API_BASE, endpoint);
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Slack API request failed for {}: {}", endpoint, e);
                OxyError::SlackApiError(format!("Request failed: {}", e))
            })?;

        let response_body: T = response.json().await.map_err(|e| {
            tracing::error!("Failed to parse Slack response for {}: {}", endpoint, e);
            OxyError::SlackApiError(format!("Failed to parse response: {}", e))
        })?;

        if !response_body.is_ok() {
            let error_msg = response_body
                .error_message()
                .unwrap_or_else(|| "Unknown error".to_string());
            tracing::error!("Slack API error for {}: {}", endpoint, error_msg);
            return Err(OxyError::SlackApiError(error_msg));
        }

        Ok(response_body)
    }

    /// Make a GET request to a Slack API endpoint
    async fn get<T: DeserializeOwned + SlackResponse>(
        &self,
        endpoint: &str,
        token: &str,
    ) -> Result<T, OxyError> {
        let url = format!("{}/{}", SLACK_API_BASE, endpoint);
        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Slack API request failed for {}: {}", endpoint, e);
                OxyError::SlackApiError(format!("Request failed: {}", e))
            })?;

        let response_body: T = response.json().await.map_err(|e| {
            tracing::error!("Failed to parse Slack response for {}: {}", endpoint, e);
            OxyError::SlackApiError(format!("Failed to parse response: {}", e))
        })?;

        if !response_body.is_ok() {
            let error_msg = response_body
                .error_message()
                .unwrap_or_else(|| "Unknown error".to_string());
            tracing::error!("Slack API error for {}: {}", endpoint, error_msg);
            return Err(OxyError::SlackApiError(error_msg));
        }

        Ok(response_body)
    }

    /// Post a message to a Slack channel
    ///
    /// # Returns
    /// Message timestamp (ts) of the posted message
    pub async fn post_message(
        &self,
        token: &str,
        channel_id: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> Result<String, OxyError> {
        let mut payload = json!({
            "channel": channel_id,
            "text": text,
        });

        if let Some(ts) = thread_ts {
            payload["thread_ts"] = json!(ts);
        }

        let response: PostMessageResponse = self.post("chat.postMessage", token, payload).await?;

        response
            .ts
            .ok_or_else(|| OxyError::SlackApiError("No ts in response".to_string()))
    }

    /// Start a streaming message
    ///
    /// # Returns
    /// Stream ID to use for appending and stopping the stream
    pub async fn start_stream(
        &self,
        token: &str,
        channel_id: &str,
        thread_ts: Option<&str>,
        team_id: Option<&str>,
    ) -> Result<String, OxyError> {
        let mut payload = json!({
            "channel": channel_id,
        });

        if let Some(ts) = thread_ts {
            payload["thread_ts"] = json!(ts);
        }

        if let Some(tid) = team_id {
            payload["recipient_team_id"] = json!(tid);
        }

        let response: StartStreamResponse = self.post("chat.startStream", token, payload).await?;

        response.stream_id.ok_or_else(|| {
            tracing::error!("Slack streaming API returned ok=true but no stream_id");
            OxyError::SlackApiError(
                "No stream_id in response - streaming API may not be available for this workspace"
                    .to_string(),
            )
        })
    }

    /// Append text to a streaming message
    pub async fn append_stream(
        &self,
        token: &str,
        stream_id: &str,
        text: &str,
    ) -> Result<(), OxyError> {
        let payload = json!({
            "stream_id": stream_id,
            "text": text,
        });

        let _: AppendStreamResponse = self.post("chat.appendStream", token, payload).await?;
        Ok(())
    }

    /// Stop a streaming message
    pub async fn stop_stream(&self, token: &str, stream_id: &str) -> Result<(), OxyError> {
        let payload = json!({
            "stream_id": stream_id,
        });

        let _: StopStreamResponse = self.post("chat.stopStream", token, payload).await?;
        Ok(())
    }

    /// Get user information from Slack
    ///
    /// # Returns
    /// User information including email if available
    pub async fn get_user_info(
        &self,
        token: &str,
        user_id: &str,
    ) -> Result<UserInfoResponse, OxyError> {
        self.get(&format!("users.info?user={}", user_id), token)
            .await
    }

    /// Set assistant thread status (for AI/Agent apps)
    pub async fn set_thread_status(
        &self,
        token: &str,
        channel_id: &str,
        thread_ts: &str,
        status: &str,
    ) -> Result<(), OxyError> {
        let payload = json!({
            "channel_id": channel_id,
            "thread_ts": thread_ts,
            "status": status,
        });

        let _: SetStatusResponse = self
            .post("assistant.threads.setStatus", token, payload)
            .await?;
        Ok(())
    }
}

impl Default for SlackClient {
    fn default() -> Self {
        Self::new()
    }
}
