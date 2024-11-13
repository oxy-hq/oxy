use crate::api::agent;
use crate::api::config;
use crate::api::conversations;
use axum::routing::{get, post};
use axum::Router;
use std::net::SocketAddr;
use tokio;
use tower_http::cors::{Any, CorsLayer};

pub async fn serve(address: &SocketAddr) {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    let app: Router = Router::new()
        .route("/ask", post(agent::ask))
        .route("/agents", get(agent::list))
        .route("/conversations", get(conversations::list))
        .route("/load-config", get(config::load_config))
        .route("/conversation/:agent", get(conversations::get))
        .route(
            "/load-project-structure",
            get(config::list_project_dir_structure),
        );

    let listener = tokio::net::TcpListener::bind(address).await.unwrap();
    axum::serve(listener, app.layer(cors)).await.unwrap();
}
