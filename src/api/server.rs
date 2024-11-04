use std::net::SocketAddr;
use axum::Router;
use tower_http::services::ServeDir;
use axum::routing::{get,post};
use crate::api::conversations;
use crate::api::agent;
use tokio;




fn serve_embedded() -> ServeDir {
    return ServeDir::new("../dist")
}


pub async fn serve(address: &SocketAddr) {
    let app: Router = Router::new()
        .route("/ask", post(agent::ask))
        .route("/conversations", post(conversations::create))
        .route("/conversations", get(conversations::list))
        .fallback_service(serve_embedded());

    let listener = tokio::net::TcpListener::bind(address).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
