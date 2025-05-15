use crate::utils::find_project_path;
use axum::extract::{self, Path};
use axum::http::StatusCode;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::path::PathBuf;
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct SaveFileRequest {
    pub data: String,
}

#[axum::debug_handler]
pub async fn save_file(
    Path(pathb64): Path<String>,
    extract::Json(payload): extract::Json<SaveFileRequest>,
) -> Result<extract::Json<String>, StatusCode> {
    let decoded_path: Vec<u8> = BASE64_STANDARD.decode(pathb64).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    let path = String::from_utf8(decoded_path).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    let project_path = find_project_path().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let file_path = project_path.join(path);
    fs::write(file_path, payload.data).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(extract::Json("success".to_string()))
}

pub async fn get_file(Path(pathb64): Path<String>) -> Result<extract::Json<String>, StatusCode> {
    let decoded_path: Vec<u8> = BASE64_STANDARD.decode(pathb64).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    let path = String::from_utf8(decoded_path).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    let project_path = find_project_path().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let file_path = project_path.join(path);
    let file_content = fs::read_to_string(file_path).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(extract::Json(file_content))
}

#[derive(Serialize, Deserialize, Clone, ToSchema, Debug)]
pub struct FileTree {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub children: Vec<FileTree>,
}

impl Display for FileTree {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

pub async fn get_file_tree() -> Result<extract::Json<Vec<FileTree>>, StatusCode> {
    let project_path = find_project_path().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let file_tree = get_file_tree_recursive(&project_path, &project_path);
    Ok(extract::Json(file_tree.children))
}

fn get_file_tree_recursive(path: &PathBuf, project_path: &PathBuf) -> FileTree {
    let mut file_tree = FileTree {
        name: path.file_name().unwrap().to_string_lossy().to_string(),
        path: path
            .strip_prefix(project_path)
            .ok()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap(),
        is_dir: path.is_dir(),
        children: vec![],
    };
    if path.is_dir() {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let entry_path = entry.path();
            file_tree
                .children
                .push(get_file_tree_recursive(&entry_path, project_path));
        }
    }
    file_tree
}
