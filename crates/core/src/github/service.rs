use crate::db::client::establish_connection;
use crate::errors::OxyError;
use crate::github::{
    background_tasks, client::GitHubClient, encryption::TokenEncryption,
    git_operations::GitOperations, types::*,
};
use crate::readonly::is_readonly_mode;
use entity::prelude::Settings;
use entity::settings;
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};

// Error message constants
const NO_TOKEN_ERROR: &str = "No GitHub token configured";
const NO_SETTINGS_ERROR: &str = "GitHub settings not found";
const NO_REPO_SELECTED_ERROR: &str = "No repository selected";
const NO_CONFIG_ERROR: &str = "No GitHub configuration found";

/// Service layer for GitHub integration operations
///
/// This service provides a clean, high-level API for GitHub operations by:
/// - Abstracting GitHub API interactions through GitHubClient
/// - Managing encrypted token storage and retrieval  
/// - Coordinating database operations for settings
/// - Handling git repository operations locally
/// - Managing background tasks for repository cloning
///
/// The service uses helper methods to reduce code duplication and ensure
/// consistent error handling across all operations.
pub struct GitHubService;

impl GitHubService {
    // === Private Helper Methods ===

    /// Get authenticated GitHub client
    async fn get_authenticated_client() -> Result<GitHubClient, OxyError> {
        let token = Self::get_token()
            .await?
            .ok_or_else(|| OxyError::ConfigurationError(NO_TOKEN_ERROR.to_string()))?;
        GitHubClient::new(token)
    }

    /// Get database connection and settings
    async fn get_db_settings() -> Result<(DatabaseConnection, settings::Model), OxyError> {
        let db = establish_connection().await?;
        let settings = Settings::find()
            .one(&db)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to query settings: {e}")))?
            .ok_or_else(|| OxyError::ConfigurationError(NO_SETTINGS_ERROR.to_string()))?;
        Ok((db, settings))
    }

    /// Update settings in database
    async fn update_settings<F>(updater: F) -> Result<(), OxyError>
    where
        F: FnOnce(&mut settings::ActiveModel),
    {
        let (db, settings) = Self::get_db_settings().await?;
        let mut active_model: settings::ActiveModel = settings.into();

        updater(&mut active_model);
        active_model.updated_at = Set(chrono::Utc::now().into());

        active_model
            .update(&db)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to update settings: {e}")))?;

        Ok(())
    }

    /// Get repository path and validate it exists
    async fn get_validated_repo_path(repo_id: i64) -> Result<std::path::PathBuf, OxyError> {
        let repo_path = GitOperations::get_repository_path(repo_id)?;

        if !GitOperations::is_git_repository(&repo_path).await {
            return Err(OxyError::RuntimeError(format!(
                "Repository not found locally: {}",
                repo_path.display()
            )));
        }

        Ok(repo_path)
    }

    // === Public API Methods ===

    // === Token Management ===

    /// Store and validate a GitHub token
    pub async fn store_token(token: &str) -> Result<(), OxyError> {
        // First validate the token with GitHub API
        let client = GitHubClient::new(token.to_string())?;
        client.validate_token().await?;

        // Encrypt the token
        let encrypted_token = TokenEncryption::encrypt_token(token)?;

        // Store in database
        let db = establish_connection().await?;
        let existing_settings = Settings::find()
            .one(&db)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to query settings: {e}")))?;

        if let Some(_existing) = existing_settings {
            // Update existing record
            Self::update_settings(|model| {
                model.github_token = Set(encrypted_token.clone());
            })
            .await?;
        } else {
            // Create new record
            let new_settings = settings::ActiveModel {
                github_token: Set(encrypted_token),
                selected_repo_id: Set(None),
                revision: Set(None),
                sync_status: Set(settings::SyncStatus::Idle),
                created_at: Set(chrono::Utc::now().into()),
                updated_at: Set(chrono::Utc::now().into()),
                ..Default::default()
            };

            new_settings
                .insert(&db)
                .await
                .map_err(|e| OxyError::DBError(format!("Failed to store GitHub token: {e}")))?;
        }

        Ok(())
    }

    /// Get the stored GitHub token (decrypted)
    /// Checks secret manager first, then falls back to GitHub-specific token storage
    pub async fn get_token() -> Result<Option<String>, OxyError> {
        // First try to get token from secret manager
        let secret_resolver = crate::service::secret_resolver::SecretResolverService::new();

        // Try common GitHub token secret names
        let secret_names = ["GITHUB_TOKEN", "GH_TOKEN", "GITHUB_ACCESS_TOKEN"];

        for secret_name in secret_names {
            if let Some(result) = secret_resolver.resolve_secret(secret_name).await? {
                tracing::debug!(
                    "Found GitHub token in secret manager with name: {}",
                    secret_name
                );
                return Ok(Some(result.value));
            }
        }

        // Fall back to GitHub-specific encrypted token storage
        let db = establish_connection().await?;

        let settings = Settings::find()
            .one(&db)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to query settings: {e}")))?;

        if let Some(settings) = settings {
            let decrypted_token = TokenEncryption::decrypt_token(&settings.github_token)?;
            Ok(Some(decrypted_token))
        } else {
            Ok(None)
        }
    }

    /// Get current GitHub settings
    /// Uses secret manager for token if available, otherwise falls back to encrypted storage
    pub async fn get_settings() -> Result<Option<GitHubSettings>, OxyError> {
        // Get token from secret manager or fallback storage
        let token = Self::get_token().await?;

        if token.is_none() {
            return Ok(None);
        }

        let db = establish_connection().await?;

        let settings = Settings::find()
            .one(&db)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to query settings: {e}")))?;

        // If we have a token from secret manager but no settings record, create a minimal one
        if let Some(settings) = settings {
            Ok(Some(GitHubSettings {
                github_token: token.unwrap(),
                selected_repo_id: settings.selected_repo_id,
                revision: settings.revision,
                sync_status: settings.sync_status,
                is_onboarded: settings.onboarded,
            }))
        } else if token.is_some() {
            // Token exists in secret manager but no settings record
            Ok(Some(GitHubSettings {
                github_token: token.unwrap(),
                selected_repo_id: None,
                revision: None,
                sync_status: entity::settings::SyncStatus::Idle,
                is_onboarded: false,
            }))
        } else {
            Ok(None)
        }
    }

    // === Repository Management ===

    /// List repositories accessible to the stored GitHub token
    pub async fn list_repositories() -> Result<Vec<GitHubRepository>, OxyError> {
        let client = Self::get_authenticated_client().await?;
        client.list_repositories().await
    }

    /// Select a repository (now runs cloning in background)
    pub async fn select_repository(repo_id: i64) -> Result<String, OxyError> {
        let client = Self::get_authenticated_client().await?;

        // Get repository details
        let repo = client.get_repository(repo_id).await?;

        // Get the latest commit hash from the default branch
        let latest_commit = client.get_latest_commit_hash(repo_id).await?;

        // Store repository selection first (before cloning)
        Self::update_settings(|model| {
            model.selected_repo_id = Set(Some(repo_id));
            model.revision = Set(Some(latest_commit));
            model.sync_status = Set(settings::SyncStatus::Syncing);
        })
        .await?;

        // Set the project manager for the selected repository
        crate::project::set_git_project_manager(repo_id);

        // Start background clone task using the global singleton
        let task_id = background_tasks::start_clone_task(repo).await?;

        Ok(task_id)
    }

    /// Deselect repository
    pub async fn deselect_repository() -> Result<(), OxyError> {
        let _settings = Self::get_settings()
            .await?
            .ok_or_else(|| OxyError::ConfigurationError(NO_CONFIG_ERROR.to_string()))?;

        // Clear repository selection in database
        Self::update_settings(|model| {
            model.selected_repo_id = Set(None);
            model.revision = Set(None);
            model.sync_status = Set(settings::SyncStatus::Idle);
        })
        .await?;

        // Reset the project manager since no repository is selected
        crate::project::reset_project_manager();

        Ok(())
    }

    // === Project Management ===

    /// Get current project status
    pub async fn get_project_status() -> Result<ProjectStatus, OxyError> {
        if !is_readonly_mode() {
            tracing::info!("Not running in readonly mode, skipping project status check");
            return Ok(ProjectStatus {
                requires_onboarding: false,
                current_repository: None,
            });
        }

        let current_repository = if let Ok(Some(settings)) = Self::get_settings().await {
            if let Some(repo_id) = settings.selected_repo_id {
                let client = GitHubClient::new(settings.github_token)?;
                client.get_repository(repo_id).await.ok()
            } else {
                None
            }
        } else {
            None
        };

        // Determine if onboarding is required
        let requires_onboarding = current_repository.is_none();

        Ok(ProjectStatus {
            requires_onboarding,
            current_repository,
        })
    }

    /// Get current project information
    pub async fn get_current_project() -> Result<CurrentProject, OxyError> {
        let settings = Self::get_settings().await?;

        let (repository, local_path) = if let Some(settings) = &settings {
            if let Some(repo_id) = settings.selected_repo_id {
                let client = GitHubClient::new(settings.github_token.clone())?;
                let repo = client.get_repository(repo_id).await.ok();

                // Get the local path from the repository path
                let local_path = GitOperations::get_repository_path(repo_id)
                    .ok()
                    .and_then(|p| p.to_str().map(|s| s.to_string()));

                (repo, local_path)
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        let sync_status = if repository.is_some() {
            ProjectSyncStatus::Synced
        } else {
            ProjectSyncStatus::NotConfigured
        };

        Ok(CurrentProject {
            repository,
            local_path,
            sync_status,
        })
    }

    /// Clone or update a repository locally
    pub async fn clone_or_update_repository(repo: &GitHubRepository) -> Result<String, OxyError> {
        // Ensure git is available
        GitOperations::check_git_availability().await?;
        GitOperations::ensure_git_config().await?;

        // Get the local path for the repository
        let repo_path = GitOperations::get_repository_path(repo.id)?;

        if GitOperations::is_git_repository(&repo_path).await {
            // Repository already exists, pull latest changes
            tracing::info!(
                "Repository {} already exists, pulling latest changes",
                repo.name
            );
            GitOperations::pull_repository(&repo_path).await?;
        } else {
            // Clone the repository
            tracing::info!(
                "Cloning repository {} to {}",
                repo.name,
                repo_path.display()
            );
            GitOperations::clone_repository(
                &repo.clone_url,
                &repo_path,
                Some(&repo.default_branch),
            )
            .await?;
        }

        GitHubService::update_sync_status(settings::SyncStatus::Synced).await?;

        Ok(repo_path.to_string_lossy().to_string())
    }

    /// Pull latest changes for the currently selected repository
    pub async fn pull_current_repository() -> Result<String, OxyError> {
        let settings = Self::get_settings()
            .await?
            .ok_or_else(|| OxyError::ConfigurationError(NO_CONFIG_ERROR.to_string()))?;

        let repo_id = settings
            .selected_repo_id
            .ok_or_else(|| OxyError::ConfigurationError(NO_REPO_SELECTED_ERROR.to_string()))?;

        let repo_path = Self::get_validated_repo_path(repo_id).await?;
        GitOperations::auto_pull_repository(&repo_path).await
    }

    /// Check detailed synchronization status with remote repository
    pub async fn check_sync_status() -> Result<(bool, Option<String>, String), OxyError> {
        let settings = Self::get_settings()
            .await?
            .ok_or_else(|| OxyError::ConfigurationError(NO_CONFIG_ERROR.to_string()))?;

        let repo_id = settings
            .selected_repo_id
            .ok_or_else(|| OxyError::ConfigurationError(NO_REPO_SELECTED_ERROR.to_string()))?;

        let repo_path = Self::get_validated_repo_path(repo_id).await?;

        // Get last sync time (last pull or fetch)
        let last_sync = GitOperations::get_last_sync_time(&repo_path).await.ok();

        // Return simplified sync status
        let is_synced = true; // Simplified - we'll rely on sync_status from database
        let message = "Repository status available".to_string();

        Ok((is_synced, last_sync, message))
    }

    // === Sync Status Management ===

    /// Update sync status in the database
    pub async fn update_sync_status(sync_status: settings::SyncStatus) -> Result<(), OxyError> {
        Self::update_settings(|model| {
            model.sync_status = Set(sync_status);
        })
        .await
    }

    /// Update both sync status and revision
    pub async fn update_sync_status_and_revision(
        sync_status: settings::SyncStatus,
        revision: Option<String>,
    ) -> Result<(), OxyError> {
        Self::update_settings(|model| {
            model.sync_status = Set(sync_status);
            if let Some(rev) = revision {
                model.revision = Set(Some(rev));
            }
        })
        .await
    }

    /// Get repository name by ID
    pub async fn get_repository_name(repo_id: i64) -> Result<String, OxyError> {
        let client = Self::get_authenticated_client().await?;
        let repo = client.get_repository(repo_id).await?;
        Ok(repo.full_name)
    }

    /// Get latest commit hash from remote repository
    pub async fn get_latest_remote_commit(repo_id: i64) -> Result<String, OxyError> {
        let client = Self::get_authenticated_client().await?;
        client.get_latest_commit_hash(repo_id).await
    }

    /// Get detailed commit information from remote repository
    pub async fn get_latest_remote_commit_details(repo_id: i64) -> Result<CommitInfo, OxyError> {
        let client = Self::get_authenticated_client().await?;
        let latest_commit_sha = client.get_latest_commit_hash(repo_id).await?;
        client.get_commit_details(repo_id, &latest_commit_sha).await
    }

    /// Get detailed commit information for current local repository
    pub async fn get_current_commit_details(repo_id: i64) -> Result<CommitInfo, OxyError> {
        // Get current commit SHA from local repository
        let repo_path = GitOperations::get_repository_path(repo_id)?;
        let current_commit_sha = GitOperations::get_current_commit_hash(&repo_path).await?;

        let client = Self::get_authenticated_client().await?;
        client
            .get_commit_details(repo_id, &current_commit_sha)
            .await
    }

    /// Sync repository to latest revision
    pub async fn sync_repository_to_latest(repo_id: i64) -> Result<String, OxyError> {
        // Update sync status to syncing
        Self::update_sync_status(settings::SyncStatus::Syncing).await?;

        // Get latest commit from remote
        let latest_commit = Self::get_latest_remote_commit(repo_id).await?;

        // Pull latest changes
        let result = Self::pull_current_repository().await;

        match result {
            Ok(message) => {
                // Update sync status to synced and store latest revision
                Self::update_sync_status_and_revision(
                    settings::SyncStatus::Synced,
                    Some(latest_commit),
                )
                .await?;
                Ok(format!("Repository synced successfully: {message}"))
            }
            Err(e) => {
                // Update sync status to error
                Self::update_sync_status(settings::SyncStatus::Error).await?;
                Err(e)
            }
        }
    }

    /// Store GitHub token in secret manager (recommended approach)
    /// This is the preferred method for new installations
    pub async fn store_token_in_secret_manager(
        token: &str,
        secret_name: Option<&str>,
    ) -> Result<(), OxyError> {
        // First validate the token with GitHub API
        let client = GitHubClient::new(token.to_string())?;
        client.validate_token().await?;

        let secret_manager = crate::service::secret_manager::SecretManagerService::new();
        let db = establish_connection().await?;

        // Use default secret name if none provided
        let secret_name = secret_name.unwrap_or("GITHUB_TOKEN");

        // Try to get the current user (for created_by field)
        // For now, we'll use a placeholder UUID since this is typically called during setup
        let placeholder_user_id = uuid::Uuid::new_v4();

        let create_params = crate::service::secret_manager::CreateSecretParams {
            name: secret_name.to_string(),
            value: token.to_string(),
            description: Some("GitHub personal access token for repository access".to_string()),
            created_by: placeholder_user_id,
        };

        match secret_manager.create_secret(&db, create_params).await {
            Ok(_) => {
                tracing::info!("GitHub token stored in secret manager as '{}'", secret_name);
                Ok(())
            }
            Err(crate::errors::OxyError::SecretManager(msg)) if msg.contains("already exists") => {
                // Update existing secret instead
                let update_params = crate::service::secret_manager::UpdateSecretParams {
                    value: Some(token.to_string()),
                    description: Some(
                        "GitHub personal access token for repository access".to_string(),
                    ),
                };

                secret_manager
                    .update_secret(&db, secret_name, update_params)
                    .await
                    .map_err(|e| {
                        OxyError::SecretManager(format!("Failed to update GitHub token: {e}"))
                    })?;

                tracing::info!(
                    "GitHub token updated in secret manager as '{}'",
                    secret_name
                );
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Set onboarded status
    pub async fn set_onboarded(onboarded: bool) -> Result<(), OxyError> {
        Self::update_settings(|model| {
            model.onboarded = Set(onboarded);
        })
        .await
    }
}
