use std::future::Future;

use crate::integrations::slack::client::SlackClient;
use crate::integrations::slack::config::SlackConfig;
use crate::integrations::slack::error::SlackError;
use crate::integrations::slack::signature::verify_request;
use crate::integrations::slack::types::{Event, EventCallback, EventPayload};
use crate::integrations::slack::webhooks::tenant_resolver::{ResolvedTenant, resolve};
use axum::Json;
use axum::body::Bytes;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use chrono::Utc;
use serde_json::json;

pub async fn handle_events(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let cfg = match SlackConfig::cached().as_runtime() {
        Some(c) => c,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": "slack_disabled"})),
            )
                .into_response();
        }
    };

    let ts = headers
        .get("x-slack-request-timestamp")
        .and_then(|v| v.to_str().ok());
    let sig = headers
        .get("x-slack-signature")
        .and_then(|v| v.to_str().ok());
    if let Err(e) = verify_request(&cfg.signing_secret, ts, sig, &body, Utc::now().timestamp()) {
        tracing::warn!("slack signature verify: {e}");
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "invalid_signature"})),
        )
            .into_response();
    }

    let payload: EventPayload = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("slack event decode: {e}");
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "invalid_json"})),
            )
                .into_response();
        }
    };

    match payload {
        EventPayload::UrlVerification(v) => {
            (StatusCode::OK, Json(json!({"challenge": v.challenge}))).into_response()
        }
        EventPayload::EventCallback(cb) => {
            let team = cb.team_id.clone();
            tokio::spawn(async move {
                if let Err(e) = dispatch(cb).await {
                    tracing::error!(team = team, "slack event dispatch: {e}");
                }
            });
            (StatusCode::OK, Json(json!({"ok": true}))).into_response()
        }
    }
}

pub(crate) async fn dispatch(cb: EventCallback) -> Result<(), oxy_shared::errors::OxyError> {
    // Deduplicate: Slack delivers events at-least-once; skip retries.
    if let Some(event_id) = cb.event_id.as_deref() {
        match crate::integrations::slack::services::seen_events::SeenEventsService::claim(event_id)
            .await
        {
            Ok(true) => {} // first time, proceed
            Ok(false) => {
                tracing::info!(event_id, "duplicate slack event, skipping");
                return Ok(());
            }
            Err(e) => {
                // DB failure: proceed anyway to avoid dropping events on transient errors.
                tracing::warn!(event_id, "event dedup claim failed, proceeding: {e}");
            }
        }
    }

    let Some(ResolvedTenant {
        installation,
        bot_token,
    }) = resolve(&cb.team_id).await?
    else {
        tracing::info!(team = cb.team_id, "drop: no active install for team");
        return Ok(());
    };

    let client = SlackClient::new();

    match cb.event {
        Event::AppMention {
            user,
            text,
            ts,
            channel,
            thread_ts,
        } => {
            let effective_ts = thread_ts.clone().unwrap_or_else(|| ts.clone());
            // Clone context strings needed by dispatch_user_event before
            // moving them into the handler future.
            let bt = bot_token.clone();
            let ch = channel.clone();
            let u = user.clone();
            dispatch_user_event(
                &client,
                &bt,
                &ch,
                &effective_ts,
                &u,
                crate::integrations::slack::events::app_mention::handle(
                    installation,
                    bot_token,
                    user,
                    text,
                    channel,
                    thread_ts,
                    ts,
                ),
            )
            .await;
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
            let effective_ts = thread_ts.clone().unwrap_or_else(|| ts.clone());
            let slack_user = user.clone().unwrap_or_default();
            let bt = bot_token.clone();
            let ch = channel.clone();
            dispatch_user_event(
                &client,
                &bt,
                &ch,
                &effective_ts,
                &slack_user,
                crate::integrations::slack::events::message::handle(
                    crate::integrations::slack::events::message::MessageArgs {
                        installation,
                        bot_token,
                        user,
                        text,
                        ts,
                        channel,
                        thread_ts,
                        channel_type,
                        subtype,
                        bot_id,
                    },
                ),
            )
            .await;
        }
        Event::AssistantThreadStarted {
            channel, thread_ts, ..
        } => {
            crate::integrations::slack::events::assistant_thread::started(
                installation,
                bot_token,
                channel,
                thread_ts,
            )
            .await?;
        }
        Event::AssistantThreadContextChanged { .. } => {
            crate::integrations::slack::events::assistant_thread::context_changed(installation)
                .await?;
        }
        Event::AppHomeOpened { user, tab, .. } if tab == "home" => {
            crate::integrations::slack::events::app_home::handle(installation, bot_token, user)
                .await?;
        }
        Event::AppHomeOpened { .. } => {} // non-home tab, ignore
        Event::AppUninstalled { .. } | Event::TokensRevoked { .. } => {
            crate::integrations::slack::events::uninstall::revoke(installation).await?;
        }
    }
    Ok(())
}

/// Route a user-triggered event and post any resulting error back to Slack.
///
/// User-facing errors (`NotAuthenticated`, `NotOrgMember`, etc.) are sent as
/// ephemerals — only the requesting user sees them. Infrastructure errors
/// (`Internal`) post in-thread so the team knows something went wrong.
///
/// This function never returns an error itself: errors in the error-surfacing
/// path are logged and swallowed so the dispatch loop always 200s Slack.
async fn dispatch_user_event(
    client: &SlackClient,
    bot_token: &str,
    channel: &str,
    thread_ts: &str,
    slack_user_id: &str,
    fut: impl Future<Output = Result<(), SlackError>>,
) {
    let Err(e) = fut.await else {
        return;
    };
    tracing::warn!(
        channel,
        thread_ts,
        slack_user_id,
        "slack user event error: {e}"
    );
    let blocks = e.to_blocks();
    let text = e.fallback_text();
    if e.is_ephemeral() {
        if let Err(post_err) = client
            .chat_post_ephemeral(
                bot_token,
                channel,
                slack_user_id,
                blocks,
                text,
                Some(thread_ts),
            )
            .await
        {
            tracing::warn!("dispatch_user_event: ephemeral post failed: {post_err}");
        }
    } else {
        if let Err(post_err) = client
            .chat_post_message_with_blocks(bot_token, channel, text, Some(thread_ts), Some(blocks))
            .await
        {
            tracing::warn!("dispatch_user_event: message post failed: {post_err}");
        }
    }
}
