//! Cloud-only global routes: logout, organization CRUD (with org-scoped
//! sub-routes for members, invitations, onboarding, workspaces, GitHub
//! integration) and the per-user GitHub account/installation routes.
//!
//! Not mounted in local mode — see [`super::protected`].

use axum::Router;
use axum::middleware;
use axum::routing::{delete, get, patch, post};

use crate::api::billing;
use crate::api::github::namespaces as github;
use crate::api::github::{account, callback, installations};
use crate::api::middlewares::{org_context, oxy_owner_guard, subscription_guard};
use crate::api::{admin, onboarding, organizations, user, workspaces};

use super::AppState;

pub(super) fn build_global_routes() -> Router<AppState> {
    Router::new()
        .route("/logout", get(user::logout))
        .route("/orgs", post(organizations::create_org))
        .route("/orgs", get(organizations::list_orgs))
        .route("/invitations/mine", get(organizations::list_my_invitations))
        .route(
            "/invitations/{token}/accept",
            post(organizations::accept_invitation),
        )
        .nest("/orgs/{org_id}", build_org_routes())
        .nest(
            "/admin",
            admin::router().layer(middleware::from_fn(
                oxy_owner_guard::oxy_owner_guard_middleware,
            )),
        )
        .nest("/user/github", build_user_github_routes())
    // NOTE: Slack webhook + OAuth-callback + magic-link routes are NOT
    // registered here. They must live in `public.rs` because the routes
    // in this file sit inside the auth middleware layer, and:
    //   - Slack's webhook POSTs (/slack/events, /slack/interactivity) have
    //     no user auth — they're signature-verified inside the handler.
    //   - The OAuth callback is reached via a browser redirect from
    //     slack.com — the browser carries no Authorization header.
    //   - The magic-link landing handles auth state itself via
    //     OptionalAuthenticatedUser; bouncing through the auth middleware
    //     first would 401 every unauth'd user.
}

fn build_org_routes() -> Router<AppState> {
    // Two sub-routers under the same `org_middleware`:
    //   - `gated` covers everything that requires an active subscription
    //     (members, invitations, onboarding, workspace CRUD, github)
    //   - `bypass` covers `/billing/*`, the only org-scoped tree the user
    //     can hit while paywalled (so they can subscribe / open the portal)
    let gated = Router::new()
        .route("/", get(organizations::get_org))
        .route("/", patch(organizations::update_org))
        .route("/", delete(organizations::delete_org))
        .route("/members", get(organizations::list_members))
        .route(
            "/members/{user_id}",
            patch(organizations::update_member_role),
        )
        .route("/members/{user_id}", delete(organizations::remove_member))
        .route("/invitations", post(organizations::create_invitation))
        .route(
            "/invitations/bulk",
            post(organizations::create_bulk_invitations),
        )
        .route("/invitations", get(organizations::list_invitations))
        .route(
            "/invitations/{invitation_id}",
            delete(organizations::revoke_invitation),
        )
        .route("/onboarding/demo", post(onboarding::setup_demo))
        .route("/onboarding/new", post(onboarding::setup_new))
        .route("/onboarding/github", post(onboarding::setup_github))
        .route("/workspaces", get(workspaces::list_workspaces))
        .route("/workspaces/{id}", delete(workspaces::delete_workspace))
        .route(
            "/workspaces/{id}/rename",
            patch(workspaces::rename_workspace),
        )
        .nest("/github", build_github_routes())
        // Slack installation management (requires org membership, admin check inside handlers)
        .route(
            "/slack/install",
            post(crate::integrations::slack::oauth::install::start_install),
        )
        .route(
            "/slack/installation",
            get(crate::integrations::slack::oauth::status::get_status)
                .delete(crate::integrations::slack::oauth::disconnect::disconnect),
        )
        .layer(middleware::from_fn(
            subscription_guard::subscription_guard_middleware,
        ));

    let bypass = Router::new().nest("/billing", billing::router());

    gated
        .merge(bypass)
        .layer(middleware::from_fn(org_context::org_middleware))
}

fn build_github_routes() -> Router<AppState> {
    Router::new()
        .route("/repositories", get(github::list_repositories))
        .route("/branches", get(github::list_branches))
        .route("/namespaces", get(github::list_git_namespaces))
        .route("/namespaces/pat", post(github::create_pat_namespace))
        .route(
            "/namespaces/installation",
            post(github::create_installation_namespace),
        )
        .route("/namespaces/{id}", delete(github::delete_git_namespace))
}

fn build_user_github_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/account",
            get(account::get_account).delete(account::delete_account),
        )
        .route("/account/oauth-url", get(account::get_oauth_url))
        .route("/installations", get(installations::list_installations))
        .route(
            "/installations/new-url",
            get(installations::get_new_installation_url),
        )
        .route("/callback", post(callback::callback))
}
