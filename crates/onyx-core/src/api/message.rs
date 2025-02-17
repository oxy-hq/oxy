use crate::service;
use crate::service::message::MessageItem;
use axum::extract::{self, Path};
use axum::http::StatusCode;

pub async fn get_messages(
    Path(agent): Path<String>,
) -> Result<extract::Json<Vec<MessageItem>>, StatusCode> {
    let res = service::message::get_messages(agent).await;
    match res {
        Ok(res) => Ok(extract::Json(res)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
