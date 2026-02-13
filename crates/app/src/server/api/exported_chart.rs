use crate::server::api::middlewares::project::ProjectManagerExtractor;
use axum::body::Body;
use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::Response;
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

/// Serve exported chart images (PNG files) from the exported chart directory
///
/// This endpoint streams PNG files that were exported via the export_chart CLI command.
/// Files are named with the format: {name}-{index}-{uuid}.png
pub async fn get_exported_chart(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path((_project_id, file_name)): Path<(Uuid, String)>,
) -> Result<Response, StatusCode> {
    // Validate file format - must be a PNG file
    if !file_name.ends_with(".png") {
        tracing::warn!("Invalid file format: {}", file_name);
        return Err(StatusCode::BAD_REQUEST);
    }

    // Basic filename validation to prevent directory traversal
    if file_name.contains("..") || file_name.contains('/') || file_name.contains('\\') {
        tracing::warn!(
            "Invalid filename (potential directory traversal): {}",
            file_name
        );
        return Err(StatusCode::BAD_REQUEST);
    }

    let exported_chart_dir = project_manager
        .config_manager
        .get_exported_chart_dir()
        .await
        .map_err(|e| {
            tracing::error!("Failed to get exported chart directory: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let file_path = exported_chart_dir.join(&file_name);

    // Check if file exists
    if !file_path.exists() {
        tracing::warn!("Exported chart file not found: {:?}", file_path);
        return Err(StatusCode::NOT_FOUND);
    }

    // Open the file
    let file = File::open(&file_path).await.map_err(|e| {
        tracing::error!("Failed to open exported chart file: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Get file metadata for content length
    let metadata = file.metadata().await.map_err(|e| {
        tracing::error!("Failed to get file metadata: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Create a stream from the file
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    // Build response with appropriate headers for PNG images
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::CONTENT_LENGTH, metadata.len())
        .header(
            header::CONTENT_DISPOSITION,
            format!("inline; filename=\"{}\"", file_name),
        )
        .body(body)
        .map_err(|e| {
            tracing::error!("Failed to build response: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(response)
}
