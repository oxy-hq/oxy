use crate::api::agent;
use crate::api::api_keys;
use crate::api::auth;
use crate::api::chart;
use crate::api::data;
use crate::api::database;
use crate::api::file;
use crate::api::thread;
use crate::api::user;
use crate::api::workflow;
use crate::auth::middleware::{AuthState, admin_middleware, auth_middleware};
use crate::auth::types::AuthMode;
use crate::config::ConfigBuilder;
use crate::db::client::establish_connection;
use crate::errors::OxyError;
use crate::utils::find_project_path;
use axum::Router;
use axum::middleware;
use axum::routing::delete;
use axum::routing::put;
use axum::routing::{get, post};
use migration::Migrator;
use migration::MigratorTrait;
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::app;
use super::artifacts;
use super::message;
use super::task;

pub async fn api_router(auth_mode: AuthMode) -> Result<Router, OxyError> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    let db = establish_connection().await;

    let project_path = find_project_path()
        .map_err(|_| OxyError::ConfigurationError("Failed to find project path".to_owned()))?;

    let config = ConfigBuilder::new()
        .with_project_path(&project_path)
        .map_err(|_| OxyError::ConfigurationError("Failed to build config".to_owned()))?
        .build()
        .await
        .map_err(|_| OxyError::ConfigurationError("Failed to build config".to_owned()))?;

    let auth = config.get_authentication();

    // migrate db
    Migrator::up(&db, None)
        .await
        .map_err(|err| OxyError::DBError(format!("Migration failed to apply: {err}")))?;

    let public_routes = Router::new()
        .route("/auth/config", get(auth::get_config))
        .route("/auth/login", post(auth::login))
        .route("/auth/register", post(auth::register))
        .route("/auth/google", post(auth::google_auth))
        .route("/auth/validate_email", post(auth::validate_email));

    // Regular protected routes (only auth_middleware)
    let regular_routes = Router::new()
        .route("/users", get(user::list_users))
        .route("/logout", get(user::logout))
        .route("/me", get(user::get_current_user))
        .route("/threads", get(thread::get_threads))
        .route("/threads/{id}", get(thread::get_thread))
        .route("/artifacts/{id}", get(artifacts::get_artifact))
        .route("/threads/{id}/ask", post(thread::ask_thread))
        .route("/threads", post(thread::create_thread))
        .route("/threads/{id}", delete(thread::delete_thread))
        .route(
            "/threads/{id}/messages",
            get(message::get_messages_by_thread),
        )
        .route("/threads", delete(thread::delete_all_threads))
        .route("/threads/bulk-delete", post(thread::bulk_delete_threads))
        .route("/agents/{pathb64}/ask", post(agent::ask_agent))
        .route(
            "/threads/{id}/workflow",
            post(workflow::run_workflow_thread),
        )
        .route("/threads/{id}/task", post(task::ask_task))
        .route("/agents", get(agent::get_agents))
        .route(
            "/builder-availability",
            get(agent::check_builder_availability),
        )
        .route("/apps", get(app::list_apps))
        .route("/app/{pathb64}", get(app::get_app))
        .route("/app/file/{pathb64}", get(app::get_data))
        .route("/app/{pathb64}/run", post(app::run_app))
        .route("/workflows", get(workflow::list))
        .route("/workflows/from-query", post(workflow::create_from_query))
        .route("/workflows/{pathb64}", get(workflow::get))
        .route("/workflows/{pathb64}/logs", get(workflow::get_logs))
        .route("/workflows/{pathb64}/run", post(workflow::run_workflow))
        .route("/agents/{pathb64}", get(agent::get_agent))
        .route("/files", get(file::get_file_tree))
        .route("/files/{pathb64}", get(file::get_file))
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
        .route(
            "/agents/{pathb64}/tests/{test_index}",
            post(agent::run_test),
        )
        .route("/charts/{file_path}", get(chart::get_chart))
        .route("/sql/{pathb64}", post(data::execute_sql))
        .route("/databases", get(database::list_databases))
        .route("/databases/sync", post(database::sync_database))
        .route("/databases/build", post(data::build_embeddings))
        .route("/api-keys", post(api_keys::create_api_key))
        .route("/api-keys", get(api_keys::list_api_keys))
        .route("/api-keys/{id}", get(api_keys::get_api_key))
        .route("/api-keys/{id}", delete(api_keys::delete_api_key));

    // Admin-only routes with explicit middleware ordering: auth_middleware -> admin_middleware
    let admin_routes = Router::new()
        .route("/users/{id}", delete(user::delete_user))
        .route("/users/{id}", put(user::update_user));

    // Apply middleware to regular routes (only auth)
    let protected_regular_routes = match auth_mode {
        AuthMode::IAP => regular_routes.layer(middleware::from_fn_with_state(
            AuthState::iap()?,
            auth_middleware,
        )),
        AuthMode::IAPCloudRun => regular_routes.layer(middleware::from_fn_with_state(
            AuthState::iap_cloud_run(),
            auth_middleware,
        )),
        AuthMode::Cognito => regular_routes.layer(middleware::from_fn_with_state(
            AuthState::cognito(),
            auth_middleware,
        )),
        AuthMode::BuiltIn => regular_routes.layer(middleware::from_fn_with_state(
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

pub async fn openapi_router() -> OpenApiRouter {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    OpenApiRouter::new()
        // Agent routes
        .routes(routes!(agent::get_agents))
        .routes(routes!(agent::ask_agent))
        // API Keys routes
        .routes(routes!(api_keys::create_api_key))
        .routes(routes!(api_keys::list_api_keys))
        .routes(routes!(api_keys::get_api_key))
        .routes(routes!(api_keys::delete_api_key))
        // App routes
        .routes(routes!(app::list_apps))
        // Workflow routes
        .routes(routes!(workflow::list))
        .routes(routes!(workflow::get_logs))
        .routes(routes!(workflow::run_workflow))
        .routes(routes!(workflow::run_workflow_thread))
        .layer(cors)
}
