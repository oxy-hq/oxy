use axum::extract::{self, Path};
use axum::http::StatusCode;

pub async fn get_chart(Path(file_path): Path<String>) -> Result<extract::Json<String>, StatusCode> {
    let file = std::fs::read_to_string(file_path).map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(extract::Json(file))
}
