use crate::adapters::connector::load_result;
use crate::adapters::project::manager::ProjectManager;
use crate::api::middlewares::project::ProjectManagerExtractor;
use crate::auth::extractor::AuthenticatedUserExtractor;
use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use axum::body::Body;
use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::Response;
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

/// Convert Arrow result to Parquet and save to results directory
///
/// This utility function:
/// 1. Loads the Arrow result from the temp file
/// 2. Gets the results directory from the project manager
/// 3. Generates a new UUID-based filename with .parquet extension
/// 4. Converts and writes the data as Parquet format
/// 5. Cleans up the temporary file
/// 6. Returns the filename
///
/// # Arguments
/// * `project_manager` - The project manager containing config
/// * `temp_file_path` - Path to the temporary Arrow result file
///
/// # Returns
/// * `Ok(String)` - The filename of the Parquet result file
/// * `Err(String)` - Error message if any step fails
pub async fn store_result_file(
    project_manager: &ProjectManager,
    temp_file_path: &str,
) -> Result<String, String> {
    // Load the Arrow result
    let (batches, schema) =
        load_result(temp_file_path).map_err(|e| format!("Failed to load Arrow result: {}", e))?;

    // Get the results directory
    let results_dir = project_manager
        .config_manager
        .get_results_dir()
        .await
        .map_err(|e| format!("Failed to get results directory: {}", e))?;

    // Generate a new filename with .parquet extension
    let file_name = format!("{}.parquet", uuid::Uuid::new_v4());
    let dest_path = results_dir.join(&file_name);

    // Write as Parquet
    write_parquet(&dest_path, &batches, schema)
        .map_err(|e| format!("Failed to write Parquet file: {}", e))?;

    // Clean up temp file
    let _ = tokio::fs::remove_file(temp_file_path).await;

    Ok(file_name)
}

/// Write record batches to a Parquet file
fn write_parquet(
    file_path: &std::path::Path,
    batches: &[RecordBatch],
    schema: SchemaRef,
) -> Result<(), String> {
    let file =
        std::fs::File::create(file_path).map_err(|e| format!("Failed to create file: {}", e))?;

    let props = WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .build();

    let mut writer = ArrowWriter::try_new(file, schema, Some(props))
        .map_err(|e| format!("Failed to create Parquet writer: {}", e))?;

    for batch in batches {
        writer
            .write(batch)
            .map_err(|e| format!("Failed to write batch: {}", e))?;
    }

    writer
        .close()
        .map_err(|e| format!("Failed to close writer: {}", e))?;

    Ok(())
}

/// Serve Parquet result files for query results
///
/// This endpoint streams Parquet files from the results directory
/// Files are named with UUIDs and stored in the state directory
pub async fn get_result_file(
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path((_project_id, file_name)): Path<(Uuid, String)>,
) -> Result<Response, StatusCode> {
    // Validate file format
    if !file_name.ends_with(".parquet") {
        tracing::warn!("Invalid file format: {}", file_name);
        return Err(StatusCode::BAD_REQUEST);
    }

    // Extract the UUID part and validate it to prevent directory traversal
    let file_uuid = file_name
        .strip_suffix(".parquet")
        .ok_or(StatusCode::BAD_REQUEST)?;

    if Uuid::parse_str(file_uuid).is_err() {
        tracing::warn!("Invalid UUID in filename: {}", file_uuid);
        return Err(StatusCode::BAD_REQUEST);
    }

    // Get the results directory from the project manager
    let results_dir = project_manager
        .config_manager
        .get_results_dir()
        .await
        .map_err(|e| {
            tracing::error!("Failed to get results directory: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Construct the full file path
    let file_path = results_dir.join(&file_name);

    // Check if file exists
    if !file_path.exists() {
        tracing::warn!("Result file not found: {:?}", file_path);
        return Err(StatusCode::NOT_FOUND);
    }

    // Open the file
    let file = File::open(&file_path).await.map_err(|e| {
        tracing::error!("Failed to open result file: {}", e);
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

    // Build response with appropriate headers
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/vnd.apache.parquet")
        .header(header::CONTENT_LENGTH, metadata.len())
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", file_name),
        )
        .body(body)
        .map_err(|e| {
            tracing::error!("Failed to build response: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(response)
}

/// Delete a result file
///
/// This endpoint allows cleanup of temporary result files
pub async fn delete_result_file(
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path((_project_id, file_id)): Path<(Uuid, String)>,
) -> Result<StatusCode, StatusCode> {
    // Validate file_id format
    if !file_id.ends_with(".parquet") {
        tracing::warn!("Invalid file format for deletion: {}", file_id);
        return Err(StatusCode::BAD_REQUEST);
    }

    // Extract the UUID part
    let file_uuid = file_id
        .strip_suffix(".parquet")
        .ok_or(StatusCode::BAD_REQUEST)?;

    // Validate it's a valid UUID
    if Uuid::parse_str(file_uuid).is_err() {
        tracing::warn!("Invalid UUID in file_id for deletion: {}", file_uuid);
        return Err(StatusCode::BAD_REQUEST);
    }

    // Get the results directory
    let results_dir = project_manager
        .config_manager
        .get_results_dir()
        .await
        .map_err(|e| {
            tracing::error!("Failed to get results directory: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Construct the full file path
    let file_path = results_dir.join(&file_id);

    // Check if file exists
    if !file_path.exists() {
        tracing::warn!("Result file not found for deletion: {:?}", file_path);
        return Err(StatusCode::NOT_FOUND);
    }

    // Delete the file
    tokio::fs::remove_file(&file_path).await.map_err(|e| {
        tracing::error!("Failed to delete result file: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tracing::info!("Successfully deleted result file: {}", file_id);
    Ok(StatusCode::NO_CONTENT)
}
