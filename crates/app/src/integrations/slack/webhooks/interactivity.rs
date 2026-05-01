use crate::integrations::slack::config::SlackConfig;
use crate::integrations::slack::signature::verify_request;
use crate::integrations::slack::types::InteractivityPayload;
use crate::integrations::slack::webhooks::handlers;
use axum::body::Bytes;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use chrono::Utc;

pub async fn handle_interactivity(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let cfg = match SlackConfig::cached().as_runtime() {
        Some(c) => c,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "slack disabled").into_response(),
    };

    let ts = headers
        .get("x-slack-request-timestamp")
        .and_then(|v| v.to_str().ok());
    let sig = headers
        .get("x-slack-signature")
        .and_then(|v| v.to_str().ok());
    if let Err(e) = verify_request(&cfg.signing_secret, ts, sig, &body, Utc::now().timestamp()) {
        tracing::warn!("interactivity signature verify: {e}");
        return (StatusCode::UNAUTHORIZED, "invalid_signature").into_response();
    }

    let form: std::collections::HashMap<String, String> = match serde_urlencoded::from_bytes(&body)
    {
        Ok(f) => f,
        Err(_) => return (StatusCode::BAD_REQUEST, "bad form").into_response(),
    };
    let Some(raw) = form.get("payload") else {
        return (StatusCode::BAD_REQUEST, "missing payload").into_response();
    };
    let payload: InteractivityPayload = match serde_json::from_str(raw) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("interactivity decode: {e}");
            return (StatusCode::BAD_REQUEST, "bad payload").into_response();
        }
    };

    dispatch_interactivity(payload).await;
    (StatusCode::OK, "").into_response()
}

pub(crate) async fn dispatch_interactivity(payload: InteractivityPayload) {
    tracing::info!(
        actions = ?payload.actions.iter().map(|a| &a.action_id).collect::<Vec<_>>(),
        "interactivity payload received"
    );

    for action in &payload.actions {
        let outcome = match action.action_id.as_str() {
            "slack_pick_workspace" => handlers::pick_workspace::handle(&payload, action).await,
            "slack_submit_workspace_picker" => {
                handlers::submit_workspace_picker::handle(&payload, action).await
            }
            "slack_reopen_picker" => handlers::reopen_picker::handle(&payload, action).await,
            "slack_make_default" => handlers::make_default::handle(&payload, action).await,
            "slack_home_disconnect" => handlers::home_disconnect::handle(&payload).await,
            "slack_home_save_defaults" => handlers::home_save_defaults::handle(&payload).await,
            "slack_view_sql_artifacts" => {
                handlers::view_sql_artifacts::handle(&payload, action).await
            }
            // View state / URL buttons — no server-side action needed.
            "slack_home_pick_workspace"
            | "slack_home_pick_agent"
            | "slack_home_connect"
            | "slack_home_open_oxy"
            | "slack_view_thread"
            | "slack_connect_oxy" => Ok(()),
            other => {
                tracing::info!("unhandled interactivity action: {other}");
                Ok(())
            }
        };
        if let Err(e) = outcome {
            tracing::error!("interactivity action {} failed: {e}", action.action_id);
        }
    }
}
