use crate::service;
use crate::service::agent::AskRequest;
use axum::extract;
use axum::response::IntoResponse;
use axum_streams::StreamBodyAs;

pub async fn ask(extract::Json(payload): extract::Json<AskRequest>) -> impl IntoResponse {
    let s = service::agent::ask(payload).await;
    StreamBodyAs::json_nl(s)
}
