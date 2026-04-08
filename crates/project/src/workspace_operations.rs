use crate::branch_service::BranchService;
use crate::database_operations::DatabaseOperations;
use crate::git_service::GitService;
use entity::prelude::*;
use entity::{branches, workspace_repos, workspaces};
use oxy::api_types::{ProjectBranch, RevisionInfoResponse};
use oxy::github::GitHubClient;
use oxy_shared::errors::OxyError;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Set};
use std::path::Path;
use tracing::info;
use uuid::Uuid;

pub struct WorkspaceService;

impl WorkspaceService {
    pub async fn get_project(project_id: Uuid) -> Result<Option<workspaces::Model>, OxyError> {
        DatabaseOperations::with_connection(|db| async move {
            Workspaces::find_by_id(project_id)
                .one(&db)
                .await
                .map_err(|e| DatabaseOperations::wrap_db_error("Failed to get workspace", e))
        })
        .await
    }

    pub async fn get_branch(branch_id: Uuid) -> Result<Option<branches::Model>, OxyError> {
        DatabaseOperations::with_connection(|db| async move {
            Branches::find_by_id(branch_id)
                .one(&db)
                .await
                .map_err(|e| DatabaseOperations::wrap_db_error("Failed to get branch", e))
        })
        .await
    }

    pub async fn delete_project(project_id: Uuid) -> Result<(), OxyError> {
        DatabaseOperations::with_connection(|db| async move {
            Branches::delete_many()
                .filter(branches::Column::ProjectId.eq(project_id))
                .exec(&db)
                .await
                .map_err(|e| {
                    DatabaseOperations::wrap_db_error("Failed to delete workspace branches", e)
                })?;

            Workspaces::delete_by_id(project_id)
                .exec(&db)
                .await
                .map_err(|e| DatabaseOperations::wrap_db_error("Failed to delete workspace", e))?;
            Ok(())
        })
        .await
    }

    pub async fn get_project_branches(project_id: Uuid) -> Result<Vec<ProjectBranch>, OxyError> {
        BranchService::get_project_branches(project_id).await
    }

    pub async fn switch_project_branch(
        project_id: Uuid,
        branch_name: String,
    ) -> Result<branches::Model, OxyError> {
        let branch = BranchService::switch_project_branch(project_id, branch_name).await?;

        if branch.revision.is_empty() {
            GitService::sync_project_branch(project_id, branch.id).await?;
        }

        Ok(branch)
    }

    pub async fn switch_project_active_branch(
        project_id: Uuid,
        branch_name: String,
    ) -> Result<branches::Model, OxyError> {
        BranchService::switch_project_active_branch(project_id, branch_name).await
    }

    pub async fn sync_project_branch(
        project_id: Uuid,
        branch_id: Uuid,
    ) -> Result<String, OxyError> {
        GitService::sync_project_branch(project_id, branch_id).await
    }

    pub async fn get_revision_info(
        project_id: Uuid,
        query_branch: Option<String>,
    ) -> Result<RevisionInfoResponse, OxyError> {
        info!("Getting revision information");
        let project = Self::load_project(project_id).await?;
        let token = GitService::require_token(&project).await?;
        let client = GitHubClient::from_token(token)?;
        let repo_id = GitService::get_project_repo_id(&project).await?;

        BranchService::get_revision_info(&project, &client, repo_id, query_branch).await
    }

    pub async fn get_file_from_git(repo_path: &Path, file_path: &str) -> Result<String, OxyError> {
        GitService::get_file_from_git(repo_path, file_path).await
    }

    pub async fn push_changes(
        project_id: Uuid,
        query_branch: Option<String>,
        commit_message: String,
    ) -> Result<(), OxyError> {
        GitService::push_changes(project_id, query_branch, commit_message).await
    }

    pub async fn pull_changes(
        project_id: Uuid,
        query_branch: Option<String>,
    ) -> Result<(), OxyError> {
        GitService::pull_changes(project_id, query_branch).await
    }

    pub async fn create_repo_from_project(
        project_id: Uuid,
        git_namespace_id: Uuid,
        repo_name: String,
    ) -> Result<(), OxyError> {
        info!("Creating repository from workspace: {}", project_id);

        let project = Self::load_project(project_id).await?;
        let project_clone = project.clone();

        if project.project_repo_id.is_some() {
            return Err(OxyError::RuntimeError(
                "Workspace already has a repository configured".to_string(),
            ));
        }

        let git_namespace = GitService::load_git_namespace(git_namespace_id).await?;
        let token = GitService::load_token_from_git_namespace(git_namespace_id).await?;

        let client = GitHubClient::from_token(token)?;
        let created_repo = client
            .create_repository(
                &repo_name,
                None,
                Some(false),
                Some(&git_namespace.owner_type),
                Some(&git_namespace.name),
            )
            .await?;

        info!(
            "Created GitHub repository '{}' with ID: {}",
            created_repo.name, created_repo.id
        );

        let project_repo_id = Uuid::new_v4();
        DatabaseOperations::with_connection(|db| async move {
            let project_repo = workspace_repos::ActiveModel {
                id: Set(project_repo_id),
                repo_id: Set(created_repo.id.to_string()),
                git_namespace_id: Set(git_namespace_id),
                created_at: Set(DatabaseOperations::now().into()),
                updated_at: Set(DatabaseOperations::now().into()),
            };

            WorkspaceRepos::insert(project_repo)
                .exec(&db)
                .await
                .map_err(|e| {
                    DatabaseOperations::wrap_db_error("Failed to create workspace repo", e)
                })?;

            let mut project_update: workspaces::ActiveModel = project.into();
            project_update.project_repo_id = Set(Some(project_repo_id));
            project_update.updated_at = Set(DatabaseOperations::now().into());

            Workspaces::update(project_update)
                .exec(&db)
                .await
                .map_err(|e| {
                    DatabaseOperations::wrap_db_error("Failed to update workspace with repo", e)
                })?;

            Ok(())
        })
        .await?;

        info!(
            "Successfully linked workspace '{}' to repository '{}'",
            project_clone.name, created_repo.full_name
        );
        GitService::init_push_repo(project_id, project_clone.active_branch_id).await?;

        info!("Successfully initialized and pushed initial repository content");

        Ok(())
    }

    async fn load_project(project_id: Uuid) -> Result<workspaces::Model, OxyError> {
        DatabaseOperations::with_connection(|db| async move {
            Workspaces::find_by_id(project_id)
                .one(&db)
                .await
                .map_err(|e| DatabaseOperations::wrap_db_error("Failed to find workspace", e))?
                .ok_or_else(|| OxyError::RuntimeError("Workspace not found".to_string()))
        })
        .await
    }
}
