use oxy_shared::errors::OxyError;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;

const DEFAULT_BASE_URL: &str = "https://slack.com/api";

/// Thin wrapper around Slack's Web API. Stateless on bot tokens —
/// every call takes the token as an argument so the same client can
/// serve multiple tenants.
#[derive(Debug, Clone)]
pub struct SlackClient {
    http: Client,
    base_url: String,
}

impl Default for SlackClient {
    fn default() -> Self {
        Self::new()
    }
}

impl SlackClient {
    pub fn new() -> Self {
        Self {
            http: Client::new(),
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }

    /// For tests: point at a mock server.
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            http: Client::new(),
            base_url: base_url.into(),
        }
    }

    /// POST form-encoded body (most Slack endpoints).
    pub async fn post_form(
        &self,
        method: &str,
        bot_token: Option<&str>,
        form: &[(&str, &str)],
    ) -> Result<Value, OxyError> {
        let url = format!("{}/{method}", self.base_url);
        let mut req = self.http.post(&url).form(form);
        if let Some(t) = bot_token {
            req = req.bearer_auth(t);
        }
        let resp: Value = req
            .send()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("slack {method} http: {e}")))?
            .json()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("slack {method} json: {e}")))?;
        check_ok(method, &resp)?;
        Ok(resp)
    }

    /// POST JSON body (chat.postMessage, views.publish, etc.).
    pub async fn post_json(
        &self,
        method: &str,
        bot_token: &str,
        body: &Value,
    ) -> Result<Value, OxyError> {
        let url = format!("{}/{method}", self.base_url);
        let resp: Value = self
            .http
            .post(&url)
            .bearer_auth(bot_token)
            .header("Content-Type", "application/json; charset=utf-8")
            .json(body)
            .send()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("slack {method} http: {e}")))?
            .json()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("slack {method} json: {e}")))?;
        check_ok(method, &resp)?;
        Ok(resp)
    }

    // ---- High-level helpers used by handlers ----

    pub async fn oauth_v2_access(
        &self,
        client_id: &str,
        client_secret: &str,
        code: &str,
        redirect_uri: &str,
    ) -> Result<OauthV2Access, OxyError> {
        let v = self
            .post_form(
                "oauth.v2.access",
                None,
                &[
                    ("client_id", client_id),
                    ("client_secret", client_secret),
                    ("code", code),
                    ("redirect_uri", redirect_uri),
                ],
            )
            .await?;
        serde_json::from_value(v)
            .map_err(|e| OxyError::RuntimeError(format!("oauth.v2.access decode: {e}")))
    }

    pub async fn auth_revoke(&self, bot_token: &str) -> Result<(), OxyError> {
        self.post_form("auth.revoke", Some(bot_token), &[]).await?;
        Ok(())
    }

    pub async fn users_info(&self, bot_token: &str, user_id: &str) -> Result<UserInfo, OxyError> {
        let v = self
            .post_form("users.info", Some(bot_token), &[("user", user_id)])
            .await?;
        serde_json::from_value(v)
            .map_err(|e| OxyError::RuntimeError(format!("users.info decode: {e}")))
    }

    pub async fn chat_post_message(
        &self,
        bot_token: &str,
        channel: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> Result<Value, OxyError> {
        self.chat_post_message_with_blocks(bot_token, channel, text, thread_ts, None)
            .await
    }

    /// Variant of `chat.postMessage` that also attaches a `blocks` array —
    /// used by the streaming fallback path so the workspace footer card /
    /// error alert still render even when chat.startStream is unavailable.
    pub async fn chat_post_message_with_blocks(
        &self,
        bot_token: &str,
        channel: &str,
        text: &str,
        thread_ts: Option<&str>,
        blocks: Option<Value>,
    ) -> Result<Value, OxyError> {
        let mut body = serde_json::json!({ "channel": channel, "text": text });
        if let Some(ts) = thread_ts {
            body["thread_ts"] = Value::String(ts.to_string());
        }
        if let Some(blk) = blocks {
            body["blocks"] = blk;
        }
        self.post_json("chat.postMessage", bot_token, &body).await
    }

    pub async fn chat_post_ephemeral(
        &self,
        bot_token: &str,
        channel: &str,
        user: &str,
        blocks: Value,
        text: &str,
        thread_ts: Option<&str>,
    ) -> Result<Value, OxyError> {
        let mut body = serde_json::json!({
            "channel": channel, "user": user, "text": text, "blocks": blocks,
        });
        if let Some(ts) = thread_ts {
            body["thread_ts"] = serde_json::Value::String(ts.to_string());
        }
        self.post_json("chat.postEphemeral", bot_token, &body).await
    }

    pub async fn views_publish(
        &self,
        bot_token: &str,
        user_id: &str,
        view: Value,
    ) -> Result<Value, OxyError> {
        let body = serde_json::json!({ "user_id": user_id, "view": view });
        self.post_json("views.publish", bot_token, &body).await
    }

    /// Set the native AI-assistant thread status indicator. With
    /// `loading_messages` populated, Slack rotates through the array as
    /// a flashing loading state (cf. Claude's "Generating response…"
    /// pattern). `status` is the static fallback shown alongside
    /// non-rotating clients.
    ///
    /// Pass `status: ""` to clear the indicator.
    /// Slack caps `loading_messages` at 10 entries.
    /// <https://docs.slack.dev/reference/methods/assistant.threads.setStatus>
    pub async fn assistant_threads_set_status(
        &self,
        bot_token: &str,
        channel: &str,
        thread_ts: &str,
        status: &str,
        loading_messages: Option<&[&str]>,
    ) -> Result<(), OxyError> {
        let mut body = serde_json::json!({
            "channel_id": channel,
            "thread_ts": thread_ts,
            "status": status,
        });
        if let Some(msgs) = loading_messages {
            body["loading_messages"] = serde_json::Value::Array(
                msgs.iter()
                    .map(|s| serde_json::Value::String(s.to_string()))
                    .collect(),
            );
        }
        self.post_json("assistant.threads.setStatus", bot_token, &body)
            .await?;
        Ok(())
    }

    /// Set the human-readable title for an Assistant thread. Slack shows
    /// this in the History tab of the bot's split-view chat — without it,
    /// each thread defaults to the first ~40 chars of the user's question
    /// or a generic placeholder. We call this on the first message of a
    /// new thread so users can scan their conversation history at a glance.
    ///
    /// Docs: <https://docs.slack.dev/reference/methods/assistant.threads.setTitle>
    pub async fn assistant_threads_set_title(
        &self,
        bot_token: &str,
        channel: &str,
        thread_ts: &str,
        title: &str,
    ) -> Result<(), OxyError> {
        let body = serde_json::json!({
            "channel_id": channel,
            "thread_ts": thread_ts,
            "title": title,
        });
        self.post_json("assistant.threads.setTitle", bot_token, &body)
            .await?;
        Ok(())
    }

    /// Set clickable starter prompts in an Assistant thread. Each prompt is a
    /// `(title, message)` pair — title is shown as a button label, message is
    /// what gets sent when the user clicks. Up to 4 prompts are displayed.
    ///
    /// Docs: <https://api.slack.com/methods/assistant.threads.setSuggestedPrompts>
    pub async fn assistant_threads_set_suggested_prompts(
        &self,
        bot_token: &str,
        channel: &str,
        thread_ts: &str,
        prompts: &[(&str, &str)],
    ) -> Result<(), OxyError> {
        let prompts_json: Vec<Value> = prompts
            .iter()
            .map(|(title, message)| serde_json::json!({ "title": title, "message": message }))
            .collect();
        let body = serde_json::json!({
            "channel_id": channel,
            "thread_ts": thread_ts,
            "prompts": prompts_json,
        });
        self.post_json("assistant.threads.setSuggestedPrompts", bot_token, &body)
            .await?;
        Ok(())
    }

    /// Open a Socket Mode WebSocket connection. Returns the WSS URL to connect to.
    ///
    /// Delete or replace the original message that triggered a block-action
    /// via Slack's `response_url` (pre-authenticated by Slack — no bot token
    /// needed). Posting `{"delete_original": true}` removes the picker from
    /// the channel after the user has made their selection.
    ///
    /// Failures are logged-and-swallowed; failing to delete a picker is
    /// cosmetically bad but not functionally blocking.
    pub async fn delete_via_response_url(&self, response_url: &str) {
        let result = self
            .http
            .post(response_url)
            .header("Content-Type", "application/json; charset=utf-8")
            .json(&serde_json::json!({ "delete_original": true }))
            .send()
            .await;
        match result {
            Ok(resp) if resp.status().is_success() => {}
            Ok(resp) => {
                tracing::warn!(
                    status = resp.status().as_u16(),
                    "delete_via_response_url: non-success status"
                );
            }
            Err(e) => {
                tracing::warn!("delete_via_response_url: http error: {e}");
            }
        }
    }

    /// Uses the app-level token (`xapp-...`), not the bot token.
    /// Docs: <https://api.slack.com/methods/apps.connections.open>
    pub async fn apps_connections_open(&self, app_level_token: &str) -> Result<String, OxyError> {
        let v = self
            .post_form("apps.connections.open", Some(app_level_token), &[])
            .await?;
        v.get("url")
            .and_then(|u| u.as_str())
            .map(str::to_string)
            .ok_or_else(|| OxyError::RuntimeError("apps.connections.open: missing url".into()))
    }
}

fn check_ok(method: &str, v: &Value) -> Result<(), OxyError> {
    if v.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(())
    } else {
        let err = v.get("error").and_then(Value::as_str).unwrap_or("unknown");
        Err(OxyError::RuntimeError(format!(
            "slack {method} not ok: {err}"
        )))
    }
}

#[derive(Debug, Deserialize)]
pub struct OauthV2Access {
    pub ok: bool,
    pub access_token: String,
    pub token_type: String,
    pub scope: String,
    pub bot_user_id: String,
    pub team: OauthTeam,
    pub enterprise: Option<OauthEnterprise>,
    pub authed_user: OauthAuthedUser,
}
#[derive(Debug, Deserialize)]
pub struct OauthTeam {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Deserialize)]
pub struct OauthEnterprise {
    pub id: String,
    pub name: Option<String>,
}
#[derive(Debug, Deserialize)]
pub struct OauthAuthedUser {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct UserInfo {
    pub ok: bool,
    pub user: Option<UserInfoUser>,
}
#[derive(Debug, Deserialize)]
pub struct UserInfoUser {
    pub id: String,
    pub profile: Option<UserInfoProfile>,
}
#[derive(Debug, Deserialize)]
pub struct UserInfoProfile {
    pub email: Option<String>,
}
