use crate::api::agent;
use crate::api::api_keys;
use crate::api::auth;
use crate::api::chart;
use crate::api::data;
use crate::api::database;
use crate::api::file;
use crate::api::github;
use crate::api::project;
use crate::api::secrets;
use crate::api::thread;
use crate::api::user;
use crate::api::workflow;
use crate::auth::middleware::{AuthState, admin_middleware, auth_middleware};
use crate::auth::types::AuthMode;
use crate::config::ConfigBuilder;
use crate::errors::OxyError;
use crate::project::resolve_project_path;
use axum::Router;
use axum::middleware;
use axum::routing::delete;
use axum::routing::put;
use axum::routing::{get, post};
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::app;
use super::artifacts;
use super::message;
use super::task;

pub async fn api_router(auth_mode: AuthMode, readonly_mode: bool) -> Result<Router, OxyError> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    // Check if we're in onboarding mode (no config.yml found)
    let project_path_result: Result<std::path::PathBuf, OxyError> = resolve_project_path();

    // Only try to build config if we have a valid project path
    let config = if let Ok(project_path) = project_path_result {
        Some(
            ConfigBuilder::new()
                .with_project_path(&project_path)
                .map_err(|_| OxyError::ConfigurationError("Failed to build config".to_owned()))?
                .build()
                .await
                .map_err(|_| OxyError::ConfigurationError("Failed to build config".to_owned()))?,
        )
    } else {
        None
    };

    let auth = config.as_ref().and_then(|c| c.get_authentication());
    let mut public_routes = Router::new()
        .route("/auth/config", get(auth::get_config))
        // GitHub project status - needed for onboarding decisions
        .route("/project/status", get(project::get_project_status));

    // authentication
    public_routes = public_routes
        .route("/auth/login", post(auth::login))
        .route("/auth/register", post(auth::register))
        .route("/auth/google", post(auth::google_auth))
        .route("/auth/validate_email", post(auth::validate_email));

    let mut protected_routes = Router::new()
        .route("/user", get(user::get_current_user))
        .route("/logout", get(user::logout))
        // GitHub integration routes - read-only operations
        .route("/github/repositories", get(github::list_repositories))
        .route("/projects/current", get(github::get_current_project))
        // GitHub integration routes - writing operations
        .route("/github/token", post(github::store_token))
        .route(
            "/github/repositories/select",
            post(github::select_repository),
        )
        .route("/git/pull", post(github::pull_repository))
        .route("/workflows/{pathb64}/run", post(workflow::run_workflow))
        // App operations - writing
        .route("/app/{pathb64}/run", post(app::run_app))
        // Thread operations - writing
        // Regular protected routes (only auth_middleware)
        .route("/users", get(user::list_users))
        .route("/artifacts/{id}", get(artifacts::get_artifact))
        .route("/threads", post(thread::create_thread))
        .route("/threads/{id}", delete(thread::delete_thread))
        .route("/threads", delete(thread::delete_all_threads))
        .route("/threads/bulk-delete", post(thread::bulk_delete_threads))
        .route(
            "/threads/{id}/workflow",
            post(workflow::run_workflow_thread),
        )
        .route("/threads/{id}/task", post(task::ask_task))
        .route("/threads", get(thread::get_threads))
        .route("/threads/{id}", get(thread::get_thread))
        .route("/logs", get(thread::get_logs))
        .route(
            "/threads/{id}/messages",
            get(message::get_messages_by_thread),
        )
        .route("/agents", get(agent::get_agents))
        .route("/agents/{pathb64}", get(agent::get_agent))
        .route("/agents/{pathb64}/ask", post(agent::ask_agent_preview))
        .route("/api-keys", get(api_keys::list_api_keys))
        .route("/api-keys", post(api_keys::create_api_key))
        .route("/api-keys/{id}", get(api_keys::get_api_key))
        .route("/api-keys/{id}", delete(api_keys::delete_api_key))
        .route("/app/{pathb64}", get(app::get_app))
        .route("/app/file/{pathb64}", get(app::get_data))
        .route("/apps", get(app::list_apps))
        .route(
            "/builder-availability",
            get(agent::check_builder_availability),
        )
        .route("/workflows", get(workflow::list))
        .route("/workflows/{pathb64}", get(workflow::get))
        .route("/workflows/{pathb64}/logs", get(workflow::get_logs))
        .route("/files", get(file::get_file_tree))
        .route("/files/{pathb64}", get(file::get_file))
        .route("/charts/{file_path}", get(chart::get_chart))
        .route("/databases", get(database::list_databases))
        .route("/threads/{id}/agent", post(agent::ask_agent))
        // Secret management routes - read-only
        .route("/secrets", get(secrets::list_secrets))
        .route("/secrets/{id}", get(secrets::get_secret))
        // GitHub settings routes - read-only
        .route("/github/settings", get(github::get_github_settings))
        .route("/github/revision", get(github::get_revision_info))
        // Database and SQL operations - writing
        .route("/databases/sync", post(database::sync_database))
        .route("/databases/build", post(data::build_embeddings))
        .route("/sql/{pathb64}", post(data::execute_sql))
        // Secret management routes - writing
        .route("/secrets", post(secrets::create_secret))
        .route("/secrets/bulk", post(secrets::bulk_create_secrets))
        .route("/secrets/{id}", put(secrets::update_secret))
        .route("/secrets/{id}", delete(secrets::delete_secret))
        // GitHub settings routes - writing
        .route("/github/settings", put(github::update_github_settings))
        .route("/github/onboarded", put(github::set_onboarded))
        .route("/github/sync", post(github::sync_github_repository))
        .route("/threads/{id}/stop", post(thread::stop_thread));

    // Add writing operations if not in readonly mode
    if !readonly_mode {
        protected_routes = protected_routes
            // Workflow operations - writing
            .route("/workflows/from-query", post(workflow::create_from_query))
            // File operations - writing
            .route("/files/{pathb64}", post(file::save_file))
            .route("/files/{pathb64}/delete-file", delete(file::delete_file))
            .route(
                "/files/{pathb64}/delete-folder",
                delete(file::delete_folder),
            )
            .route("/files/{pathb64}/rename-file", put(file::rename_file))
            .route("/files/{pathb64}/rename-folder", put(file::rename_folder))
            .route("/files/{pathb64}/new-file", post(file::create_file))
            .route("/files/{pathb64}/new-folder", post(file::create_folder))
            // Agent operations - writing
            .route(
                "/agents/{pathb64}/tests/{test_index}",
                post(agent::run_test),
            );
    }

    // GitHub API endpoints are public, not requiring authentication

    // Admin-only routes with explicit middleware ordering: auth_middleware -> admin_middleware
    let admin_routes = Router::new()
        .route("/users/{id}", delete(user::delete_user))
        .route("/users/{id}", put(user::update_user));

    // Apply middleware to regular routes (only auth)
    let protected_regular_routes = match auth_mode {
        AuthMode::IAP => protected_routes.layer(middleware::from_fn_with_state(
            AuthState::iap()?,
            auth_middleware,
        )),
        AuthMode::IAPCloudRun => protected_routes.layer(middleware::from_fn_with_state(
            AuthState::iap_cloud_run(),
            auth_middleware,
        )),
        AuthMode::Cognito => protected_routes.layer(middleware::from_fn_with_state(
            AuthState::cognito(),
            auth_middleware,
        )),
        AuthMode::BuiltIn => protected_routes.layer(middleware::from_fn_with_state(
            AuthState::built_in(auth.clone()),
            auth_middleware,
        )),
    };

    // Apply middleware to admin routes (auth + admin) with explicit ordering
    let protected_admin_routes = match auth_mode {
        AuthMode::IAP => {
            let auth_state = AuthState::iap()?;
            admin_routes.layer(
                ServiceBuilder::new()
                    .layer(middleware::from_fn_with_state(auth_state, auth_middleware))
                    .layer(middleware::from_fn(admin_middleware)),
            )
        }
        AuthMode::IAPCloudRun => {
            let auth_state = AuthState::iap_cloud_run();
            admin_routes.layer(
                ServiceBuilder::new()
                    .layer(middleware::from_fn_with_state(auth_state, auth_middleware))
                    .layer(middleware::from_fn(admin_middleware)),
            )
        }
        AuthMode::Cognito => {
            let auth_state = AuthState::cognito();
            admin_routes.layer(
                ServiceBuilder::new()
                    .layer(middleware::from_fn_with_state(auth_state, auth_middleware))
                    .layer(middleware::from_fn(admin_middleware)),
            )
        }
        AuthMode::BuiltIn => {
            let auth_state = AuthState::built_in(auth);
            admin_routes.layer(
                ServiceBuilder::new()
                    .layer(middleware::from_fn_with_state(auth_state, auth_middleware))
                    .layer(middleware::from_fn(admin_middleware)),
            )
        }
    };

    // Merge all protected routes
    let protected_routes = protected_regular_routes.merge(protected_admin_routes);

    let app_routes = public_routes.merge(protected_routes);

    Ok(app_routes.with_state(auth_mode).layer(cors))
}

pub async fn openapi_router(_readonly_mode: bool) -> OpenApiRouter {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    OpenApiRouter::new()
        // Agent routes
        .routes(routes!(agent::get_agents))
        .routes(routes!(agent::ask_agent_preview))
        // API Keys routes
        .routes(routes!(api_keys::create_api_key))
        .routes(routes!(api_keys::list_api_keys))
        .routes(routes!(api_keys::get_api_key))
        .routes(routes!(api_keys::delete_api_key))
        // App routes
        .routes(routes!(app::list_apps))
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
}
