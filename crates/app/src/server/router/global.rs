//! Cloud-only global routes: logout, organization CRUD (with org-scoped
//! sub-routes for members, invitations, onboarding, workspaces, GitHub
//! integration) and the per-user GitHub account/installation routes.
//!
//! Not mounted in local mode — see [`super::protected`].

use axum::Router;
use axum::middleware;
use axum::routing::{delete, get, patch, post};

use crate::api::github::namespaces as github;
use crate::api::github::{account, callback, installations};
use crate::api::middlewares::org_context;
use crate::api::{onboarding, organizations, user, workspaces};

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
        .nest("/user/github", build_user_github_routes())
}

fn build_org_routes() -> Router<AppState> {
    Router::new()
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
