use crate::api::project::{BranchType, ProjectBranch, RevisionInfoResponse};
use crate::errors::OxyError;
use crate::github::GitHubClient;
use crate::github::{GitHubAppAuth, GitOperations};
use crate::service::project::database_operations::DatabaseOperations;
use entity::prelude::*;
use entity::{branches, projects};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use tracing::info;
use uuid::Uuid;

pub struct BranchService;

impl BranchService {
    pub async fn find_branch_by_name_or_active(
        project: &projects::Model,
        query_branch: Option<String>,
    ) -> Result<branches::Model, OxyError> {
        DatabaseOperations::with_connection(|db| async move {
            match query_branch {
                Some(name) => Branches::find()
                    .filter(entity::branches::Column::ProjectId.eq(project.id))
                    .filter(entity::branches::Column::Name.eq(name.clone()))
                    .one(&db)
                    .await
                    .map_err(|e| {
                        DatabaseOperations::wrap_db_error("Database error when fetching branch", e)
                    })?
                    .ok_or_else(|| OxyError::RuntimeError("Branch not found".to_string())),
                None => Branches::find_by_id(project.active_branch_id)
                    .one(&db)
                    .await
                    .map_err(|e| {
                        DatabaseOperations::wrap_db_error("Database error when fetching branch", e)
                    })?
                    .ok_or_else(|| {
                        OxyError::RuntimeError(format!(
                            "Active branch {} not found",
                            project.active_branch_id
                        ))
                    }),
            }
        })
        .await
    }

    pub async fn set_branch_status(
        branch: &branches::Model,
        status: entity::branches::SyncStatus,
        revision: Option<String>,
    ) -> Result<branches::Model, OxyError> {
        DatabaseOperations::with_connection(|db| async move {
            let mut m: branches::ActiveModel = branch.clone().into();
            m.sync_status = Set(status.as_str().to_string());
            if let Some(r) = revision {
                m.revision = Set(r);
            }
            m.updated_at = Set(DatabaseOperations::now().into());
            m.update(&db)
                .await
                .map_err(|e| DatabaseOperations::wrap_db_error("Failed to update branch", e))
        })
        .await
    }

    pub async fn get_project_branches(project_id: Uuid) -> Result<Vec<ProjectBranch>, OxyError> {
        let locals = Self::get_local_branches(project_id).await?;
        let mut out: Vec<ProjectBranch> = locals
            .into_iter()
            .map(|b| ProjectBranch {
                id: b.id,
                project_id: b.project_id,
                branch_type: BranchType::Local,
                name: b.name,
                revision: b.revision,
                sync_status: b.sync_status,
                created_at: b.created_at.to_string(),
                updated_at: b.updated_at.to_string(),
            })
            .collect();

        if let Ok(remotes) = Self::fetch_github_branches(project_id).await {
            Self::merge_github_branches(&mut out, remotes);
        }
        Ok(out)
    }

    pub async fn switch_project_branch(
        project_id: Uuid,
        branch_name: String,
    ) -> Result<branches::Model, OxyError> {
        let existing = Self::find_existing_branch(project_id, &branch_name).await?;

        if let Some(branch) = existing {
            return Ok(branch);
        }

        info!(
            "Branch '{}' not found for project '{}', creating new branch",
            branch_name, project_id
        );

        Self::create_new_branch(project_id, branch_name).await
    }

    pub async fn switch_project_active_branch(
        project_id: Uuid,
        branch_name: String,
    ) -> Result<branches::Model, OxyError> {
        DatabaseOperations::with_connection(|db| async move {
            let project = Projects::find_by_id(project_id)
                .one(&db)
                .await
                .map_err(|e| DatabaseOperations::wrap_db_error("Failed to find project", e))?
                .ok_or_else(|| OxyError::RuntimeError("Project not found".to_string()))?;

            if let Some(active) = Branches::find_by_id(project.active_branch_id)
                .one(&db)
                .await
                .map_err(|e| DatabaseOperations::wrap_db_error("Failed to find branch", e))?
                && active.name == branch_name
            {
                return Ok(active);
            }

            let branch = Self::switch_project_branch(project_id, branch_name).await?;

            let mut pm: projects::ActiveModel = project.into();
            pm.active_branch_id = Set(branch.id);
            pm.updated_at = Set(DatabaseOperations::now().into());
            pm.update(&db)
                .await
                .map_err(|e| DatabaseOperations::wrap_db_error("Failed to update project", e))?;
            Ok(branch)
        })
        .await
    }

    pub async fn get_revision_info(
        project: &projects::Model,
        client: &GitHubClient,
        repo_id: i64,
        query_branch: Option<String>,
    ) -> Result<RevisionInfoResponse, OxyError> {
        let branch = Self::find_branch_by_name_or_active(project, query_branch).await?;

        let latest_hash = client.get_branch_commit_hash(repo_id, &branch.name).await?;
        let latest_commit = client.get_commit_details(repo_id, &latest_hash).await?;

        let repo_path = GitOperations::get_repository_path(project.id, branch.id)?;
        let current_commit_sha = GitOperations::get_current_commit_hash(&repo_path).await?;
        let current_commit = client
            .get_commit_details(repo_id, &current_commit_sha)
            .await?;

        Ok(RevisionInfoResponse {
            current_revision: Some(branch.revision),
            latest_revision: Some(latest_hash),
            current_commit: Some(current_commit),
            latest_commit: Some(latest_commit),
            sync_status: branch.sync_status,
            last_sync_time: Some(branch.updated_at.to_string()),
        })
    }

    // Private helper methods
    async fn get_local_branches(project_id: Uuid) -> Result<Vec<branches::Model>, OxyError> {
        DatabaseOperations::with_connection(|db| async move {
            Branches::find()
                .filter(branches::Column::ProjectId.eq(project_id))
                .all(&db)
                .await
                .map_err(|e| DatabaseOperations::wrap_db_error("Failed to get branches", e))
        })
        .await
    }

    async fn fetch_github_branches(project_id: Uuid) -> Result<Vec<ProjectBranch>, OxyError> {
        let project = Self::load_project(project_id).await?;

        // Load project repo
        let project_repo_id = project.project_repo_id.as_ref().ok_or_else(|| {
            OxyError::ConfigurationError("No repository configured for project".to_string())
        })?;

        let project_repo = DatabaseOperations::with_connection(|db| async move {
            entity::project_repos::Entity::find_by_id(*project_repo_id)
                .one(&db)
                .await
                .map_err(|e| DatabaseOperations::wrap_db_error("Failed to find project repo", e))?
                .ok_or_else(|| OxyError::RuntimeError("Project repo not found".to_string()))
        })
        .await?;

        let repo_id: i64 = project_repo
            .repo_id
            .parse()
            .map_err(|_| OxyError::ConfigurationError("Invalid repository ID".to_string()))?;

        // Load git namespace and token
        let git_namespace = DatabaseOperations::with_connection(|db| async move {
            entity::git_namespaces::Entity::find_by_id(project_repo.git_namespace_id)
                .one(&db)
                .await
                .map_err(|e| DatabaseOperations::wrap_db_error("Failed to find git namespace", e))?
                .ok_or_else(|| OxyError::RuntimeError("Git namespace not found".to_string()))
        })
        .await?;

        let app_auth = GitHubAppAuth::from_env()?;
        let token = app_auth
            .get_installation_token(&git_namespace.installation_id.to_string())
            .await?;

        let client = GitHubClient::from_token(token)?;
        let repo = client.get_repository(repo_id).await?;
        let gh_branches = client.list_branches(repo.full_name.clone()).await?;

        let mut out = Vec::with_capacity(gh_branches.len());
        for gh in gh_branches {
            if let Ok(rev) = client.get_branch_commit_hash(repo_id, &gh.name).await {
                out.push(ProjectBranch {
                    id: Uuid::new_v4(),
                    project_id,
                    branch_type: BranchType::Remote,
                    name: gh.name.clone(),
                    revision: rev,
                    sync_status: entity::branches::SyncStatus::Pending.as_str().to_string(),
                    created_at: DatabaseOperations::now().to_string(),
                    updated_at: DatabaseOperations::now().to_string(),
                });
            }
        }
        Ok(out)
    }

    fn merge_github_branches(local: &mut Vec<ProjectBranch>, remote: Vec<ProjectBranch>) {
        for rb in remote {
            if !local.iter().any(|b| b.name == rb.name) {
                info!("Found new GitHub branch '{}' (pending)", rb.name);
                local.push(rb);
            }
        }
    }

    async fn find_existing_branch(
        project_id: Uuid,
        branch_name: &str,
    ) -> Result<Option<branches::Model>, OxyError> {
        DatabaseOperations::with_connection(|db| async move {
            Branches::find()
                .filter(branches::Column::ProjectId.eq(project_id))
                .filter(branches::Column::Name.eq(branch_name))
                .one(&db)
                .await
                .map_err(|e| DatabaseOperations::wrap_db_error("Failed to find branch", e))
        })
        .await
    }

    async fn create_new_branch(
        project_id: Uuid,
        branch_name: String,
    ) -> Result<branches::Model, OxyError> {
        let new_b = DatabaseOperations::with_connection(|db| async move {
            branches::ActiveModel {
                id: Set(Uuid::new_v4()),
                project_id: Set(project_id),
                name: Set(branch_name.clone()),
                revision: Set(String::new()),
                sync_status: Set(entity::branches::SyncStatus::Pending.as_str().to_string()),
                created_at: Set(DatabaseOperations::now().into()),
                updated_at: Set(DatabaseOperations::now().into()),
            }
            .insert(&db)
            .await
            .map_err(|e| DatabaseOperations::wrap_db_error("Failed to create branch", e))
        })
        .await?;

        // Note: Sync operation will be handled by the caller to avoid circular dependency
        Ok(new_b)
    }

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
}
