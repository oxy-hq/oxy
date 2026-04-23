use axum::{Json, extract::Path, http::StatusCode, response::Json as ResponseJson};
use oxy::config::model::Repository;
use oxy::database::client::establish_connection;
use oxy::github::{default_git_client, github_token_for_namespace};
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_git::{FileStatus, GitClient};
use oxy_project::data_repo_service::resolve_data_repo_path;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use tracing::error;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::server::api::middlewares::role_guards::{WorkspaceAdmin, WorkspaceEditor};
use crate::server::api::middlewares::workspace_context::WorkspaceManagerExtractor;

/// Validate a repository name to prevent path traversal attacks.
///
/// Only ASCII alphanumerics, hyphens, and underscores are allowed — the same
/// character set enforced by `sanitize_project_name` in onboarding.rs.
fn validate_repo_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DataRepoResponse {
    pub name: String,
    pub path: Option<String>,
    pub git_url: Option<String>,
    pub branch: Option<String>,
    pub git_namespace_id: Option<String>,
}

impl From<&Repository> for DataRepoResponse {
    fn from(r: &Repository) -> Self {
        Self {
            name: r.name.clone(),
            path: r.path.clone(),
            git_url: r.git_url.clone(),
            branch: r.branch.clone(),
            git_namespace_id: r.git_namespace_id.clone(),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AddDataRepoRequest {
    pub name: String,
    /// Local path (relative to project root or absolute)
    pub path: Option<String>,
    /// Git URL to clone
    pub git_url: Option<String>,
    /// Branch for git URL repos (optional)
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AddRepoFromGitHubRequest {
    /// Display name / path prefix (e.g. "dbt-models")
    pub name: String,
    /// UUID of the GitNamespace row (for token refresh)
    pub git_namespace_id: String,
    /// HTTPS clone URL from the GitHub API (e.g. "https://github.com/acme/dbt.git")
    pub clone_url: String,
    /// Branch to check out (defaults to the repo's default branch)
    pub branch: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RepoBranchResponse {
    pub branch: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RepoBranchesResponse {
    pub branches: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CheckoutBranchRequest {
    pub branch: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RepoCommitResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CommitRepoRequest {
    pub message: String,
}

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Resolves the repo filesystem path for a named repository, returning 404 if not found.
fn resolve_repo(
    workspace_manager: &oxy::adapters::workspace::manager::WorkspaceManager,
    name: &str,
) -> Result<std::path::PathBuf, StatusCode> {
    let workspace_root =
        std::path::PathBuf::from(workspace_manager.config_manager.workspace_path());
    let config = workspace_manager.config_manager.get_config();
    let repo = config
        .repositories
        .iter()
        .find(|r| r.name == name)
        .ok_or(StatusCode::NOT_FOUND)?;
    resolve_data_repo_path(&workspace_root, repo).map_err(|e| {
        error!("Failed to resolve repository path for '{}': {}", name, e);
        StatusCode::NOT_FOUND
    })
}

/// Look up the org_id for a given workspace.
async fn get_workspace_org_id(workspace_id: Uuid) -> Result<Option<Uuid>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        error!("DB connection error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let workspace = entity::workspaces::Entity::find_by_id(workspace_id)
        .one(&db)
        .await
        .map_err(|e| {
            error!("DB query error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(workspace.org_id)
}

async fn get_namespace_token(
    namespace_id_str: &str,
    org_id: Option<Uuid>,
) -> Result<String, StatusCode> {
    let namespace_id = Uuid::parse_str(namespace_id_str).map_err(|_| StatusCode::BAD_REQUEST)?;

    let db = establish_connection().await.map_err(|e| {
        error!("DB connection error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut query = entity::git_namespaces::Entity::find()
        .filter(entity::git_namespaces::Column::Id.eq(namespace_id));

    if let Some(oid) = org_id {
        query = query.filter(entity::git_namespaces::Column::OrgId.eq(oid));
    }

    let namespace = query
        .one(&db)
        .await
        .map_err(|e| {
            error!("DB query error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    github_token_for_namespace(&namespace).await.map_err(|e| {
        error!("Failed to load token from git namespace: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

// ─── handlers ────────────────────────────────────────────────────────────────

pub async fn list_repositories(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path(_workspace_id): Path<Uuid>,
) -> ResponseJson<Vec<DataRepoResponse>> {
    let repos: Vec<DataRepoResponse> = workspace_manager
        .config_manager
        .list_repositories()
        .iter()
        .map(DataRepoResponse::from)
        .collect();
    ResponseJson(repos)
}

pub async fn add_repository(
    _: WorkspaceAdmin,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path(_workspace_id): Path<Uuid>,
    Json(body): Json<AddDataRepoRequest>,
) -> Result<ResponseJson<DataRepoResponse>, StatusCode> {
    if !validate_repo_name(&body.name) {
        return Err(StatusCode::BAD_REQUEST);
    }

    if body.path.is_none() && body.git_url.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let repo = Repository {
        name: body.name,
        path: body.path,
        git_url: body.git_url,
        branch: body.branch,
        git_namespace_id: None,
    };

    workspace_manager
        .config_manager
        .add_repository(repo.clone())
        .await
        .map_err(|e| {
            error!("Failed to add repository: {}", e);
            StatusCode::CONFLICT
        })?;

    Ok(ResponseJson(DataRepoResponse::from(&repo)))
}

pub async fn add_repo_from_github(
    _: WorkspaceAdmin,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Path(workspace_id): Path<Uuid>,
    Json(body): Json<AddRepoFromGitHubRequest>,
) -> Result<ResponseJson<DataRepoResponse>, StatusCode> {
    if !validate_repo_name(&body.name)
        || body.clone_url.is_empty()
        || body.git_namespace_id.is_empty()
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Get a fresh token to authenticate the clone.
    let org_id = get_workspace_org_id(workspace_id).await?;
    let token = get_namespace_token(&body.git_namespace_id, org_id).await?;

    let repo = Repository {
        name: body.name.clone(),
        git_url: Some(body.clone_url.clone()),
        branch: body.branch.clone(),
        git_namespace_id: Some(body.git_namespace_id.clone()),
        path: None,
    };

    // Save config first so the IDE sidebar can show the repo immediately.
    workspace_manager
        .config_manager
        .add_repository(repo.clone())
        .await
        .map_err(|e| {
            error!("Failed to save repository config: {}", e);
            StatusCode::CONFLICT
        })?;

    // Ensure .repositories/ is gitignored so the main project git doesn't track cloned repos.
    let workspace_root =
        std::path::PathBuf::from(workspace_manager.config_manager.workspace_path());
    let gitignore_path = workspace_root.join(".gitignore");
    let entry = ".repositories/\n";
    let already_ignored = tokio::fs::read_to_string(&gitignore_path)
        .await
        .map(|s| s.contains(".repositories/"))
        .unwrap_or(false);
    if !already_ignored {
        match tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&gitignore_path)
            .await
        {
            Ok(mut f) => {
                use tokio::io::AsyncWriteExt;
                if let Err(e) = f.write_all(entry.as_bytes()).await {
                    tracing::warn!("Could not write .gitignore: {}", e);
                }
            }
            Err(e) => tracing::warn!("Could not open .gitignore: {}", e),
        }
    }

    // Clone in the background — large repos can take longer than request timeout.
    let repo_name = body.name.clone();
    let clone_url = body.clone_url.clone();
    let branch = body.branch.clone().unwrap_or_else(|| "HEAD".to_string());

    tokio::spawn(async move {
        let dest = workspace_root.join(".repositories").join(&repo_name);
        match default_git_client()
            .clone_or_init(&dest, Some(&clone_url), &branch, Some(&token))
            .await
        {
            Ok(()) => tracing::info!("Linked repository '{}' cloned successfully", repo_name),
            Err(e) => tracing::error!("Failed to clone linked repository '{}': {}", repo_name, e),
        }
    });

    Ok(ResponseJson(DataRepoResponse::from(&repo)))
}

pub async fn remove_repository(
    _: WorkspaceAdmin,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path((_workspace_id, name)): Path<(Uuid, String)>,
) -> Result<StatusCode, StatusCode> {
    workspace_manager
        .config_manager
        .remove_repository(&name)
        .await
        .map_err(|e| {
            error!("Failed to remove repository: {}", e);
            StatusCode::NOT_FOUND
        })?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_repo_branch(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path((_workspace_id, name)): Path<(Uuid, String)>,
) -> Result<ResponseJson<RepoBranchResponse>, StatusCode> {
    let repo_path = resolve_repo(&workspace_manager, &name)?;
    let branch = default_git_client()
        .get_current_branch(&repo_path)
        .await
        .unwrap_or_else(|_| "HEAD".to_string());
    Ok(ResponseJson(RepoBranchResponse { branch }))
}

pub async fn get_repo_diff(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path((_workspace_id, name)): Path<(Uuid, String)>,
) -> Result<ResponseJson<Vec<FileStatus>>, StatusCode> {
    let repo_path = resolve_repo(&workspace_manager, &name)?;
    let diff = default_git_client()
        .diff_numstat_summary(&repo_path)
        .await
        .unwrap_or_default();
    Ok(ResponseJson(diff))
}

pub async fn commit_repo(
    _: WorkspaceEditor,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Path((workspace_id, name)): Path<(Uuid, String)>,
    Json(body): Json<CommitRepoRequest>,
) -> Result<ResponseJson<RepoCommitResponse>, StatusCode> {
    if body.message.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let repo_path = resolve_repo(&workspace_manager, &name)?;

    // Resolve a fresh token for push if this repo was linked via GitHub App.
    let org_id = get_workspace_org_id(workspace_id).await?;
    let token: Option<String> = {
        let config = workspace_manager.config_manager.get_config();
        if let Some(repo) = config.repositories.iter().find(|r| r.name == name) {
            if let Some(ns_id) = &repo.git_namespace_id {
                match get_namespace_token(ns_id, org_id).await {
                    Ok(t) => Some(t),
                    Err(_) => {
                        tracing::warn!(
                            "Could not refresh token for repository '{}', push may fail",
                            name
                        );
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        }
    };

    // Append co-author trailer so the committer appears in the git log.
    // Strip newlines from OAuth-sourced fields to prevent trailer injection.
    let safe_name = user.name.replace(['\n', '\r'], "");
    let safe_email = user.email.replace(['\n', '\r'], "");
    let commit_message = format!(
        "{}\n\nCo-authored-by: {} <{}>",
        body.message.trim(),
        safe_name,
        safe_email
    );

    // Stage + commit
    let git = default_git_client();
    let sha = git
        .commit_changes(&repo_path, &commit_message)
        .await
        .map_err(|e| {
            error!("Failed to commit repository '{}': {}", name, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if sha.is_empty() {
        return Ok(ResponseJson(RepoCommitResponse {
            success: true,
            message: "Nothing to commit".to_string(),
        }));
    }

    // Push with the fresh token (or without if not a GitHub App repo).
    let push_result = git.push_to_remote(&repo_path, token.as_deref()).await;

    let msg = match push_result {
        Ok(_) => format!("Committed and pushed ({})", sha.trim()),
        Err(_) => format!(
            "Committed {} (push skipped — no remote configured)",
            sha.trim()
        ),
    };

    Ok(ResponseJson(RepoCommitResponse {
        success: true,
        message: msg,
    }))
}

pub async fn get_repo_file_tree(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path((_workspace_id, name)): Path<(Uuid, String)>,
) -> Result<ResponseJson<Vec<crate::server::api::file::FileTree>>, StatusCode> {
    let repo_path = resolve_repo(&workspace_manager, &name)?;
    if !repo_path.exists() {
        // Clone still in progress; return empty list rather than an error.
        return Ok(ResponseJson(vec![]));
    }
    let tree = tokio::task::spawn_blocking(move || {
        crate::server::api::file::get_file_tree_recursive(&repo_path, &repo_path)
    })
    .await
    .map_err(|e| {
        error!("File tree task panicked for repo '{}': {}", name, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(ResponseJson(tree.children))
}

pub async fn list_repo_branches(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Path((workspace_id, name)): Path<(Uuid, String)>,
) -> Result<ResponseJson<RepoBranchesResponse>, StatusCode> {
    let repo_path = resolve_repo(&workspace_manager, &name)?;
    if !repo_path.exists() {
        return Ok(ResponseJson(RepoBranchesResponse { branches: vec![] }));
    }

    // Resolve a token for authenticated fetch (GitHub App repos).
    let org_id = get_workspace_org_id(workspace_id).await?;
    let token: Option<String> = {
        let config = workspace_manager.config_manager.get_config();
        if let Some(repo) = config.repositories.iter().find(|r| r.name == name) {
            if let Some(ns_id) = &repo.git_namespace_id {
                get_namespace_token(ns_id, org_id).await.ok()
            } else {
                None
            }
        } else {
            None
        }
    };

    let branches = default_git_client()
        .list_all_branches(&repo_path, token.as_deref())
        .await
        .map_err(|e| {
            error!("Failed to list branches for repo '{}': {}", name, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(ResponseJson(RepoBranchesResponse { branches }))
}

pub async fn checkout_repo_branch(
    _: WorkspaceEditor,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Path((workspace_id, name)): Path<(Uuid, String)>,
    Json(body): Json<CheckoutBranchRequest>,
) -> Result<StatusCode, StatusCode> {
    if body.branch.trim().is_empty() || body.branch.starts_with('-') {
        return Err(StatusCode::BAD_REQUEST);
    }

    let repo_path = resolve_repo(&workspace_manager, &name)?;

    let org_id = get_workspace_org_id(workspace_id).await?;
    let token: Option<String> = {
        let config = workspace_manager.config_manager.get_config();
        if let Some(repo) = config.repositories.iter().find(|r| r.name == name) {
            if let Some(ns_id) = &repo.git_namespace_id {
                get_namespace_token(ns_id, org_id).await.ok()
            } else {
                None
            }
        } else {
            None
        }
    };

    default_git_client()
        .checkout_branch(&repo_path, &body.branch, token.as_deref())
        .await
        .map_err(|e| {
            error!(
                "Failed to checkout branch '{}' for repo '{}': {}",
                body.branch, name, e
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(StatusCode::NO_CONTENT)
}
