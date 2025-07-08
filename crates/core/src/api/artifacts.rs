use axum::extract::Path;
use sea_orm::EntityTrait;
use serde_json::Value;
use uuid::Uuid;

use crate::{auth::extractor::AuthenticatedUserExtractor, db::client::establish_connection};
use axum::{
    extract::{self},
    http::StatusCode,
};
use entity::prelude::Artifacts;

#[derive(serde::Serialize)]
pub struct ArtifactItem {
    pub id: String,
    pub content: Value,
    pub kind: String,
    pub message_id: String,
    pub thread_id: String,
}

pub async fn get_artifact(
    Path(id): Path<String>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
) -> Result<extract::Json<ArtifactItem>, StatusCode> {
    let connection = establish_connection().await?;
    let artifact_id = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;

    let artifact = Artifacts::find_by_id(artifact_id)
        .one(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(extract::Json(ArtifactItem {
        id: artifact.id.to_string(),
        content: artifact.content,
        kind: artifact.kind,
        message_id: artifact.message_id.to_string(),
        thread_id: artifact.thread_id.to_string(),
    }))
}
