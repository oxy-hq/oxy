use crate::server::api::middlewares::role_guards::WorkspaceEditor;
use crate::server::api::middlewares::workspace_context::WorkspaceManagerExtractor;
use crate::server::router::AppState;
use axum::Json;
use axum::extract::{self, Path, State};
use axum::http::StatusCode;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use futures::TryFutureExt;
use oxy::github::default_git_client;
use oxy_git::{FileStatus, GitClient};
use oxy_project::data_repo_service::{parse_data_repo_path, resolve_data_repo_path};
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
    _: WorkspaceEditor,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path((_workspace_id, pathb64)): Path<(Uuid, String)>,
) -> Result<extract::Json<String>, StatusCode> {
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let resolved = workspace_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let file_path = PathBuf::from(&resolved);

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
    _: WorkspaceEditor,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path((_workspace_id, pathb64)): Path<(Uuid, String)>,
) -> Result<extract::Json<String>, StatusCode> {
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let resolved = workspace_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let file_path = PathBuf::from(&resolved);
    fs::create_dir_all(&file_path).await.map_err(|e| {
        tracing::error!("Failed to create folder {:?}: {}", file_path, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(extract::Json("success".to_string()))
}

pub async fn delete_file(
    _: WorkspaceEditor,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path((_workspace_id, pathb64)): Path<(Uuid, String)>,
) -> Result<extract::Json<String>, StatusCode> {
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let resolved = workspace_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let file_path = PathBuf::from(&resolved);
    fs::remove_file(&file_path).await.map_err(|e| {
        tracing::error!("Failed to delete file {:?}: {}", file_path, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(extract::Json("success".to_string()))
}

pub async fn delete_folder(
    _: WorkspaceEditor,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path((_workspace_id, pathb64)): Path<(Uuid, String)>,
) -> Result<extract::Json<String>, StatusCode> {
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let resolved = workspace_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let file_path = PathBuf::from(&resolved);
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
    _: WorkspaceEditor,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path((_workspace_id, pathb64)): Path<(Uuid, String)>,
    extract::Json(payload): extract::Json<RenameFileRequest>,
) -> Result<extract::Json<String>, StatusCode> {
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let resolved = workspace_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let file_path = PathBuf::from(&resolved);
    let resolved_new = workspace_manager
        .config_manager
        .resolve_file(payload.new_name)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let new_file_path = PathBuf::from(&resolved_new);

    fs::rename(&file_path, &new_file_path).await.map_err(|e| {
        tracing::error!("Failed to rename file {:?}: {}", file_path, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(extract::Json("success".to_string()))
}

pub async fn rename_folder(
    _: WorkspaceEditor,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path((_workspace_id, pathb64)): Path<(Uuid, String)>,
    extract::Json(payload): extract::Json<RenameFileRequest>,
) -> Result<extract::Json<String>, StatusCode> {
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let resolved = workspace_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let file_path = PathBuf::from(&resolved);
    let resolved_new = workspace_manager
        .config_manager
        .resolve_file(payload.new_name)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let new_file_path = PathBuf::from(&resolved_new);

    fs::rename(&file_path, &new_file_path).await.map_err(|e| {
        tracing::error!("Failed to rename folder {:?}: {}", file_path, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(extract::Json("success".to_string()))
}

#[axum::debug_handler]
pub async fn save_file(
    _: WorkspaceEditor,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path((_workspace_id, pathb64)): Path<(Uuid, String)>,
    extract::Json(payload): extract::Json<SaveFileRequest>,
) -> Result<extract::Json<String>, StatusCode> {
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Data repo files are rooted outside the workspace and need the workspace
    // path to resolve their `@repo/...` prefix.
    if parse_data_repo_path(&path).is_some() {
        let workspace_root = workspace_manager
            .config_manager
            .workspace_path()
            .to_path_buf();
        let file_path =
            resolve_file_path(&workspace_manager.config_manager, &workspace_root, &path).await?;
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                tracing::error!("Failed to create parent dir {:?}: {}", parent, e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }
        fs::write(&file_path, payload.data).await.map_err(|e| {
            tracing::error!("Failed to save data repo file {:?}: {}", file_path, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        return Ok(extract::Json("success".to_string()));
    }

    let resolved = workspace_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let file_path = PathBuf::from(&resolved);

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
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
) -> Result<Json<Vec<FileStatus>>, StatusCode> {
    let repo_path = workspace_manager.config_manager.workspace_path();

    // Uncommitted working-tree changes take priority so the header button
    // reflects files that still need to be committed. When the working tree
    // is clean (e.g. the user just pushed), fall back to commits that are
    // ahead of the remote so the "Push N" state is still surfaced.
    let uncommitted = default_git_client()
        .diff_numstat_summary(&repo_path)
        .await
        .unwrap_or_default();
    let file_statuses = if uncommitted.is_empty() {
        default_git_client()
            .diff_numstat_ahead(&repo_path)
            .await
            .unwrap_or_default()
    } else {
        uncommitted
    };

    Ok(Json(
        file_statuses
            .into_iter()
            .filter(|f| !f.path.starts_with(".worktrees/"))
            .collect(),
    ))
}

pub async fn get_file(
    State(_app_state): State<AppState>,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path((_workspace_id, pathb64)): Path<(Uuid, String)>,
) -> Result<extract::Json<String>, StatusCode> {
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Data repo paths live outside the workspace and resolve via `@repo/...`.
    if parse_data_repo_path(&path).is_some() {
        let workspace_root = workspace_manager
            .config_manager
            .workspace_path()
            .to_path_buf();
        let file_path =
            resolve_file_path(&workspace_manager.config_manager, &workspace_root, &path).await?;
        let content = fs::read_to_string(&file_path).await.map_err(|e| {
            tracing::error!("Failed to read data repo file {:?}: {}", file_path, e);
            StatusCode::NOT_FOUND
        })?;
        return Ok(extract::Json(content));
    }

    let resolved = workspace_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let file_path = PathBuf::from(&resolved);
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
    State(_app_state): State<AppState>,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path((_workspace_id, pathb64)): Path<(Uuid, String)>,
) -> Result<extract::Json<String>, StatusCode> {
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let repo_path = workspace_manager.config_manager.workspace_path();
    let file_path = workspace_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let relative_file_path = PathBuf::from(&file_path)
        .strip_prefix(repo_path)
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .to_string_lossy()
        .to_string();
    let file_content = default_git_client()
        .file_at_rev(repo_path, &relative_file_path, None)
        .await?;
    Ok(extract::Json(file_content))
}

/// Discard working-tree changes for a single file by restoring it to HEAD.
///
/// - Modified / deleted files: `git checkout HEAD -- <path>`
/// - Newly added (untracked) files: `git clean -f -- <path>` (removes the file)
pub async fn revert_file(
    _: WorkspaceEditor,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path((_workspace_id, pathb64)): Path<(Uuid, String)>,
) -> Result<extract::Json<String>, StatusCode> {
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?;
    let repo_path = workspace_manager.config_manager.workspace_path();
    let file_path = workspace_manager
        .config_manager
        .resolve_file(path)
        .map_err(|_| StatusCode::BAD_REQUEST)
        .await?;
    let relative_file_path = PathBuf::from(&file_path)
        .strip_prefix(repo_path)
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .to_string_lossy()
        .to_string();

    // Try restoring tracked file (modified or deleted)
    let checkout = tokio::process::Command::new("git")
        .args(["checkout", "HEAD", "--", &relative_file_path])
        .current_dir(repo_path)
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
        .current_dir(repo_path)
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

#[derive(Serialize, Deserialize, Clone, ToSchema, Debug)]
pub struct RepoTree {
    pub name: String,
    /// "ready" once the clone directory contains a .git folder; "cloning" while the background
    /// clone is still in progress; "error" if the path is unresolvable.
    pub sync_status: String,
    /// The git remote URL for the repository, if configured. Used by the IDE to build a GitHub link.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_url: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, ToSchema, Debug)]
pub struct FileTreeResponse {
    pub primary: Vec<FileTree>,
    pub repositories: Vec<RepoTree>,
}

/// Directory names that are always hidden from the file tree.
const HIDDEN_DIRS: &[&str] = &[".git", ".worktrees", ".repositories"];

pub async fn get_file_tree(
    State(_app_state): State<AppState>,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
) -> Result<extract::Json<FileTreeResponse>, StatusCode> {
    let workspace_root = PathBuf::from(workspace_manager.config_manager.workspace_path());
    let serve_root = workspace_root.clone();

    let primary_tree =
        tokio::task::spawn_blocking(move || get_file_tree_recursive(&serve_root, &serve_root))
            .await
            .map_err(|e| {
                tracing::error!("File tree task panicked: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    // Return only repo stubs (name + sync_status); files are fetched lazily per repo.
    // Use async stat() so we don't block the executor thread for N repos.
    let config = workspace_manager.config_manager.get_config();
    let repo_trees: Vec<RepoTree> = {
        let futs: Vec<_> = config
            .repositories
            .iter()
            .map(|r| {
                let name = r.name.clone();
                let git_url = r.git_url.clone();
                let git_dot = if r.git_url.is_some() {
                    Some(
                        workspace_root
                            .join(".repositories")
                            .join(&r.name)
                            .join(".git"),
                    )
                } else {
                    None
                };
                async move {
                    let sync_status = match git_dot {
                        Some(p) => {
                            if tokio::fs::try_exists(&p).await.unwrap_or(false) {
                                "ready"
                            } else {
                                "cloning"
                            }
                        }
                        None => "ready",
                    };
                    RepoTree {
                        name,
                        sync_status: sync_status.to_string(),
                        git_url,
                    }
                }
            })
            .collect();
        futures::future::join_all(futs).await
    };

    Ok(extract::Json(FileTreeResponse {
        primary: primary_tree.children,
        repositories: repo_trees,
    }))
}

/// Resolves a decoded file path to an absolute filesystem path.
/// Handles both regular project paths and `@repo-name/relative/path` repository paths.
async fn resolve_file_path(
    workspace_manager: &oxy::config::ConfigManager,
    workspace_root: &PathBuf,
    decoded_path: &str,
) -> Result<PathBuf, StatusCode> {
    if let Some((repo_name, file_path)) = parse_data_repo_path(decoded_path) {
        let config = workspace_manager.get_config();
        let repo = config
            .repositories
            .iter()
            .find(|r| r.name == repo_name)
            .ok_or(StatusCode::NOT_FOUND)?;
        let repo_root = resolve_data_repo_path(workspace_root, repo).map_err(|e| {
            tracing::error!("Repository '{}' unavailable: {}", repo_name, e);
            StatusCode::NOT_FOUND
        })?;
        // Prevent path traversal
        let joined = repo_root.join(file_path);
        if !joined.starts_with(&repo_root) {
            return Err(StatusCode::BAD_REQUEST);
        }
        Ok(joined)
    } else {
        let resolved = workspace_manager
            .resolve_file(decoded_path)
            .map_err(|_| StatusCode::BAD_REQUEST)
            .await?;
        Ok(PathBuf::from(resolved))
    }
}

pub(crate) fn get_file_tree_recursive(path: &PathBuf, root: &PathBuf) -> FileTree {
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
