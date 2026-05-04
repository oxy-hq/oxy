//! Routes that do not require authentication: health probes, auth endpoints,
//! current-user lookup, and Slack-originated webhooks/callbacks.

use axum::Router;
use axum::routing::{get, post};

use crate::api::{auth, billing, healthcheck, user};

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
        .route("/webhooks/stripe", post(billing::webhook::stripe_webhook))
        // Slack-originated traffic. None of these carry a user Authorization
        // header; they're either signature-verified (webhooks) or reached
        // via a browser redirect from slack.com (OAuth callback / magic-link
        // landing page, which uses OptionalAuthenticatedUser to handle
        // logged-in vs logged-out cases itself).
        .route(
            "/slack/oauth/callback",
            get(crate::integrations::slack::oauth::callback::callback),
        )
        .route(
            "/slack/events",
            post(crate::integrations::slack::webhooks::events::handle_events),
        )
        .route(
            "/slack/interactivity",
            post(crate::integrations::slack::webhooks::interactivity::handle_interactivity),
        )
        .route(
            "/slack/link",
            get(crate::integrations::slack::linking::landing::landing),
        )
        .route(
            "/slack/link/confirm",
            post(crate::integrations::slack::linking::landing::confirm),
        )
    // NOTE: A public `/slack/charts/{ws}/{stem}.png` endpoint was
    // considered for inline Slack image blocks, but Slack's CDN can't
    // reach `localhost` from a developer machine without an external
    // tunnel (cloudflared/ngrok). Until that infrastructure is in
    // place, the Slack renderer surfaces the on-disk PNG path in a
    // footer context block instead — see
    // `crate::integrations::slack::render::on_chart` and the
    // documentation on `crate::integrations::slack::chart_render`.
}
