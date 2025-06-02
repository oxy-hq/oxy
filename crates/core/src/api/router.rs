use crate::api::agent;
use crate::api::chart;
use crate::api::data;
use crate::api::database;
use crate::api::file;
use crate::api::thread;
use crate::api::user;
use crate::api::workflow;
use crate::auth::middleware::{AuthState, auth_middleware};
use crate::auth::types::AuthMode;
use crate::db::client::establish_connection;
use crate::errors::OxyError;
use axum::Router;
use axum::middleware;
use axum::routing::delete;
use axum::routing::put;
use axum::routing::{get, post};
use migration::Migrator;
use migration::MigratorTrait;
use tower_http::cors::{Any, CorsLayer};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::app;
use super::message;
use super::task;

pub async fn api_router(auth_mode: AuthMode) -> Result<Router, OxyError> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    let db = establish_connection().await;
    // migrate db
    Migrator::up(&db, None)
        .await
        .map_err(|err| OxyError::DBError(format!("Migration failed to apply: {}", err)))?;

    let mut protected_routes = Router::new()
        .route("/user", get(user::get_current_user))
        .route("/user", put(user::update_current_user))
        .route("/threads", get(thread::get_threads))
        .route("/threads/{id}", get(thread::get_thread))
        .route("/threads/{id}/ask", post(thread::ask_thread))
        .route("/threads", post(thread::create_thread))
        .route("/threads/{id}", delete(thread::delete_thread))
        .route(
            "/threads/{id}/messages",
            get(message::get_messages_by_thread),
        )
        .route("/threads", delete(thread::delete_all_threads))
        .route("/threads/bulk-delete", post(thread::bulk_delete_threads))
        .route("/agents/{pathb64}/ask", post(thread::ask_agent))
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
        .route("/databases/build", post(data::build_embeddings));

    protected_routes = match auth_mode {
        AuthMode::IAP => protected_routes.route_layer(middleware::from_fn_with_state(
            AuthState::iap()?,
            auth_middleware,
        )),
        AuthMode::IAPCloudRun => protected_routes.route_layer(middleware::from_fn_with_state(
            AuthState::iap_cloud_run(),
            auth_middleware,
        )),
        AuthMode::Local => protected_routes.route_layer(middleware::from_fn_with_state(
            AuthState::local(),
            auth_middleware,
        )),
    };

    Ok(protected_routes.layer(cors))
}

pub async fn openapi_router() -> OpenApiRouter {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    OpenApiRouter::new()
        .routes(routes!(agent::get_agents))
        .routes(routes!(workflow::list, workflow::run_workflow))
        .layer(cors)
}
