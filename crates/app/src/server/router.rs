use crate::api::agent;
use crate::api::api_keys;
use crate::api::artifacts;
use crate::api::auth;
use crate::api::chart;
use crate::api::data;
use crate::api::database;
use crate::api::execution_analytics;
use crate::api::exported_chart;
use crate::api::file;
use crate::api::github;
use crate::api::healthcheck;
use crate::api::integration;
use crate::api::metrics;
use crate::api::middlewares::project::project_middleware;
use crate::api::middlewares::timeout::timeout_middleware;
use crate::api::project;
use crate::api::result_files;
use crate::api::run;
use crate::api::secrets;
use crate::api::semantic;
use crate::api::slack;
use crate::api::test_file;
use crate::api::test_project_run;
use crate::api::test_run;
use crate::api::thread;
use crate::api::traces;
use crate::api::user;
use crate::api::workflow;
use crate::api::workspace;
use crate::server::providers;
use axum::Router;
use axum::body::Body;
use axum::extract::FromRequestParts;
use axum::extract::State;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::middleware;
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::delete;
use axum::routing::put;
use axum::routing::{get, post};
use entity::projects;
use entity::users::UserRole;
use oxy::adapters::project::manager::ProjectManager;
use oxy_auth::middleware::{AuthState, auth_middleware, internal_auth_middleware};
use oxy_auth::types::AuthenticatedUser;
use oxy_shared::errors::OxyError;
use sentry::integrations::tower::NewSentryLayer;
use std::future::Future;
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};
use tower_http::timeout::TimeoutLayer;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::api::{app, message, task};

use agentic_http::{
    AgenticState, app_builder_router, cleanup_stale_runs, router as agentic_router,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub cloud: bool,
    pub enterprise: bool,
    pub internal: bool,
    pub readonly: bool,
    pub backend: std::sync::Arc<providers::ProjectBackend>,
}

fn build_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_private_network(true)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any)
}

fn build_public_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(healthcheck::health_check))
        .route("/ready", get(healthcheck::readiness_check))
        .route("/live", get(healthcheck::liveness_check))
        .route("/version", get(healthcheck::version_info))
        .route("/auth/config", get(auth::get_config))
        .route("/auth/google", post(auth::google_auth))
        .route("/auth/okta", post(auth::okta_auth))
        .route("/auth/magic-link/request", post(auth::request_magic_link))
        .route("/auth/magic-link/verify", post(auth::verify_magic_link))
        .route("/user", get(user::get_current_user_public))
        .route("/webhooks/github", post(github::github_webhook))
        .merge(build_slack_routes())
}

// TODO: Right now, all incoming Slack requests default to the empty UUID project_id,
//       but we can bind a project to a Slack channel via `/oxy bind <project_id> <agent_id>`.
//       In the future, to support Slack integration for cloud deployments, we may need to scope
//       Slack routes to a specific workspace and/or project.
fn build_slack_routes() -> Router<AppState> {
    // Slack routes are always registered. Configuration (signing secret, bot token)
    // is loaded from config.yml per-request in the handlers.
    Router::new()
        .route("/slack/events", post(slack::handle_events))
        .route("/slack/commands", post(slack::handle_commands))
}

fn build_global_routes() -> Router<AppState> {
    Router::new()
        .route("/logout", get(user::logout))
        .route("/auth/invite", post(auth::invite_user))
        .route("/users/batch", post(user::batch_get_users))
        .route("/github/repositories", get(github::list_repositories))
        .route("/github/branches", get(github::list_branches))
        .route("/github/namespaces", get(github::list_git_namespaces))
        .route("/github/install-app-url", get(github::gen_install_app_url))
        .route("/github/namespaces", post(github::create_git_namespace))
}

fn build_workspace_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(workspace::list_workspaces))
        .route("/", post(workspace::create_workspace))
        .route("/{workspace_id}/users", get(workspace::list_users))
        .route(
            "/{workspace_id}/users",
            post(workspace::add_user_to_workspace),
        )
        .route(
            "/{workspace_id}/users",
            put(workspace::update_user_role_in_workspace),
        )
        .route(
            "/{workspace_id}/users/{user_id}",
            delete(workspace::remove_user_from_workspace),
        )
}

fn build_project_routes(app_state: AppState, agentic_state: Arc<AgenticState>) -> Router<AppState> {
    Router::new()
        .route("/details", get(project::get_project))
        .route("/status", get(project::get_project_status))
        .route("/revision-info", get(project::get_revision_info))
        .route("/branches", get(project::get_project_branches))
        .route("/branches/{branch_name}", delete(project::delete_branch))
        .route("/switch-branch", post(project::switch_project_branch))
        .route(
            "/switch-active-branch",
            post(project::switch_project_active_branch),
        )
        .route("/pull-changes", post(project::pull_changes))
        .route("/push-changes", post(project::push_changes))
        .route("/abort-rebase", post(project::abort_rebase))
        .route("/continue-rebase", post(project::continue_rebase))
        .route(
            "/resolve-conflict-file",
            post(project::resolve_conflict_file),
        )
        .route(
            "/unresolve-conflict-file",
            post(project::unresolve_conflict_file),
        )
        .route(
            "/resolve-conflict-with-content",
            post(project::resolve_conflict_with_content),
        )
        .route("/force-push", post(project::force_push_branch))
        .route("/recent-commits", get(project::get_recent_commits))
        .route("/reset-to-commit", post(project::reset_to_commit))
        .route("/create-repo", post(project::create_repo_from_project))
        .nest("/workflows", build_workflow_routes())
        .nest("/automations", build_automation_routes())
        .nest("/threads", build_thread_routes())
        .nest("/agents", build_agent_routes())
        .nest("/api-keys", build_api_key_routes())
        .nest("/files", build_file_routes())
        .nest("/databases", build_database_routes())
        .nest("/integrations", build_integration_routes())
        .nest("/secrets", build_secret_routes(app_state))
        .nest("/tests", build_test_file_routes())
        .nest("/apps", build_app_routes())
        .nest("/traces", traces::traces_routes())
        .nest("/metrics", metrics::metrics_routes())
        .nest(
            "/execution-analytics",
            execution_analytics::execution_analytics_routes(),
        )
        .route("/artifacts/{id}", get(artifacts::get_artifact))
        .route("/charts/{file_path}", get(chart::get_chart))
        .route(
            "/exported-charts/{file_name}",
            get(exported_chart::get_exported_chart),
        )
        .route("/logs", get(thread::get_logs))
        .route("/events", get(run::workflow_events))
        .route("/events/lookup", get(task::agentic_events))
        .route("/events/sync", get(run::workflow_events_sync))
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
        .route("/sql/query", post(data::execute_sql_query))
        .route("/semantic", post(semantic::execute_semantic_query))
        .route("/semantic/compile", post(semantic::compile_semantic_query))
        .route(
            "/semantic/topic/{file_path_b64}",
            get(semantic::get_topic_details),
        )
        .route(
            "/semantic/view/{file_path_b64}",
            get(semantic::get_view_details),
        )
        .route(
            "/results/files/{file_id}",
            get(result_files::get_result_file),
        )
        .route(
            "/results/files/{file_id}",
            delete(result_files::delete_result_file),
        )
        .nest("/analytics", agentic_router(agentic_state.clone()))
        .nest("/app-builder", app_builder_router(agentic_state))
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

fn build_workflow_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(workflow::list))
        .route("/from-query", post(workflow::create_from_query))
        .route("/runs/bulk-delete", post(run::bulk_delete_workflow_runs))
        .route("/{pathb64}", get(workflow::get))
        .route("/{pathb64}/run", post(workflow::run_workflow))
        .route("/{pathb64}/run-sync", post(workflow::run_workflow_sync))
        .route("/{pathb64}/logs", get(workflow::get_logs))
        .route("/{pathb64}/runs", get(run::get_workflow_runs))
        .route("/{pathb64}/runs", post(run::create_workflow_run))
        .route(
            "/{pathb64}/runs/{run_id}",
            get(workflow::get_workflow_run).delete(run::delete_workflow_run),
        )
}

fn build_automation_routes() -> Router<AppState> {
    Router::new().route("/save", post(workflow::save_automation))
}

fn build_thread_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(thread::get_threads))
        .route("/", post(thread::create_thread))
        .route("/", delete(thread::delete_all_threads))
        .route("/bulk-delete", post(thread::bulk_delete_threads))
        .route("/{id}", get(thread::get_thread))
        .route("/{id}", delete(thread::delete_thread))
        .route("/{id}/task", post(task::ask_task))
        .route("/{id}/agentic", post(task::ask_agentic))
        .route("/{id}/workflow", post(workflow::run_workflow_thread))
        .route(
            "/{id}/workflow-sync",
            post(workflow::run_workflow_thread_sync),
        )
        .route("/{id}/messages", get(message::get_messages_by_thread))
        .route("/{id}/agent", post(agent::ask_agent))
        .route("/{id}/stop", post(thread::stop_thread))
}

fn build_agent_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(agent::get_agents))
        .route("/{pathb64}", get(agent::get_agent))
        .route("/{pathb64}/ask", post(agent::ask_agent_preview))
        .route("/{pathb64}/ask-sync", post(agent::ask_agent_sync))
        .route("/{pathb64}/tests/{test_index}", post(agent::run_test))
}

fn build_api_key_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(api_keys::list_api_keys))
        .route("/", post(api_keys::create_api_key))
        .route("/{id}", get(api_keys::get_api_key))
        .route("/{id}", delete(api_keys::delete_api_key))
}

fn build_file_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(file::get_file_tree))
        .route("/diff-summary", get(file::get_diff_summary))
        .route("/{pathb64}", get(file::get_file))
        .route("/{pathb64}/from-git", get(file::get_file_from_git))
        .route("/{pathb64}/revert", post(file::revert_file))
        .route("/{pathb64}", post(file::save_file))
        .route("/{pathb64}/delete-file", delete(file::delete_file))
        .route("/{pathb64}/delete-folder", delete(file::delete_folder))
        .route("/{pathb64}/rename-file", put(file::rename_file))
        .route("/{pathb64}/rename-folder", put(file::rename_folder))
        .route("/{pathb64}/new-file", post(file::create_file))
        .route("/{pathb64}/new-folder", post(file::create_folder))
}

fn build_database_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(database::list_databases))
        .route("/", post(database::create_database_config))
        .route("/test-connection", post(database::test_database_connection))
        .route("/sync", post(database::sync_database))
        .route("/build", post(data::build_embeddings))
        .route("/clean", post(database::clean_data))
}

fn build_integration_routes() -> Router<AppState> {
    Router::new()
        .route("/looker", get(integration::list_looker_integrations))
        .route("/looker/query", post(integration::execute_looker_query))
        .route("/looker/query/sql", post(integration::compile_looker_query))
}

/// Middleware that gates secrets routes to admin users only.
///
/// - Cloud mode: requires `UserRole::Admin` DB role.
/// - Local mode: checks `config.admins` list; auto-grants access when the list is empty
///   (permissive default for single-user local installs). The built-in local guest
///   (`<local-user@example.com>`) always passes.
async fn secrets_access_middleware(
    State(app_state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let user = request
        .extensions()
        .get::<AuthenticatedUser>()
        .cloned()
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if app_state.cloud {
        if user.role != UserRole::Admin {
            tracing::warn!(
                "Non-admin user {} attempted to access secrets (cloud mode)",
                user.email
            );
            return Err(StatusCode::FORBIDDEN);
        }
    } else {
        let is_admin = is_local_admin(&user.email, &request);
        if !is_admin {
            tracing::warn!(
                "Non-admin user {} attempted to access secrets (local mode)",
                user.email
            );
            return Err(StatusCode::FORBIDDEN);
        }
    }

    Ok(next.run(request).await)
}

/// Checks whether a user email is considered admin in local (non-cloud) mode.
///
/// Logic:
/// 1. The built-in local guest (`<local-user@example.com>`) is always admin.
/// 2. If the ProjectManager is loaded, consult `config.admins`:
///    - Non-empty list: email must be listed.
///    - Empty list: all users are admin (permissive default for single-user installs).
/// 3. If the ProjectManager could not be loaded (error path), only the local guest passes.
fn is_local_admin(email: &str, request: &Request<Body>) -> bool {
    match request.extensions().get::<ProjectManager>() {
        Some(pm) => {
            let admins = &pm.config_manager.get_config().admins;
            oxy_auth::is_admin_email(admins, email)
        }
        // Config unavailable — restrict to built-in local guest only
        None => email == oxy_auth::LOCAL_GUEST_EMAIL,
    }
}

fn build_secret_routes(app_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(secrets::list_secrets))
        .route("/", post(secrets::create_secret))
        .route("/bulk", post(secrets::bulk_create_secrets))
        .route("/env", get(secrets::list_env_secrets))
        .route("/{id}", get(secrets::get_secret))
        .route("/{id}", put(secrets::update_secret))
        .route("/{id}", delete(secrets::delete_secret))
        .route("/{id}/value", get(secrets::reveal_secret))
        .layer(middleware::from_fn_with_state(
            app_state,
            secrets_access_middleware,
        ))
}

fn build_test_file_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(test_file::list_test_files))
        .route(
            "/project-runs",
            get(test_project_run::list_project_runs).post(test_project_run::create_project_run),
        )
        .route(
            "/project-runs/{project_run_id}",
            delete(test_project_run::delete_project_run),
        )
        .route("/{pathb64}", get(test_file::get_test_file))
        .route(
            "/{pathb64}/cases/{case_index}",
            post(test_file::run_test_case),
        )
        .route(
            "/{pathb64}/runs",
            get(test_run::list_runs).post(test_run::create_run),
        )
        .route(
            "/{pathb64}/runs/{run_index}",
            get(test_run::get_run).delete(test_run::delete_run),
        )
        .route(
            "/{pathb64}/runs/{run_index}/human-verdicts",
            get(test_run::list_human_verdicts),
        )
        .route(
            "/{pathb64}/runs/{run_index}/cases/{case_index}/human-verdict",
            put(test_run::set_human_verdict),
        )
}

fn build_app_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(app::list_apps))
        .route("/{pathb64}", get(app::get_app_data))
        .route("/{pathb64}/run", post(app::run_app))
        .route("/{pathb64}/result", post(app::get_app_result))
        .route("/{pathb64}/displays", get(app::get_displays))
        .route("/{pathb64}/charts/{chart_path}", get(app::get_chart_image))
        .route("/file/{pathb64}", get(app::get_data))
        .route("/source/{pathb64}", get(app::get_source_file))
        .route("/save-from-run/{run_id}", post(app::save_app_builder_run))
}

fn build_protected_routes(
    app_state: AppState,
    agentic_state: Arc<AgenticState>,
) -> Router<AppState> {
    Router::new()
        .merge(build_global_routes())
        .nest("/workspaces", build_workspace_routes())
        .nest(
            "/{project_id}",
            build_project_routes(app_state.clone(), agentic_state).layer(
                middleware::from_fn_with_state(app_state, project_middleware),
            ),
        )
}

fn apply_middleware(
    protected_routes: Router<AppState>,
    cloud: bool,
) -> Result<Router<AppState>, OxyError> {
    let protected_regular_routes = protected_routes
        .layer(middleware::from_fn(timeout_middleware))
        .layer(middleware::from_fn_with_state(
            AuthState::built_in(cloud),
            auth_middleware,
        ));

    Ok(protected_regular_routes)
}
fn build_backend(cloud: bool) -> providers::ProjectBackend {
    if cloud {
        providers::ProjectBackend::Cloud
    } else {
        let root = match oxy::config::resolve_local_project_path() {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    "Could not resolve project root ({}); falling back to current directory",
                    e
                );
                std::path::PathBuf::from(".")
            }
        };
        providers::ProjectBackend::Local(providers::LocalBackend { root })
    }
}

pub async fn api_router(cloud: bool, enterprise: bool, readonly: bool) -> Result<Router, OxyError> {
    let app_state = AppState {
        cloud,
        enterprise,
        internal: false,
        readonly,
        backend: std::sync::Arc::new(build_backend(cloud)),
    };
    cleanup_stale_runs().await.ok();
    let agentic_state = Arc::new(AgenticState::new());
    let public_routes = build_public_routes();
    let protected_routes = build_protected_routes(app_state.clone(), agentic_state);
    let protected_routes = apply_middleware(protected_routes, cloud)?;
    let app_routes = public_routes.merge(protected_routes);
    let cors = build_cors_layer();

    // Global timeout for ALL requests (60 seconds) - aligned with load balancer limits
    // Individual sync endpoints use their own configurable timeouts for workflow execution
    let global_timeout =
        TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(60));

    Ok(app_routes
        .with_state(app_state.clone())
        .layer(cors)
        .layer(global_timeout)
        .layer(ServiceBuilder::new().layer(NewSentryLayer::<Request<Body>>::new_from_top())))
}

pub async fn internal_api_router(
    cloud: bool,
    enterprise: bool,
    readonly: bool,
) -> Result<Router, OxyError> {
    let app_state = AppState {
        cloud,
        enterprise,
        internal: true,
        readonly,
        backend: std::sync::Arc::new(build_backend(cloud)),
    };
    cleanup_stale_runs().await.ok();
    let agentic_state = Arc::new(AgenticState::new());
    let public_routes = build_public_routes();
    let protected_routes = build_protected_routes(app_state.clone(), agentic_state);

    let protected_routes = protected_routes
        .layer(middleware::from_fn(timeout_middleware))
        .layer(middleware::from_fn(internal_auth_middleware));

    let app_routes = public_routes.merge(protected_routes);
    let cors = build_cors_layer();

    let global_timeout =
        TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(60));

    Ok(app_routes
        .with_state(app_state)
        .layer(cors)
        .layer(global_timeout)
        .layer(ServiceBuilder::new().layer(NewSentryLayer::<Request<Body>>::new_from_top())))
}

pub async fn openapi_router() -> OpenApiRouter<AppState> {
    let cors = build_cors_layer();

    OpenApiRouter::new()
        // Health check routes
        .routes(routes!(healthcheck::health_check))
        .routes(routes!(healthcheck::readiness_check))
        .routes(routes!(healthcheck::liveness_check))
        .routes(routes!(healthcheck::version_info))
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
        .routes(routes!(app::get_app_result))
        .routes(routes!(app::get_chart_image))
        // Workspace routes
        .routes(routes!(workspace::list_workspaces))
        .routes(routes!(workspace::create_workspace))
        // Project routes
        .routes(routes!(project::get_project))
        .routes(routes!(project::delete_project))
        .routes(routes!(project::get_project_branches))
        .routes(routes!(project::create_repo_from_project))
        // Run routes
        .routes(routes!(run::get_workflow_runs))
        .routes(routes!(run::create_workflow_run))
        .routes(routes!(run::cancel_workflow_run))
        .routes(routes!(run::delete_workflow_run))
        .routes(routes!(run::bulk_delete_workflow_runs))
        .routes(routes!(run::workflow_events))
        .routes(routes!(run::workflow_events_sync))
        .routes(routes!(run::get_blocks))
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
        .routes(routes!(workflow::get))
        .routes(routes!(workflow::get_logs))
        .routes(routes!(workflow::run_workflow))
        .routes(routes!(workflow::run_workflow_sync))
        .routes(routes!(workflow::run_workflow_thread))
        .routes(routes!(workflow::run_workflow_thread_sync))
        .routes(routes!(workflow::create_from_query))
        .routes(routes!(workflow::get_workflow_run))
        // Automation routes
        .routes(routes!(workflow::save_automation))
        // Database routes
        .routes(routes!(database::create_database_config))
        .routes(routes!(database::test_database_connection))
        .layer(cors)
        .layer(ServiceBuilder::new().layer(NewSentryLayer::<Request<Body>>::new_from_top()))
}
