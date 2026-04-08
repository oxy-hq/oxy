use include_dir::{Dir, include_dir};
use std::fs;
use tracing::error;

#[cfg(target_os = "windows")]
static DEMO_DIST: Dir = include_dir!("D:\\a\\oxy\\oxy\\crates\\core\\demo_project");
#[cfg(not(target_os = "windows"))]
static DEMO_DIST: Dir = include_dir!("$CARGO_MANIFEST_DIR/demo_project");

/// Copy the embedded demo project files into `target`, creating it if needed.
pub async fn copy_demo_files_to(target: &std::path::Path) -> Result<(), axum::http::StatusCode> {
    copy_embedded_dir_recursive(&DEMO_DIST, target).await
}

async fn copy_embedded_dir_recursive(
    src: &Dir<'static>,
    dst: &std::path::Path,
) -> Result<(), axum::http::StatusCode> {
    if !dst.exists() {
        fs::create_dir_all(dst).map_err(|e| {
            error!("Failed to create directory {:?}: {}", dst, e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }

    for entry in src.entries() {
        let name = entry.path().file_name().ok_or_else(|| {
            error!("Failed to get file name from path: {:?}", entry.path());
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;

        let dst_path = dst.join(name);

        if let Some(file) = entry.as_file() {
            let content = file.contents();
            fs::write(&dst_path, content).map_err(|e| {
                error!("Failed to write file {:?}: {}", dst_path, e);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;
        } else if let Some(dir) = entry.as_dir() {
            Box::pin(copy_embedded_dir_recursive(dir, &dst_path)).await?;
        }
    }

    Ok(())
}
