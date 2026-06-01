//! Routes that do not require authentication: health probes, auth endpoints,
//! current-user lookup, and Slack-originated webhooks/callbacks.

use axum::Router;
use axum::routing::{get, post};

use crate::api::{auth, billing, healthcheck, user};
use crate::server::api::admin::apps::handlers::get_build_config;
use crate::server::api::{customer_apps_debug, projects};

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
        .route("/auth/return-to/validate", get(auth::validate_return_to))
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
        // Public build-config endpoint — no auth required. CI reads this at
        // build time so no per-app env vars need to live in customer-apps repo.
        .route(
            "/apps/{org_slug}/{app_slug}/build-config",
            get(get_build_config),
        )
        // Diagnostic snapshot for admins — what the server sees about
        // this app + its manifest. Same cookie-auth + org-membership
        // gate; human inspection only, shape not guaranteed stable.
        .route(
            "/customer-apps/{org_slug}/{app_slug}/debug",
            get(customer_apps_debug::get_debug),
        )
        // Project-scoped proxies for customer-app bundles. Auth is
        // performed inline (session cookie or API key) so these sit
        // in the public router rather than under workspace middleware.
        // Shared gate chain lives in `customer_apps_gates.rs`.
        .route(
            "/projects/{project_id}/query",
            post(projects::query::run_query),
        )
        .route(
            "/projects/{project_id}/semantic-query",
            post(projects::semantic_query::run_semantic_query),
        )
        // Phase 2 — one-shot ask. POST starts a run, GET polls for state.
        // Bundle SDK exposes both behind `useAsk({agentId})`.
        .route(
            "/projects/{project_id}/agents/{agent_id}/asks",
            post(projects::agent_ask::start_ask),
        )
        // No GET poll endpoint — the SDK consolidated on
        // `useAgentRun` (SSE) for chat. Bundles that want one-shot
        // ask UX use the same SSE stream and bail out on the first
        // terminal event; bundles that don't want streaming use
        // `useProcedureRun` for their batch case. Removing the
        // poll endpoint keeps the surface small and removes a
        // second source of truth for run state.
        .route(
            "/projects/{project_id}/agents/asks/{run_id}/cancel",
            post(projects::agent_ask::cancel_ask),
        )
        // Phase 3 — long-running procedures. POST starts, GET polls,
        // POST .../cancel stops in-flight. Persistent state in
        // `customer_app_procedure_runs` survives server restarts.
        .route(
            "/projects/{project_id}/procedures/{procedure_id}/runs",
            post(projects::procedure_run::start_procedure_run),
        )
        .route(
            "/projects/{project_id}/procedures/runs/{run_id}",
            get(projects::procedure_run::poll_procedure_run),
        )
        .route(
            "/projects/{project_id}/procedures/runs/{run_id}/cancel",
            post(projects::procedure_run::cancel_procedure_run),
        )
        // Phase 4 — agent-run SSE stream. Same pipeline as `useAsk`
        // but emitting events in real time. Bundle SDK exposes via
        // `useAgentRun({agentId})`.
        .route(
            "/projects/{project_id}/agents/runs/{run_id}/events",
            get(projects::agent_run_stream::stream_agent_run),
        )
}
