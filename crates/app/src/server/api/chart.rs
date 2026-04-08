use crate::server::api::middlewares::workspace_context::WorkspaceManagerExtractor;
use axum::extract::{self, Path};
use axum::http::StatusCode;
use uuid::Uuid;

pub async fn get_chart(
    Path((_workspace_id, file_path)): Path<(Uuid, String)>,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
) -> Result<extract::Json<String>, StatusCode> {
    let charts_dir = workspace_manager
        .config_manager
        .get_charts_dir()
        .await
        .map_err(|e| {
            tracing::error!("Failed to get charts directory: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let file_path = charts_dir.join(file_path);

    let file = std::fs::read_to_string(file_path).map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(extract::Json(file))
}
