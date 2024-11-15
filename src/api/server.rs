use crate::api::agent;
use crate::api::config;
use crate::api::conversations;
use crate::db::client::get_db_directory;
use axum::routing::{get, post};
use axum::Router;
use migration::Migrator;
use migration::MigratorTrait;
use std::fs;
use std::net::SocketAddr;
use tokio;
use tower_http::cors::{Any, CorsLayer};

pub async fn serve(address: &SocketAddr) {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    // create db directory if not exists
    let _ = fs::create_dir_all(get_db_directory());
    let db = crate::db::client::establish_connection().await;
    // migrate db
    let _ = Migrator::up(&db, None).await;

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
