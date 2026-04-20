//! Routes that do not require authentication: health probes, auth endpoints,
//! current-user lookup, and Slack webhooks.

use axum::Router;
use axum::routing::{get, post};

use crate::api::{auth, healthcheck, slack, user};

use super::AppState;

pub(super) fn build_public_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(healthcheck::health_check))
        .route("/ready", get(healthcheck::readiness_check))
        .route("/live", get(healthcheck::liveness_check))
        .route("/version", get(healthcheck::version_info))
        .route("/auth/config", get(auth::get_config))
        .route("/auth/oauth/state", post(auth::issue_oauth_state))
        .route("/auth/google", post(auth::google_auth))
        .route("/auth/github", post(auth::github_auth))
        .route("/auth/okta", post(auth::okta_auth))
        .route("/auth/magic-link/request", post(auth::request_magic_link))
        .route("/auth/magic-link/verify", post(auth::verify_magic_link))
        .route("/user", get(user::get_current_user_public))
        .merge(build_slack_routes())
}

// TODO: Right now, all incoming Slack requests default to the empty UUID workspace_id,
//       but we can bind a workspace to a Slack channel via `/oxy bind <workspace_id> <agent_id>`.
//       In the future, to support Slack integration for cloud deployments, we may need to scope
//       Slack routes to a specific workspace.
fn build_slack_routes() -> Router<AppState> {
    // Slack routes are always registered. Configuration (signing secret, bot token)
    // is loaded from config.yml per-request in the handlers.
    Router::new()
        .route("/slack/events", post(slack::handle_events))
        .route("/slack/commands", post(slack::handle_commands))
}
