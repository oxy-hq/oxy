use crate::api::agent;
use crate::api::chart;
use crate::api::message;
use crate::api::thread;
use crate::api::workflow;
use crate::db::client::establish_connection;
use axum::Router;
use axum::routing::delete;
use axum::routing::{get, post};
use migration::Migrator;
use migration::MigratorTrait;
use tower_http::cors::{Any, CorsLayer};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::app;

pub async fn api_router() -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    let db = establish_connection().await;
    // migrate db
    let _ = Migrator::up(&db, None).await;

    Router::new()
        .route("/ask", post(agent::ask))
        .route("/messages/{agent}", get(message::get_messages))
        .route("/agents", get(agent::get_agents))
        .route("/apps", get(app::list_apps))
        .route("/app/{pathb64}", get(app::get_app))
        .route("/app/file/{pathb64}", get(app::get_data))
        .route("/app/{pathb64}/run", post(app::run_app))
        .route("/threads", get(thread::get_threads))
        .route("/threads/{id}", get(thread::get_thread))
        .route("/threads/{id}/ask", get(thread::ask_thread))
        .route("/threads", post(thread::create_thread))
        .route("/threads/{id}", delete(thread::delete_thread))
        .route("/threads", delete(thread::delete_all_threads))
        .route("/workflows", get(workflow::list))
        .route("/workflows/{pathb64}", get(workflow::get))
        .route("/workflows/{pathb64}/logs", get(workflow::get_logs))
        .route("/workflows/{pathb64}/run", post(workflow::run_workflow))
        .route("/agents/{pathb64}", get(agent::get_agent))
        .route(
            "/agents/{pathb64}/tests/{test_index}",
            post(agent::run_test),
        )
        .route("/charts/{file_path}", get(chart::get_chart))
        .layer(cors)
}

pub async fn openapi_router() -> OpenApiRouter {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    OpenApiRouter::new()
        .routes(routes!(agent::ask, agent::get_agents))
        .routes(routes!(workflow::list, workflow::run_workflow))
        .layer(cors)
}
