use crate::api::agent;
use crate::api::api_keys;
use crate::api::artifacts;
use crate::api::auth;
use crate::api::chart;
use crate::api::data;
use crate::api::database;
use crate::api::file;
use crate::api::middlewares::project::project_middleware;
use crate::api::organization;
use crate::api::project;
use crate::api::run;
use crate::api::secrets;
use crate::api::thread;
use crate::api::user;
use crate::api::workflow;
use crate::auth::middleware::{AuthState, auth_middleware};
use crate::errors::OxyError;
use axum::Router;
use axum::body::Body;
use axum::extract::FromRequestParts;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::middleware;
use axum::routing::delete;
use axum::routing::put;
use axum::routing::{get, post};
use entity::projects;
use sentry::integrations::tower::NewSentryLayer;
use std::future::Future;
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::app;
use super::message;
use super::task;

fn build_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any)
}

fn build_public_routes() -> Router<()> {
    Router::new()
        .route("/auth/config", get(auth::get_config))
        .route("/auth/login", post(auth::login))
        .route("/auth/register", post(auth::register))
        .route("/auth/google", post(auth::google_auth))
        .route("/auth/validate_email", post(auth::validate_email))
}

fn build_global_routes() -> Router<()> {
    Router::new()
        .route("/user", get(user::get_current_user))
        .route("/logout", get(user::logout))
        .route("/github/repositories", get(project::list_repositories))
        .route("/github/branches", get(project::list_branches))
}

fn build_organization_routes() -> Router<()> {
    Router::new()
        .route("/", get(organization::list_organizations))
        .route("/", post(organization::create_organization))
        .route("/{organization_id}/users", get(organization::list_users))
        .route(
            "/{organization_id}/users",
            post(organization::add_user_to_organization),
        )
        .route(
            "/{organization_id}/users",
            put(organization::update_user_role_in_organization),
        )
        .route(
            "/{organization_id}/users/{user_id}",
            delete(organization::remove_user_from_organization),
        )
        .nest(
            "/{organization_id}/projects",
            build_organization_project_routes(),
        )
}

fn build_organization_project_routes() -> Router<()> {
    Router::new()
        .route("/", post(project::create_project))
        .route("/", get(project::list_projects))
        .route("/{project_id}", delete(project::delete_project))
}

fn build_project_routes() -> Router<()> {
    Router::new()
        .route("/details", get(project::get_project))
        .route("/status", get(project::get_project_status))
        .route("/revision-info", get(project::get_revision_info))
        .route("/branches", get(project::get_project_branches))
        .route("/switch-branch", post(project::switch_project_branch))
        .route(
            "/switch-active-branch",
            post(project::switch_project_active_branch),
        )
        .route("/pull-changes", post(project::pull_changes))
        .route("/push-changes", post(project::push_changes))
        .route("/git-token", post(project::change_git_token))
        .nest("/workflows", build_workflow_routes())
        .nest("/threads", build_thread_routes())
        .nest("/agents", build_agent_routes())
        .nest("/api-keys", build_api_key_routes())
        .nest("/files", build_file_routes())
        .nest("/databases", build_database_routes())
        .nest("/secrets", build_secret_routes())
        .nest("/app", build_app_routes())
        .route("/artifacts/{id}", get(artifacts::get_artifact))
        .route("/charts/{file_path}", get(chart::get_chart))
        .route("/logs", get(thread::get_logs))
        .route("/events", get(run::workflow_events))
        .route("/blocks", get(run::get_blocks))
        .route(
            "/runs/{source_id}/{run_index}",
            delete(run::cancel_workflow_run),
        )
        .route(
            "/builder-availability",
            get(agent::check_builder_availability),
        )
        .route("/sql/{pathb64}", post(data::execute_sql))
}

#[derive(Clone)]
pub struct ProjectExtractor(pub projects::Model);

impl<S> FromRequestParts<S> for ProjectExtractor
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let result = parts
            .extensions
            .get::<projects::Model>()
            .cloned()
            .map(ProjectExtractor)
            .ok_or(StatusCode::UNAUTHORIZED);

        async move { result }
    }
}

fn build_workflow_routes() -> Router<()> {
    Router::new()
        .route("/", get(workflow::list))
        .route("/from-query", post(workflow::create_from_query))
        .route("/{pathb64}", get(workflow::get))
        .route("/{pathb64}/run", post(workflow::run_workflow))
        .route("/{pathb64}/logs", get(workflow::get_logs))
        .route("/{pathb64}/runs", get(run::get_workflow_runs))
        .route("/{pathb64}/runs", post(run::create_workflow_run))
}

fn build_thread_routes() -> Router<()> {
    Router::new()
        .route("/", get(thread::get_threads))
        .route("/", post(thread::create_thread))
        .route("/", delete(thread::delete_all_threads))
        .route("/bulk-delete", post(thread::bulk_delete_threads))
        .route("/{id}", get(thread::get_thread))
        .route("/{id}", delete(thread::delete_thread))
        .route("/{id}/task", post(task::ask_task))
        .route("/{id}/workflow", post(workflow::run_workflow_thread))
        .route("/{id}/messages", get(message::get_messages_by_thread))
        .route("/{id}/agent", post(agent::ask_agent))
        .route("/{id}/stop", post(thread::stop_thread))
}

fn build_agent_routes() -> Router<()> {
    Router::new()
        .route("/", get(agent::get_agents))
        .route("/{pathb64}", get(agent::get_agent))
        .route("/{pathb64}/ask", post(agent::ask_agent_preview))
        .route("/{pathb64}/ask_sync", post(agent::ask_agent_sync))
        .route("/{pathb64}/tests/{test_index}", post(agent::run_test))
}

fn build_api_key_routes() -> Router<()> {
    Router::new()
        .route("/", get(api_keys::list_api_keys))
        .route("/", post(api_keys::create_api_key))
        .route("/{id}", get(api_keys::get_api_key))
        .route("/{id}", delete(api_keys::delete_api_key))
}

fn build_file_routes() -> Router<()> {
    Router::new()
        .route("/", get(file::get_file_tree))
        .route("/diff-summary", get(file::get_diff_summary))
        .route("/{pathb64}", get(file::get_file))
        .route("/{pathb64}/from-git", get(file::get_file_from_git))
        .route("/{pathb64}", post(file::save_file))
        .route("/{pathb64}/delete-file", delete(file::delete_file))
        .route("/{pathb64}/delete-folder", delete(file::delete_folder))
        .route("/{pathb64}/rename-file", put(file::rename_file))
        .route("/{pathb64}/rename-folder", put(file::rename_folder))
        .route("/{pathb64}/new-file", post(file::create_file))
        .route("/{pathb64}/new-folder", post(file::create_folder))
}

fn build_database_routes() -> Router<()> {
    Router::new()
        .route("/", get(database::list_databases))
        .route("/sync", post(database::sync_database))
        .route("/build", post(data::build_embeddings))
        .route("/clean", post(database::clean_data))
}

fn build_secret_routes() -> Router<()> {
    Router::new()
        .route("/", get(secrets::list_secrets))
        .route("/", post(secrets::create_secret))
        .route("/bulk", post(secrets::bulk_create_secrets))
        .route("/{id}", get(secrets::get_secret))
        .route("/{id}", put(secrets::update_secret))
        .route("/{id}", delete(secrets::delete_secret))
}

fn build_app_routes() -> Router<()> {
    Router::new()
        .route("/", get(app::list_apps))
        .route("/{pathb64}", get(app::get_app_data))
        .route("/{pathb64}/run", post(app::run_app))
        .route("/{pathb64}/displays", get(app::get_displays))
        .route("/file/{pathb64}", get(app::get_data))
}

fn build_protected_routes() -> Router<()> {
    Router::new()
        .merge(build_global_routes())
        .nest("/organizations", build_organization_routes())
        .nest(
            "/{project_id}",
            build_project_routes().layer(middleware::from_fn(project_middleware)),
        )
}

fn apply_middleware(protected_routes: Router<()>) -> Result<Router<()>, OxyError> {
    let protected_regular_routes = protected_routes.layer(middleware::from_fn_with_state(
        AuthState::built_in(),
        auth_middleware,
    ));

    Ok(protected_regular_routes)
}
pub async fn api_router() -> Result<Router, OxyError> {
    let public_routes = build_public_routes();
    let protected_routes = build_protected_routes();
    let protected_routes = apply_middleware(protected_routes)?;
    let app_routes = public_routes.merge(protected_routes);
    let cors = build_cors_layer();
    Ok(app_routes
        .with_state(())
        .layer(cors)
        .layer(ServiceBuilder::new().layer(NewSentryLayer::<Request<Body>>::new_from_top())))
}

pub async fn openapi_router() -> OpenApiRouter {
    let cors = build_cors_layer();

    OpenApiRouter::new()
        // Agent routes
        .routes(routes!(agent::get_agents))
        .routes(routes!(agent::ask_agent_preview))
        .routes(routes!(agent::ask_agent_sync))
        // API Keys routes
        .routes(routes!(api_keys::create_api_key))
        .routes(routes!(api_keys::list_api_keys))
        .routes(routes!(api_keys::get_api_key))
        .routes(routes!(api_keys::delete_api_key))
        // App routes
        .routes(routes!(app::list_apps))
        // Organization routes
        .routes(routes!(organization::list_organizations))
        .routes(routes!(organization::create_organization))
        // Project routes
        .routes(routes!(project::create_project))
        .routes(routes!(project::get_project))
        .routes(routes!(project::delete_project))
        .routes(routes!(project::get_project_branches))
        // Thread routes
        .routes(routes!(thread::get_threads))
        .routes(routes!(thread::get_thread))
        .routes(routes!(thread::create_thread))
        .routes(routes!(thread::delete_thread))
        .routes(routes!(thread::delete_all_threads))
        .routes(routes!(thread::stop_thread))
        .routes(routes!(thread::bulk_delete_threads))
        .routes(routes!(thread::get_logs))
        // Workflow routes
        .routes(routes!(workflow::list))
        .routes(routes!(workflow::get_logs))
        .routes(routes!(workflow::run_workflow))
        .routes(routes!(workflow::run_workflow_thread))
        .layer(cors)
        .layer(ServiceBuilder::new().layer(NewSentryLayer::<Request<Body>>::new_from_top()))
}
