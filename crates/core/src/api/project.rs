use entity::settings::SyncStatus;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::config::ConfigBuilder;
use crate::github::{GitHubRepository, GitHubService};
use crate::project::resolve_project_path;
use crate::readonly::is_readonly_mode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStatus {
    pub github_connected: bool, // token input and verified
    pub repository: Option<GitHubRepository>,
    pub repository_sync_status: Option<SyncStatus>,
    pub required_secrets: Option<Vec<String>>,
    pub is_config_valid: bool, // true if the config.yml is valid
    pub is_readonly: bool,     // true if the project is read-only
    pub is_onboarded: bool,    // true if the user has completed onboarding
}

pub async fn get_project_status() -> Result<axum::response::Json<ProjectStatus>, StatusCode> {
    info!("Getting overall project status");

    // Check GitHub connection and get repository information
    let (github_connected, repository, is_onboarded, sync_status) =
        match GitHubService::get_settings().await {
            Ok(Some(settings)) => {
                let onboarded = settings.is_onboarded;
                let sync_status = Some(settings.sync_status);
                if let Some(repo_id) = settings.selected_repo_id {
                    // Create a GitHub client from the token to get repository details
                    match crate::github::client::GitHubClient::new(settings.github_token) {
                        Ok(client) => match client.get_repository(repo_id).await {
                            Ok(repo) => (true, Some(repo), onboarded, sync_status),
                            Err(_) => (true, None, onboarded, sync_status), // Token is valid but repo fetch failed
                        },
                        Err(_) => (false, None, onboarded, sync_status), // Invalid token
                    }
                } else {
                    (true, None, onboarded, sync_status) // Token exists but no repo selected
                }
            }
            Ok(None) => (false, None, false, None), // No GitHub settings found
            Err(e) => {
                error!("Failed to get GitHub settings: {}", e);
                (false, None, false, None)
            }
        };

    // Check config validity and required secrets
    let (is_config_valid, required_secrets) = match resolve_project_path() {
        Ok(project_path) => {
            match ConfigBuilder::new()
                .with_project_path(&project_path)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .build()
                .await
            {
                Ok(config) => {
                    let secrets = match config.get_required_secrets().await {
                        Ok(secrets) => secrets,
                        Err(e) => {
                            error!("Failed to get required secrets: {}", e);
                            None
                        }
                    };
                    (true, secrets) // Config is valid if we can build it successfully
                }
                Err(e) => {
                    error!("Failed to build config: {}", e);
                    (false, None) // Config is invalid if building fails
                }
            }
        }
        Err(e) => {
            error!("Failed to resolve project path: {}", e);
            (false, None) // Config is invalid if project path cannot be resolved
        }
    };

    let status = ProjectStatus {
        github_connected,
        repository,
        required_secrets,
        is_config_valid,
        is_readonly: is_readonly_mode(),
        is_onboarded,
        repository_sync_status: sync_status,
    };

    Ok(axum::response::Json(status))
}
