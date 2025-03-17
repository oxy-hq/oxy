use crate::api::agent;
use crate::api::message;
use crate::api::thread;
use axum::routing::{get, post};
use axum::Router;
use migration::Migrator;
use migration::MigratorTrait;
use std::net::SocketAddr;
use tokio;
use tower_http::cors::{Any, CorsLayer};

use super::workflow;

pub async fn serve(address: &SocketAddr) {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    let db = crate::db::client::establish_connection().await;
    // migrate db
    let _ = Migrator::up(&db, None).await;

    let app: Router = Router::new()
        .route("/ask", post(agent::ask))
        .route("/messages/:agent", get(message::get_messages))
        .route("/agents", get(agent::get_agents))
        .route("/threads", get(thread::get_threads))
        .route("/threads/:id", get(thread::get_thread))
        .route("/threads/:id/ask", get(thread::ask_thread))
        .route("/threads", post(thread::create_thread))
        .route("/workflows", get(workflow::list))
        .route("/workflows/:pathb64", get(workflow::get))
        .route("/workflows/:pathb64/logs", get(workflow::get_logs))
        .route("/workflows/:pathb64/run", post(workflow::run_workflow));

    let listener = tokio::net::TcpListener::bind(address).await.unwrap();
    axum::serve(listener, app.layer(cors)).await.unwrap();
}
