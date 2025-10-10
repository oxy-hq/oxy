use crate::errors::OxyError;
use crate::github::{GitHubAppAuth, GitHubClient, GitHubRepository, GitOperations};
use crate::service::project::branch_service::BranchService;
use crate::service::project::database_operations::{DatabaseOperations, ValidationUtils};
use entity::prelude::*;
use entity::{branches, projects};
use sea_orm::EntityTrait;
use std::path::{Path, PathBuf};
use tracing::info;
use uuid::Uuid;

pub struct GitService;

impl GitService {
    pub async fn load_project_repo(
        project: &projects::Model,
    ) -> Result<entity::project_repos::Model, OxyError> {
        let project_repo_id = project.project_repo_id.as_ref().ok_or_else(|| {
            OxyError::ConfigurationError("No repository configured for project".to_string())
        })?;

        DatabaseOperations::with_connection(|db| async move {
            entity::project_repos::Entity::find_by_id(*project_repo_id)
                .one(&db)
                .await
                .map_err(|e| DatabaseOperations::wrap_db_error("Failed to find project repo", e))?
                .ok_or_else(|| OxyError::RuntimeError("Project repo not found".to_string()))
        })
        .await
    }

    pub async fn load_git_namespace(
        git_namespace_id: Uuid,
    ) -> Result<entity::git_namespaces::Model, OxyError> {
        DatabaseOperations::with_connection(|db| async move {
            entity::git_namespaces::Entity::find_by_id(git_namespace_id)
                .one(&db)
                .await
                .map_err(|e| DatabaseOperations::wrap_db_error("Failed to find git namespace", e))?
                .ok_or_else(|| OxyError::RuntimeError("Git namespace not found".to_string()))
        })
        .await
    }

    pub async fn load_token_from_git_namespace(git_namespace_id: Uuid) -> Result<String, OxyError> {
        let git_namespace = Self::load_git_namespace(git_namespace_id).await?;
        if !git_namespace.oauth_token.is_empty() {
            return Ok(git_namespace.oauth_token.clone());
        }
        let app_auth = GitHubAppAuth::from_env()?;
        app_auth
            .get_installation_token(&git_namespace.installation_id.to_string())
            .await
    }

    pub async fn require_token(project: &projects::Model) -> Result<String, OxyError> {
        let repo = Self::load_project_repo(project).await?;
        info!("Using GitHub namespace id {}", repo.git_namespace_id);
        Self::load_token_from_git_namespace(repo.git_namespace_id).await
    }

    pub async fn get_project_repo_id(project: &projects::Model) -> Result<i64, OxyError> {
        let repo = Self::load_project_repo(project).await?;
        ValidationUtils::parse_repo_id(&repo.repo_id)
    }

    pub async fn ensure_repo_cloned_and_on_branch(
        repo: &GitHubRepository,
        branch_name: &str,
        project_id: Uuid,
        branch_id: Uuid,
        token: &str,
    ) -> Result<PathBuf, OxyError> {
        let repo_path = GitOperations::get_repository_path(project_id, branch_id)?;

        GitOperations::check_git_availability().await?;
        GitOperations::ensure_git_config().await?;

        if GitOperations::is_git_repository(&repo_path).await {
            let current = GitOperations::get_current_branch(&repo_path).await?;
            if current != branch_name {
                info!("Switching from branch '{}' to '{}'", current, branch_name);
                GitOperations::switch_branch(&repo_path, branch_name, token).await?;
            }
        } else {
            info!(
                "Cloning repository '{}' to {}",
                repo.name,
                repo_path.display()
            );
            GitOperations::clone_repository(
                &repo.clone_url,
                &repo_path,
                Some(branch_name),
                Some(token),
            )
            .await?;
        }
        Ok(repo_path)
    }

    pub async fn latest_commit_for_branch(
        client: &GitHubClient,
        repo_id: i64,
        repo: &GitHubRepository,
        branch_name: &str,
    ) -> Result<String, OxyError> {
        if branch_name == repo.default_branch {
            client.get_latest_commit_hash(repo_id).await
        } else {
            client.get_branch_commit_hash(repo_id, branch_name).await
        }
    }

    pub async fn init_push_repo(project_id: Uuid, branch_id: Uuid) -> Result<(), OxyError> {
        let project = Self::load_project(project_id).await?;
        let branch = Self::load_branch(branch_id).await?;
        let token = Self::require_token(&project).await?;
        let repo_id = Self::get_project_repo_id(&project).await?;
        let client = GitHubClient::from_token(token.clone())?;
        let repo = client.get_repository(repo_id).await?;

        let repo_path = GitOperations::get_repository_path(project_id, branch_id)?;

        info!(
            "Initializing and pushing new repository for project '{}' at {}",
            project.name,
            repo_path.display()
        );

        GitOperations::init_and_push_repository(&repo_path, &repo.clone_url, Some(&token)).await?;

        let latest = Self::latest_commit_for_branch(&client, repo_id, &repo, &branch.name).await?;

        let _ = BranchService::set_branch_status(
            &branch,
            entity::branches::SyncStatus::Synced,
            Some(latest.clone()),
        )
        .await?;

        info!(
            "Successfully initialized and pushed repository for project '{}' to {}",
            project.name, repo.full_name
        );

        Ok(())
    }

    pub async fn sync_project_branch(
        project_id: Uuid,
        branch_id: Uuid,
    ) -> Result<String, OxyError> {
        let project = Self::load_project(project_id).await?;
        let token = Self::require_token(&project).await?;
        let repo_id = Self::get_project_repo_id(&project).await?;

        let client = GitHubClient::from_token(token.clone())?;

        let branch = Self::load_branch(branch_id).await?;
        ValidationUtils::validate_project_branch_relationship(branch.project_id, project_id)?;

        let _ =
            BranchService::set_branch_status(&branch, entity::branches::SyncStatus::Syncing, None)
                .await?;

        info!(
            "Starting sync for project '{}' branch '{}' ({})",
            project.name, branch.name, branch_id
        );

        let repo = client.get_repository(repo_id).await?;
        let repo_path = Self::ensure_repo_cloned_and_on_branch(
            &repo,
            &branch.name,
            project_id,
            branch.id,
            &token,
        )
        .await?;

        GitOperations::pull_repository(&repo_path, Some(&token)).await?;
        let latest = Self::latest_commit_for_branch(&client, repo_id, &repo, &branch.name).await?;

        let updated = BranchService::set_branch_status(
            &branch,
            entity::branches::SyncStatus::Synced,
            Some(latest.clone()),
        )
        .await?;

        info!(
            "Successfully synced project '{}' branch '{}' to commit {}",
            project.name, updated.name, latest
        );
        Ok(latest)
    }

    pub async fn get_file_from_git(repo_path: &Path, file_path: &str) -> Result<String, OxyError> {
        info!("Getting file content from Git");
        GitOperations::get_file_content(repo_path, file_path, None).await
    }

    pub async fn sync_with_remote<F, Fut>(
        project_id: Uuid,
        query_branch: Option<String>,
        operation: F,
    ) -> Result<(), OxyError>
    where
        F: Fn(&Path, &str) -> Fut,
        Fut: std::future::Future<Output = Result<(), OxyError>>,
    {
        let project = Self::load_project(project_id).await?;
        let token = Self::require_token(&project).await?;

        let branch = BranchService::find_branch_by_name_or_active(&project, query_branch).await?;
        let _ =
            BranchService::set_branch_status(&branch, entity::branches::SyncStatus::Syncing, None)
                .await?;

        let repo_path = GitOperations::get_repository_path(project.id, branch.id)?;
        let res = operation(&repo_path, &token).await;

        match res {
            Ok(()) => {
                let current_commit = match GitOperations::get_current_commit_hash(&repo_path).await
                {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("Failed to get current commit hash after operation: {}", e);
                        branch.revision.clone()
                    }
                };
                let updated = BranchService::set_branch_status(
                    &branch,
                    entity::branches::SyncStatus::Synced,
                    Some(current_commit),
                )
                .await?;
                info!(
                    "Successfully synced project '{}' branch '{}'",
                    project.name, updated.name
                );
                Ok(())
            }
            Err(e) => {
                let _ = BranchService::set_branch_status(
                    &branch,
                    entity::branches::SyncStatus::Failed,
                    None,
                )
                .await;
                Err(e)
            }
        }
    }

    pub async fn push_changes(
        project_id: Uuid,
        query_branch: Option<String>,
        commit_message: String,
    ) -> Result<(), OxyError> {
        Self::sync_with_remote(project_id, query_branch, move |path, token| {
            let commit = commit_message.clone();
            let path = path.to_owned();
            let token = token.to_owned();
            async move { GitOperations::push_repository(&path, Some(&token), &commit).await }
        })
        .await
    }

    pub async fn pull_changes(
        project_id: Uuid,
        query_branch: Option<String>,
    ) -> Result<(), OxyError> {
        Self::sync_with_remote(project_id, query_branch, |path, token| {
            let path = path.to_owned();
            let token = token.to_owned();
            async move { GitOperations::pull_repository(&path, Some(&token)).await }
        })
        .await
    }

    // Private helper methods
    async fn load_project(project_id: Uuid) -> Result<projects::Model, OxyError> {
        DatabaseOperations::with_connection(|db| async move {
            Projects::find_by_id(project_id)
                .one(&db)
                .await
                .map_err(|e| DatabaseOperations::wrap_db_error("Failed to find project", e))?
                .ok_or_else(|| OxyError::RuntimeError("Project not found".to_string()))
        })
        .await
    }

    async fn load_branch(branch_id: Uuid) -> Result<branches::Model, OxyError> {
        DatabaseOperations::with_connection(|db| async move {
            Branches::find_by_id(branch_id)
                .one(&db)
                .await
                .map_err(|e| DatabaseOperations::wrap_db_error("Failed to find branch", e))?
                .ok_or_else(|| OxyError::RuntimeError("Branch not found".to_string()))
        })
        .await
    }
}
