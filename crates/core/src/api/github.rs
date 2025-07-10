use crate::github::{CurrentProject, GitHubRepository, GitHubService};
use axum::{extract::Json, http::StatusCode, response::Json as ResponseJson};
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use utoipa::ToSchema;

// Request/Response types
#[derive(Debug, Deserialize)]
pub struct StoreTokenRequest {
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct SelectRepositoryRequest {
    pub repository_id: i64,
}

#[derive(Debug, Serialize)]
pub struct SelectRepositoryResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ListRepositoriesResponse {
    pub repositories: Vec<GitHubRepository>,
}

#[derive(Debug, Serialize)]
pub struct PullResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GitHubSettingsResponse {
    /// Whether GitHub token is configured
    pub token_configured: bool,
    /// Selected repository ID
    pub selected_repo_id: Option<i64>,
    /// Repository name (owner/repo)
    pub repository_name: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RevisionInfoResponse {
    /// Current local revision/commit hash
    pub current_revision: Option<String>,
    /// Latest revision on main/default branch
    pub latest_revision: Option<String>,
    /// Detailed information about current commit
    pub current_commit: Option<crate::github::CommitInfo>,
    /// Detailed information about latest commit
    pub latest_commit: Option<crate::github::CommitInfo>,
    /// Current sync status
    pub sync_status: String,
    /// Last sync time
    pub last_sync_time: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateGitHubSettingsRequest {
    /// GitHub token (will be encrypted)
    pub token: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetOnboardedRequest {
    /// Whether onboarding is completed
    pub onboarded: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SettingsUpdateResponse {
    pub success: bool,
    pub message: String,
}

/// Store and validate GitHub token
pub async fn store_token(
    Json(request): Json<StoreTokenRequest>,
) -> Result<ResponseJson<TokenResponse>, StatusCode> {
    info!("Storing GitHub token");

    match GitHubService::store_token(&request.token).await {
        Ok(_) => {
            info!("GitHub token stored successfully");
            Ok(ResponseJson(TokenResponse {
                success: true,
                message: "GitHub token stored and validated successfully".to_string(),
            }))
        }
        Err(e) => {
            error!("Failed to store GitHub token: {}", e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

/// List accessible GitHub repositories
pub async fn list_repositories() -> Result<ResponseJson<ListRepositoriesResponse>, StatusCode> {
    info!("Listing GitHub repositories");

    match GitHubService::list_repositories().await {
        Ok(repositories) => {
            info!("Found {} repositories", repositories.len());
            Ok(ResponseJson(ListRepositoriesResponse { repositories }))
        }
        Err(e) => {
            error!("Failed to list repositories: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Select a repository
pub async fn select_repository(
    Json(request): Json<SelectRepositoryRequest>,
) -> Result<ResponseJson<SelectRepositoryResponse>, StatusCode> {
    info!("Selecting repository with ID: {}", request.repository_id);

    match GitHubService::select_repository(request.repository_id).await {
        Ok(_) => {
            info!("Repository selection started successfully");
            Ok(ResponseJson(SelectRepositoryResponse {
                success: true,
                message: "Repository selection started. Cloning in background.".to_string(),
            }))
        }
        Err(e) => {
            error!("Failed to select repository: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get current project information
pub async fn get_current_project() -> Result<ResponseJson<CurrentProject>, StatusCode> {
    info!("Getting current project information");

    match GitHubService::get_current_project().await {
        Ok(project) => {
            info!(
                "Current project retrieved: {:?}",
                project.repository.as_ref().map(|r| &r.name)
            );
            Ok(ResponseJson(project))
        }
        Err(e) => {
            error!("Failed to get current project: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Pull latest changes for the current repository
pub async fn pull_repository() -> Result<ResponseJson<PullResponse>, StatusCode> {
    info!("Pulling latest changes for current repository");

    match GitHubService::pull_current_repository().await {
        Ok(message) => {
            info!("Repository pull successful: {}", message);
            Ok(ResponseJson(PullResponse {
                success: true,
                message,
            }))
        }
        Err(e) => {
            error!("Failed to pull repository: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get GitHub settings information
pub async fn get_github_settings() -> Result<ResponseJson<GitHubSettingsResponse>, StatusCode> {
    info!("Getting GitHub settings");

    let github_response = match GitHubService::get_settings().await {
        Ok(Some(settings)) => {
            let repository_name = if let Some(repo_id) = settings.selected_repo_id {
                GitHubService::get_repository_name(repo_id).await.ok()
            } else {
                None
            };

            GitHubSettingsResponse {
                token_configured: true,
                selected_repo_id: settings.selected_repo_id,
                repository_name,
            }
        }
        Ok(None) => GitHubSettingsResponse {
            token_configured: false,
            selected_repo_id: None,
            repository_name: None,
        },
        Err(e) => {
            error!("Failed to get settings: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    Ok(ResponseJson(github_response))
}

/// Get revision information for the current repository
pub async fn get_revision_info() -> Result<ResponseJson<RevisionInfoResponse>, StatusCode> {
    info!("Getting revision information");

    let settings = GitHubService::get_settings()
        .await
        .map_err(|e| {
            error!("Failed to get settings: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::BAD_REQUEST)?;

    let repo_id = settings.selected_repo_id.ok_or(StatusCode::BAD_REQUEST)?;

    // Get commit information concurrently
    let (latest_commit, latest_commit_details, current_commit_details, last_sync_time) = tokio::join!(
        async { GitHubService::get_latest_remote_commit(repo_id).await.ok() },
        async {
            GitHubService::get_latest_remote_commit_details(repo_id)
                .await
                .ok()
        },
        async {
            GitHubService::get_current_commit_details(repo_id)
                .await
                .ok()
        },
        async {
            GitHubService::check_sync_status()
                .await
                .map(|(_, last_sync, _)| last_sync)
                .unwrap_or(None)
        }
    );

    Ok(ResponseJson(RevisionInfoResponse {
        current_revision: settings.revision,
        latest_revision: latest_commit,
        current_commit: current_commit_details,
        latest_commit: latest_commit_details,
        sync_status: format!("{:?}", settings.sync_status).to_lowercase(),
        last_sync_time,
    }))
}

/// Update GitHub settings (primarily the token)
pub async fn update_github_settings(
    Json(request): Json<UpdateGitHubSettingsRequest>,
) -> Result<ResponseJson<SettingsUpdateResponse>, StatusCode> {
    info!("Updating GitHub settings");

    let Some(token) = request.token else {
        return Ok(ResponseJson(SettingsUpdateResponse {
            success: true,
            message: "No changes to apply".to_string(),
        }));
    };

    GitHubService::store_token(&token).await.map_err(|e| {
        error!("Failed to update GitHub token: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("GitHub token updated successfully");
    Ok(ResponseJson(SettingsUpdateResponse {
        success: true,
        message: "GitHub settings updated successfully".to_string(),
    }))
}

/// Sync the current repository to the latest revision
pub async fn sync_github_repository() -> Result<ResponseJson<SettingsUpdateResponse>, StatusCode> {
    info!("Syncing GitHub repository to latest revision");

    let settings = GitHubService::get_settings().await.map_err(|e| {
        error!("Failed to get settings: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let Some(settings) = settings else {
        return Ok(ResponseJson(SettingsUpdateResponse {
            success: false,
            message: "No GitHub configuration found".to_string(),
        }));
    };

    let Some(repo_id) = settings.selected_repo_id else {
        return Ok(ResponseJson(SettingsUpdateResponse {
            success: false,
            message: "No repository selected".to_string(),
        }));
    };

    match GitHubService::sync_repository_to_latest(repo_id).await {
        Ok(message) => {
            info!("Repository sync completed successfully");
            Ok(ResponseJson(SettingsUpdateResponse {
                success: true,
                message,
            }))
        }
        Err(e) => {
            error!("Failed to sync repository: {}", e);
            Ok(ResponseJson(SettingsUpdateResponse {
                success: false,
                message: format!("Failed to sync repository: {e}"),
            }))
        }
    }
}

/// Set onboarded status
pub async fn set_onboarded(
    Json(request): Json<SetOnboardedRequest>,
) -> Result<ResponseJson<SettingsUpdateResponse>, StatusCode> {
    info!("Setting onboarded status to: {}", request.onboarded);

    GitHubService::set_onboarded(request.onboarded)
        .await
        .map_err(|e| {
            error!("Failed to set onboarded status: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    info!("Onboarded status updated successfully");
    Ok(ResponseJson(SettingsUpdateResponse {
        success: true,
        message: format!("Onboarded status set to {}", request.onboarded),
    }))
}
