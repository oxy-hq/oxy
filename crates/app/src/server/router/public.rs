//! Routes that do not require authentication: health probes, auth endpoints,
//! current-user lookup, and Slack-originated webhooks/callbacks.

use axum::Router;
use axum::routing::{get, post};

use crate::api::{auth, billing, healthcheck, user, webhooks};
use crate::server::api::admin::apps::handlers::{get_build_config, get_org_for_project};
use crate::server::api::{custom_apps_debug, projects};

use super::AppState;

pub(super) fn build_public_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(healthcheck::health_check))
        .route("/ready", get(healthcheck::readiness_check))
        .route("/live", get(healthcheck::liveness_check))
        .route("/version", get(healthcheck::version_info))
        // Trusted-publishing OIDC exchange — unauthenticated by construction: the
        // GitHub Actions OIDC JWT in the Authorization header IS the credential.
        // Returns a short-lived, app-scoped publish token. See
        // `custom_apps_publish_oidc`.
        .route(
            "/customer-apps/publish/oidc-exchange",
            post(crate::api::custom_apps_publish_oidc::oidc_exchange_handler),
        )
        .route("/auth/config", get(auth::get_config))
        .route("/auth/session", get(auth::get_session))
        .route("/auth/oauth/state", post(auth::issue_oauth_state))
        .route("/auth/google", post(auth::google_auth))
        .route("/auth/github", post(auth::github_auth))
        .route("/auth/okta", post(auth::okta_auth))
        .route("/auth/magic-link/request", post(auth::request_magic_link))
        .route("/auth/magic-link/verify", post(auth::verify_magic_link))
        .route("/auth/return-to/validate", get(auth::validate_return_to))
        // Dev-only sign-in bypass. Public by necessity (it IS the login), but
        // 404s unless an allow-list resolves — `OXY_DEV_LOGIN_EMAILS` in any
        // build, or the staff roster on a debug build for a loopback caller —
        // and only ever issues a session for an address on it. The three rungs
        // and why they differ: `api::auth::dev_login`.
        .route(
            "/auth/dev-login",
            get(auth::dev_login_get).post(auth::dev_login),
        )
        .route("/user", get(user::get_current_user_public))
        .route("/webhooks/stripe", post(billing::webhook::stripe_webhook))
        .route(
            "/webhooks/toast/orders",
            post(webhooks::toast::toast_order_webhook),
        )
        // Slack-originated traffic. None of these carry a user Authorization
        // header; they're either signature-verified (webhooks) or reached
        // via a browser redirect from slack.com (OAuth callback / magic-link
        // landing page, which uses OptionalAuthenticatedUser to handle
        // logged-in vs logged-out cases itself).
        .route(
            "/slack/oauth/callback",
            get(crate::integrations::slack::oauth::callback::callback),
        )
        // Public: Intuit redirects the bare browser here after consent. The
        // state nonce (single-use, 15-min TTL) is the CSRF/replay guard.
        .route(
            "/quickbooks/oauth/callback",
            get(crate::integrations::quickbooks::oauth::callback::callback),
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
        // Public: resolve a workspace's org slug so `oxy publish --project <id>`
        // can bake the /customer-apps/<org>/<app>/ base path without a hardcoded
        // orgSlug (same rationale as build-config — ids/slugs aren't secrets).
        .route("/org-for-project/{project_id}", get(get_org_for_project))
        // Diagnostic snapshot for admins — what the server sees about
        // this app + its manifest. Same cookie-auth + org-membership
        // gate; human inspection only, shape not guaranteed stable.
        .route(
            "/customer-apps/{org_slug}/{app_slug}/debug",
            get(custom_apps_debug::get_debug),
        )
        // External liveness for ONE published custom app. In the public router
        // because it authenticates inline (session cookie or bearer token) like
        // its neighbours — and the token is mandatory: auth runs before the app
        // lookup so an unauthenticated caller gets 401 whether or not the app
        // exists, which is what keeps this from becoming an app-enumeration
        // oracle. Two forms: slug-explicit (pollable from the admin host, one
        // monitor for every app) and Host-resolved (`/customer-apps/health` on
        // an `<org>--<slug>.customer-apps.<zone>` subdomain, where `/api/*` is
        // the only prefix the subdomain rewrite passes through). NOT a k8s
        // probe — see the module docs.
        .route(
            "/customer-apps/{org_slug}/{app_slug}/health",
            get(crate::server::api::custom_apps_health::get_health),
        )
        .route(
            "/customer-apps/health",
            get(crate::server::api::custom_apps_health::get_health_for_host),
        )
        // Project-scoped proxies for custom-app bundles. Auth is
        // performed inline (session cookie or API key) so these sit
        // in the public router rather than under workspace middleware.
        // Shared gate chain lives in `custom_apps_gates.rs`.
        .route(
            "/projects/{project_id}/query",
            post(projects::query::run_query),
        )
        .route(
            "/projects/{project_id}/semantic-query",
            post(projects::semantic_query::run_semantic_query),
        )
        // Metric-tree analysis ops for bundles — drivers / what-if / RCA /
        // opportunity sizing. Same airlayer analyses as the IDE's workspace
        // `/semantic/metric-tree*` routes, but gated for customer apps and
        // loading the layer from the compile boundary (stateless-fleet safe).
        // SDK exposes via `useMetricTree` / `useSensitivity` / `usePredict` /
        // `useExplain` / `useOpportunity` / `useDistribution` /
        // `useTimeDimensions`.
        .route(
            "/projects/{project_id}/semantic/metric-tree",
            get(projects::metric_tree::get_metric_tree),
        )
        .route(
            "/projects/{project_id}/semantic/metric-tree/{measure_id}/sensitivity",
            get(projects::metric_tree::get_sensitivity),
        )
        .route(
            "/projects/{project_id}/semantic/metric-tree/predict",
            post(projects::metric_tree::post_predict),
        )
        .route(
            "/projects/{project_id}/semantic/metric-tree/explain",
            post(projects::metric_tree::post_explain),
        )
        .route(
            "/projects/{project_id}/semantic/metric-tree/opportunity",
            post(projects::metric_tree::post_opportunity),
        )
        .route(
            "/projects/{project_id}/semantic/metric-tree/time-dimensions",
            get(projects::metric_tree::get_time_dimensions),
        )
        .route(
            "/projects/{project_id}/semantic/metric-tree/distribution",
            post(projects::metric_tree::post_distribution),
        )
        // World-model graph + instances + per-instance driver-tree for
        // bundles — the entity/measure map the IDE's World Model surface
        // renders. SDK exposes via `useWorldModel` / `useWorldModelInstances`
        // / `useMeasureBreakdown`.
        .route(
            "/projects/{project_id}/semantic/world-model",
            get(projects::world_model::get_world_model),
        )
        .route(
            "/projects/{project_id}/semantic/world-model/instances",
            get(projects::world_model::get_world_model_instances),
        )
        .route(
            "/projects/{project_id}/semantic/world-model/measure-breakdown",
            get(projects::world_model::get_measure_breakdown),
        )
        // Shell bootstrap for `@oxy-hq/sdk/shell` chrome inside bundles:
        // workspace/org identity + published-apps list + host-aware
        // product links. Pure Postgres reads behind the same gate chain →
        // FleetOk (deliberately not pinned in role_manifest.rs).
        .route(
            "/projects/{project_id}/shell-context",
            get(crate::server::api::custom_apps_shell_context::get_shell_context),
        )
        // Bundle chat history for the SDK Ask dock. Pure Postgres reads
        // behind the same gate chain → FleetOk (not pinned in
        // role_manifest.rs). `/threads` lists the viewer's threads;
        // `/threads/{id}` rebuilds a transcript for restore.
        .route(
            "/projects/{project_id}/threads",
            get(crate::server::api::custom_apps_threads::list_threads),
        )
        .route(
            "/projects/{project_id}/threads/{thread_id}",
            get(crate::server::api::custom_apps_threads::get_thread_transcript),
        )
        // Bundle-SDK custom event ingest — `useTrackEvent("name", {...})`.
        // Sits next to /query because both share the same gate chain
        // (cookie or bearer auth + org-member/app-admin access check).
        // See `custom_apps_activity::post_event`.
        .route(
            "/customer-apps/{project_id}/events",
            post(crate::server::api::custom_apps_activity::post_event),
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
        // Phase 3 — long-running automations. POST starts, GET polls,
        // POST .../cancel stops in-flight. Persistent state in
        // `customer_app_procedure_runs` survives server restarts.
        .route(
            "/projects/{project_id}/procedures/{procedure_id}/runs",
            post(projects::automation_run::start_automation_run),
        )
        .route(
            "/projects/{project_id}/procedures/runs/{run_id}",
            get(projects::automation_run::poll_automation_run),
        )
        .route(
            "/projects/{project_id}/procedures/runs/{run_id}/cancel",
            post(projects::automation_run::cancel_automation_run),
        )
        // Phase 4 — agent-run SSE stream. Same pipeline as `useAsk`
        // but emitting events in real time. Bundle SDK exposes via
        // `useAgentRun({agentId})`.
        .route(
            "/projects/{project_id}/agents/runs/{run_id}/events",
            get(projects::agent_run_stream::stream_agent_run),
        )
}
