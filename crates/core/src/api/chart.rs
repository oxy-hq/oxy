use crate::api::middlewares::project::ProjectManagerExtractor;
use axum::extract::{self, Path};
use axum::http::StatusCode;
use uuid::Uuid;

pub async fn get_chart(
    Path((_project_id, file_path)): Path<(Uuid, String)>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
) -> Result<extract::Json<String>, StatusCode> {
    let charts_dir = project_manager
        .config_manager
        .get_charts_dir()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let file_path = charts_dir.join(file_path);

    let file = std::fs::read_to_string(file_path).map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(extract::Json(file))
}
