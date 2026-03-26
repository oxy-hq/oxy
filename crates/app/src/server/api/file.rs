use crate::server::api::middlewares::project::{BranchQuery, ProjectManagerExtractor};
use crate::server::router::AppState;
use crate::server::service::project::ProjectService;
use axum::Json;
use axum::extract::{self, Path, Query, State};
use axum::http::StatusCode;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use futures::TryFutureExt;
use oxy::github::{FileStatus, GitOperations};
use oxy_project::LocalGitService;
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};
use std::fs as sync_fs;
use std::path::PathBuf;
use tokio::fs;
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
    Query(branch_query): Query<BranchQuery>,
) -> Result<extract::Json<String>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let resolved = project_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let project_root = project_manager.config_manager.project_path().to_path_buf();
    let file_path = app_state
        .backend
        .resolve_path(
            branch_query.branch.as_deref(),
            PathBuf::from(&resolved),
            &project_root,
        )
        .await;

    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).await.map_err(|e| {
            tracing::error!("Failed to create parent directory {:?}: {}", parent, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }
    fs::write(&file_path, "").await.map_err(|e| {
        tracing::error!("Failed to create file {:?}: {}", file_path, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(extract::Json("success".to_string()))
}

pub async fn create_folder(
    State(app_state): State<AppState>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    Query(branch_query): Query<BranchQuery>,
) -> Result<extract::Json<String>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let resolved = project_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let project_root = project_manager.config_manager.project_path().to_path_buf();
    let file_path = app_state
        .backend
        .resolve_path(
            branch_query.branch.as_deref(),
            PathBuf::from(&resolved),
            &project_root,
        )
        .await;
    fs::create_dir_all(&file_path).await.map_err(|e| {
        tracing::error!("Failed to create folder {:?}: {}", file_path, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(extract::Json("success".to_string()))
}

pub async fn delete_file(
    State(app_state): State<AppState>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    Query(branch_query): Query<BranchQuery>,
) -> Result<extract::Json<String>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let resolved = project_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let project_root = project_manager.config_manager.project_path().to_path_buf();
    let file_path = app_state
        .backend
        .resolve_path(
            branch_query.branch.as_deref(),
            PathBuf::from(&resolved),
            &project_root,
        )
        .await;
    fs::remove_file(&file_path).await.map_err(|e| {
        tracing::error!("Failed to delete file {:?}: {}", file_path, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(extract::Json("success".to_string()))
}

pub async fn delete_folder(
    State(app_state): State<AppState>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    Query(branch_query): Query<BranchQuery>,
) -> Result<extract::Json<String>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let resolved = project_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let project_root = project_manager.config_manager.project_path().to_path_buf();
    let file_path = app_state
        .backend
        .resolve_path(
            branch_query.branch.as_deref(),
            PathBuf::from(&resolved),
            &project_root,
        )
        .await;
    fs::remove_dir_all(&file_path).await.map_err(|e| {
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
    Query(branch_query): Query<BranchQuery>,
    extract::Json(payload): extract::Json<RenameFileRequest>,
) -> Result<extract::Json<String>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let project_root = project_manager.config_manager.project_path().to_path_buf();
    let resolved = project_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let file_path = app_state
        .backend
        .resolve_path(
            branch_query.branch.as_deref(),
            PathBuf::from(&resolved),
            &project_root,
        )
        .await;
    let resolved_new = project_manager
        .config_manager
        .resolve_file(payload.new_name)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let new_file_path = app_state
        .backend
        .resolve_path(
            branch_query.branch.as_deref(),
            PathBuf::from(&resolved_new),
            &project_root,
        )
        .await;

    fs::rename(&file_path, &new_file_path).await.map_err(|e| {
        tracing::error!("Failed to rename file {:?}: {}", file_path, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(extract::Json("success".to_string()))
}

pub async fn rename_folder(
    State(app_state): State<AppState>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    Query(branch_query): Query<BranchQuery>,
    extract::Json(payload): extract::Json<RenameFileRequest>,
) -> Result<extract::Json<String>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let project_root = project_manager.config_manager.project_path().to_path_buf();
    let resolved = project_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let file_path = app_state
        .backend
        .resolve_path(
            branch_query.branch.as_deref(),
            PathBuf::from(&resolved),
            &project_root,
        )
        .await;
    let resolved_new = project_manager
        .config_manager
        .resolve_file(payload.new_name)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let new_file_path = app_state
        .backend
        .resolve_path(
            branch_query.branch.as_deref(),
            PathBuf::from(&resolved_new),
            &project_root,
        )
        .await;

    fs::rename(&file_path, &new_file_path).await.map_err(|e| {
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
    Query(branch_query): Query<BranchQuery>,
    extract::Json(payload): extract::Json<SaveFileRequest>,
) -> Result<extract::Json<String>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let resolved = project_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let project_root = project_manager.config_manager.project_path().to_path_buf();
    let file_path = app_state
        .backend
        .resolve_path(
            branch_query.branch.as_deref(),
            PathBuf::from(&resolved),
            &project_root,
        )
        .await;

    // Ensure parent directory exists (worktree may not have it yet)
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).await.map_err(|e| {
            tracing::error!("Failed to create parent directory {:?}: {}", parent, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }

    fs::write(&file_path, payload.data).await.map_err(|e| {
        tracing::error!("Failed to save file {:?}: {}", file_path, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(extract::Json("success".to_string()))
}

pub async fn get_diff_summary(
    State(app_state): State<AppState>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Query(branch_query): Query<BranchQuery>,
) -> Result<Json<Vec<FileStatus>>, StatusCode> {
    let project_root = project_manager.config_manager.project_path();
    let repo_path = app_state
        .backend
        .worktree_root(branch_query.branch.as_deref(), project_root)
        .await;

    // In local mode, uncommitted working-tree changes take priority so the
    // header button reflects files that still need to be committed.  When the
    // working tree is clean (e.g. the user just pushed), fall back to commits
    // that are ahead of the remote so the "Push N" state is still surfaced.
    let file_statuses = if app_state.backend.is_local() {
        let uncommitted = GitOperations::diff_numstat_summary(&repo_path)
            .await
            .unwrap_or_default();
        if uncommitted.is_empty() {
            LocalGitService::diff_numstat_ahead(&repo_path)
                .await
                .unwrap_or_default()
        } else {
            uncommitted
        }
    } else {
        match GitOperations::diff_numstat_summary(&repo_path).await {
            Ok(statuses) => statuses,
            Err(e) => {
                tracing::error!("{:?}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    };

    Ok(Json(
        file_statuses
            .into_iter()
            .filter(|f| !f.path.starts_with(".worktrees/"))
            .collect(),
    ))
}

pub async fn get_file(
    State(app_state): State<AppState>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    Query(branch_query): Query<BranchQuery>,
) -> Result<extract::Json<String>, StatusCode> {
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let resolved = project_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let project_root = project_manager.config_manager.project_path().to_path_buf();
    let file_path = app_state
        .backend
        .resolve_path(
            branch_query.branch.as_deref(),
            PathBuf::from(&resolved),
            &project_root,
        )
        .await;
    let file_content = fs::read_to_string(&file_path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            StatusCode::NOT_FOUND
        } else {
            tracing::error!("Failed to read file {:?}: {}", file_path, e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;
    Ok(extract::Json(file_content))
}

pub async fn get_file_from_git(
    State(app_state): State<AppState>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    Query(branch_query): Query<BranchQuery>,
) -> Result<extract::Json<String>, StatusCode> {
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let project_root = project_manager.config_manager.project_path();
    let repo_path = app_state
        .backend
        .worktree_root(branch_query.branch.as_deref(), project_root)
        .await;
    let file_path = project_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let relative_file_path = PathBuf::from(&file_path)
        .strip_prefix(project_root)
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .to_string_lossy()
        .to_string();
    let file_content = ProjectService::get_file_from_git(&repo_path, &relative_file_path).await?;
    Ok(extract::Json(file_content))
}

/// Discard working-tree changes for a single file by restoring it to HEAD.
///
/// - Modified / deleted files: `git checkout HEAD -- <path>`
/// - Newly added (untracked) files: `git clean -f -- <path>` (removes the file)
pub async fn revert_file(
    State(app_state): State<AppState>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    Query(branch_query): Query<BranchQuery>,
) -> Result<extract::Json<String>, StatusCode> {
    if app_state.readonly {
        return Err(StatusCode::FORBIDDEN);
    }
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let project_root = project_manager.config_manager.project_path();
    let repo_path = app_state
        .backend
        .worktree_root(branch_query.branch.as_deref(), project_root)
        .await;
    let file_path = project_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let relative_file_path = PathBuf::from(&file_path)
        .strip_prefix(project_root)
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .to_string_lossy()
        .to_string();

    // Try restoring tracked file (modified or deleted)
    let checkout = tokio::process::Command::new("git")
        .args(["checkout", "HEAD", "--", &relative_file_path])
        .current_dir(&repo_path)
        .output()
        .await
        .map_err(|e| {
            tracing::error!("Failed to run git checkout: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if checkout.status.success() {
        return Ok(extract::Json("success".to_string()));
    }

    // File is untracked (newly added) — remove it from the working tree
    let clean = tokio::process::Command::new("git")
        .args(["clean", "-f", "--", &relative_file_path])
        .current_dir(&repo_path)
        .output()
        .await
        .map_err(|e| {
            tracing::error!("Failed to run git clean: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if clean.status.success() {
        return Ok(extract::Json("success".to_string()));
    }

    let stderr = String::from_utf8_lossy(&clean.stderr);
    tracing::error!("Failed to revert file {}: {}", relative_file_path, stderr);
    Err(StatusCode::INTERNAL_SERVER_ERROR)
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

/// Directory names that are always hidden from the file tree.
const HIDDEN_DIRS: &[&str] = &[".git", ".worktrees"];

pub async fn get_file_tree(
    State(app_state): State<AppState>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Query(branch_query): Query<BranchQuery>,
) -> Result<extract::Json<Vec<FileTree>>, StatusCode> {
    let project_root = project_manager.config_manager.project_path();
    let project_root = PathBuf::from(project_root);

    // Serve the worktree directory when a non-default branch is requested.
    let serve_root = app_state
        .backend
        .worktree_root(branch_query.branch.as_deref(), &project_root)
        .await;

    let file_tree =
        tokio::task::spawn_blocking(move || get_file_tree_recursive(&serve_root, &serve_root))
            .await
            .map_err(|e| {
                tracing::error!("File tree task panicked: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    Ok(extract::Json(file_tree.children))
}

fn get_file_tree_recursive(path: &PathBuf, root: &PathBuf) -> FileTree {
    let mut file_tree = FileTree {
        name: path.file_name().unwrap().to_string_lossy().to_string(),
        path: path
            .strip_prefix(root)
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap(),
        is_dir: path.is_dir(),
        children: vec![],
    };
    if path.is_dir()
        && let Ok(entries) = sync_fs::read_dir(path)
    {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            // Skip hidden git/worktree internals
            let name = entry_path.file_name().unwrap_or_default().to_string_lossy();
            if HIDDEN_DIRS.iter().any(|hidden| name == *hidden) {
                continue;
            }
            file_tree
                .children
                .push(get_file_tree_recursive(&entry_path, root));
        }
    }
    file_tree
}
