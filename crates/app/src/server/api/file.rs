use crate::server::api::middlewares::project::ProjectManagerExtractor;
use crate::server::router::AppState;
use crate::server::service::project::ProjectService;
use axum::Json;
use axum::extract::{self, Path, State};
use axum::http::StatusCode;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use futures::TryFutureExt;
use oxy::github::{FileStatus, GitOperations};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::path::PathBuf;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct SaveFileRequest {
    pub data: String,
}

pub async fn create_file(
    State(app_state): State<AppState>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
) -> Result<extract::Json<String>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let file_path = project_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;

    let file_path = PathBuf::from(&file_path);

    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            tracing::error!("Failed to create parent directory {:?}: {}", parent, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }
    fs::write(&file_path, "").map_err(|e| {
        tracing::error!("Failed to create file {:?}: {}", file_path, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(extract::Json("success".to_string()))
}

pub async fn create_folder(
    State(app_state): State<AppState>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
) -> Result<extract::Json<String>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let file_path = project_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;

    let file_path = PathBuf::from(&file_path);
    fs::create_dir_all(&file_path).map_err(|e| {
        tracing::error!("Failed to create folder {:?}: {}", file_path, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(extract::Json("success".to_string()))
}

pub async fn delete_file(
    State(app_state): State<AppState>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
) -> Result<extract::Json<String>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let file_path = project_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;

    let file_path = PathBuf::from(&file_path);
    fs::remove_file(&file_path).map_err(|e| {
        tracing::error!("Failed to delete file {:?}: {}", file_path, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(extract::Json("success".to_string()))
}

pub async fn delete_folder(
    State(app_state): State<AppState>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
) -> Result<extract::Json<String>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let file_path = project_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;

    let file_path = PathBuf::from(&file_path);
    fs::remove_dir_all(&file_path).map_err(|e| {
        tracing::error!("Failed to delete folder {:?}: {}", file_path, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(extract::Json("success".to_string()))
}

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct RenameFileRequest {
    pub new_name: String,
}

pub async fn rename_file(
    State(app_state): State<AppState>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    extract::Json(payload): extract::Json<RenameFileRequest>,
) -> Result<extract::Json<String>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let file_path = project_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;

    let file_path = PathBuf::from(&file_path);

    let new_file_path = project_manager
        .config_manager
        .resolve_file(payload.new_name)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;

    fs::rename(&file_path, PathBuf::from(&new_file_path)).map_err(|e| {
        tracing::error!("Failed to rename file {:?}: {}", file_path, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(extract::Json("success".to_string()))
}

pub async fn rename_folder(
    State(app_state): State<AppState>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    extract::Json(payload): extract::Json<RenameFileRequest>,
) -> Result<extract::Json<String>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let file_path = project_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;

    let file_path = PathBuf::from(&file_path);

    let new_file_path = project_manager
        .config_manager
        .resolve_file(payload.new_name)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;

    fs::rename(&file_path, PathBuf::from(&new_file_path)).map_err(|e| {
        tracing::error!("Failed to rename folder {:?}: {}", file_path, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(extract::Json("success".to_string()))
}

#[axum::debug_handler]
pub async fn save_file(
    State(app_state): State<AppState>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    extract::Json(payload): extract::Json<SaveFileRequest>,
) -> Result<extract::Json<String>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let file_path = project_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;

    let file_path = PathBuf::from(&file_path);
    fs::write(&file_path, payload.data).map_err(|e| {
        tracing::error!("Failed to save file {:?}: {}", file_path, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(extract::Json("success".to_string()))
}

pub async fn get_diff_summary(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
) -> Result<Json<Vec<FileStatus>>, StatusCode> {
    let repo_path = project_manager.config_manager.project_path();

    match GitOperations::diff_numstat_summary(repo_path).await {
        Ok(file_statuses) => Ok(Json(file_statuses)),
        Err(e) => {
            tracing::error!("{:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
pub async fn get_file(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
) -> Result<extract::Json<String>, StatusCode> {
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let file_path = project_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;

    let file_path = PathBuf::from(&file_path);
    let file_content = fs::read_to_string(&file_path).map_err(|e| {
        tracing::error!("Failed to read file {:?}: {}", file_path, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(extract::Json(file_content))
}

pub async fn get_file_from_git(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
) -> Result<extract::Json<String>, StatusCode> {
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let repo_path = project_manager.config_manager.project_path();
    let file_path = project_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let relative_file_path = PathBuf::from(&file_path)
        .strip_prefix(repo_path)
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .to_string_lossy()
        .to_string();
    let file_content = ProjectService::get_file_from_git(repo_path, &relative_file_path).await?;
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

pub async fn get_file_tree(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
) -> Result<extract::Json<Vec<FileTree>>, StatusCode> {
    let project_path = project_manager.config_manager.project_path();
    let project_path = PathBuf::from(project_path);
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
