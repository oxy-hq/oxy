use crate::api::agent;
use crate::api::chart;
use crate::api::data;
use crate::api::file;
use crate::api::message;
use crate::api::thread;
use crate::api::workflow;
use crate::db::client::establish_connection;
use axum::Router;
use axum::routing::delete;
use axum::routing::put;
use axum::routing::{get, post};
use migration::Migrator;
use migration::MigratorTrait;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{self, TraceLayer};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::app;
use super::task;

pub async fn api_router() -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    // Configure HTTP request/response logging
    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(trace::DefaultMakeSpan::new().level(tracing::Level::INFO))
        .on_request(trace::DefaultOnRequest::new().level(tracing::Level::INFO))
        .on_response(
            trace::DefaultOnResponse::new()
                .level(tracing::Level::INFO)
                .latency_unit(tower_http::LatencyUnit::Millis),
        )
        .on_failure(trace::DefaultOnFailure::new().level(tracing::Level::ERROR));

    let db = establish_connection().await;
    // migrate db
    let _ = Migrator::up(&db, None).await;

    Router::new()
        .route("/ask", post(agent::ask))
        .route("/messages/{agent}", get(message::get_messages))
        .route("/agents", get(agent::get_agents))
        .route(
            "/builder-availability",
            get(agent::check_builder_availability),
        )
        .route("/apps", get(app::list_apps))
        .route("/app/{pathb64}", get(app::get_app))
        .route("/app/file/{pathb64}", get(app::get_data))
        .route("/app/{pathb64}/run", post(app::run_app))
        .route("/threads", get(thread::get_threads))
        .route("/threads/{id}", get(thread::get_thread))
        .route("/threads/{id}/ask", get(thread::ask_thread))
        .route(
            "/threads/{id}/workflow",
            post(workflow::run_workflow_thread),
        )
        .route("/threads/{id}/task", get(task::ask_task))
        .route("/threads", post(thread::create_thread))
        .route("/threads/{id}", delete(thread::delete_thread))
        .route("/threads", delete(thread::delete_all_threads))
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
        .route("/databases", get(data::list_databases))
        .route(
            "/agents/{pathb64}/tests/{test_index}",
            post(agent::run_test),
        )
        .route("/charts/{file_path}", get(chart::get_chart))
        .route("/sql/{pathb64}", post(data::execute_sql))
        .route("/agents/{pathb64}/ask", post(thread::ask_agent))
        .layer(cors)
        .layer(trace_layer)
}

pub async fn openapi_router() -> OpenApiRouter {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    // Configure HTTP request/response logging for OpenAPI router
    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(trace::DefaultMakeSpan::new().level(tracing::Level::INFO))
        .on_request(trace::DefaultOnRequest::new().level(tracing::Level::INFO))
        .on_response(
            trace::DefaultOnResponse::new()
                .level(tracing::Level::INFO)
                .latency_unit(tower_http::LatencyUnit::Millis),
        )
        .on_failure(trace::DefaultOnFailure::new().level(tracing::Level::ERROR));

    OpenApiRouter::new()
        .routes(routes!(agent::ask, agent::get_agents))
        .routes(routes!(workflow::list, workflow::run_workflow))
        .layer(cors)
        .layer(trace_layer)
}
