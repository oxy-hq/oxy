use std::path::Path;

use crate::api::project::{BranchType, ProjectBranch, RevisionInfoResponse};
use crate::db::client::establish_connection;
use crate::errors::OxyError;
use crate::github::{GitHubClient, GitHubRepository, GitOperations, encryption::TokenEncryption};
use entity::prelude::*;
use entity::{branches, projects};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use tracing::{info, warn};
use uuid::Uuid;

pub struct ProjectService;

impl ProjectService {
    #[inline]
    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now()
    }

    async fn db() -> Result<DatabaseConnection, OxyError> {
        establish_connection().await
    }

    fn wrap_db_err<E: std::fmt::Display>(msg: &str, e: E) -> OxyError {
        OxyError::DBError(format!("{}: {}", msg, e))
    }

    async fn with_db<F, Fut, T>(f: F) -> Result<T, OxyError>
    where
        F: FnOnce(DatabaseConnection) -> Fut,
        Fut: std::future::Future<Output = Result<T, OxyError>>,
    {
        let db = Self::db().await?;
        f(db).await
    }

    async fn load_project(project_id: Uuid) -> Result<projects::Model, OxyError> {
        Self::with_db(|db| async move {
            Projects::find_by_id(project_id)
                .one(&db)
                .await
                .map_err(|e| Self::wrap_db_err("Failed to find project", e))?
                .ok_or_else(|| OxyError::RuntimeError("Project not found".to_string()))
        })
        .await
    }

    fn parse_repo_id(repo_id_str: &str) -> Result<i64, OxyError> {
        repo_id_str
            .parse::<i64>()
            .map_err(|_| OxyError::ConfigurationError("Invalid repository ID".to_string()))
    }

    fn require_token(project: &projects::Model) -> Result<String, OxyError> {
        let encrypted = project.token.as_ref().ok_or_else(|| {
            OxyError::ConfigurationError("No GitHub token configured for project".to_string())
        })?;
        TokenEncryption::decrypt_token(encrypted)
    }

    fn project_repo_id(project: &projects::Model) -> Result<i64, OxyError> {
        let repo_id_str = project.repo_id.as_ref().ok_or_else(|| {
            OxyError::ConfigurationError("No repository configured for project".to_string())
        })?;
        Self::parse_repo_id(repo_id_str)
    }

    fn client_from_token(token: String) -> Result<GitHubClient, OxyError> {
        GitHubClient::new(token)
    }

    fn github_client_and_token(
        project: &projects::Model,
    ) -> Result<(GitHubClient, String), OxyError> {
        let token = Self::require_token(project)?;
        let client = Self::client_from_token(token.clone())?;
        Ok((client, token))
    }

    async fn find_branch_by_name_or_active(
        db: &DatabaseConnection,
        project: &projects::Model,
        query_branch: Option<String>,
    ) -> Result<branches::Model, OxyError> {
        match query_branch {
            Some(name) => Branches::find()
                .filter(entity::branches::Column::ProjectId.eq(project.id))
                .filter(entity::branches::Column::Name.eq(name.clone()))
                .one(db)
                .await
                .map_err(|e| Self::wrap_db_err("Database error when fetching branch", e))?
                .ok_or_else(|| OxyError::RuntimeError("Branch not found".to_string())),
            None => Branches::find_by_id(project.active_branch_id)
                .one(db)
                .await
                .map_err(|e| Self::wrap_db_err("Database error when fetching branch", e))?
                .ok_or_else(|| {
                    OxyError::RuntimeError(format!(
                        "Active branch {} not found",
                        project.active_branch_id
                    ))
                }),
        }
    }

    async fn set_branch_status(
        db: &DatabaseConnection,
        branch: &branches::Model,
        status: entity::branches::SyncStatus,
        revision: Option<String>,
    ) -> Result<branches::Model, OxyError> {
        let mut m: branches::ActiveModel = branch.clone().into();
        m.sync_status = Set(status.as_str().to_string());
        if let Some(r) = revision {
            m.revision = Set(r);
        }
        m.updated_at = Set(Self::now().into());
        m.update(db)
            .await
            .map_err(|e| Self::wrap_db_err("Failed to update branch", e))
    }

    async fn ensure_repo_cloned_and_on_branch(
        repo: &GitHubRepository,
        branch_name: &str,
        project_id: Uuid,
        branch_id: Uuid,
        token: &str,
    ) -> Result<std::path::PathBuf, OxyError> {
        let repo_path = GitOperations::get_repository_path(project_id, branch_id)?;

        GitOperations::check_git_availability().await?;
        GitOperations::ensure_git_config().await?;

        if GitOperations::is_git_repository(&repo_path).await {
            let current = GitOperations::get_current_branch(&repo_path).await?;
            if current != branch_name {
                info!("Switching from branch '{}' to '{}'", current, branch_name);
                GitOperations::switch_branch(&repo_path, branch_name).await?;
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

    async fn latest_commit_for_branch(
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

    async fn sync_with_remote<F, Fut>(
        project_id: Uuid,
        query_branch: Option<String>,
        operation: F,
    ) -> Result<(), OxyError>
    where
        F: Fn(&Path, &str) -> Fut,
        Fut: std::future::Future<Output = Result<(), OxyError>>,
    {
        let db = Self::db().await?;
        let project = Self::load_project(project_id).await?;
        let token = Self::require_token(&project)?;

        let branch = Self::find_branch_by_name_or_active(&db, &project, query_branch).await?;
        let _ = Self::set_branch_status(&db, &branch, entity::branches::SyncStatus::Syncing, None)
            .await?;

        let repo_path = GitOperations::get_repository_path(project.id, branch.id)?;
        let res = operation(&repo_path, &token).await;

        match res {
            Ok(()) => {
                let current_commit = match GitOperations::get_current_commit_hash(&repo_path).await
                {
                    Ok(c) => c,
                    Err(e) => {
                        warn!("Failed to get current commit hash after operation: {}", e);
                        branch.revision.clone()
                    }
                };
                let updated = Self::set_branch_status(
                    &db,
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
                let _ = Self::set_branch_status(
                    &db,
                    &branch,
                    entity::branches::SyncStatus::Failed,
                    None,
                )
                .await;
                Err(e)
            }
        }
    }
}

// Public API implementations (reusing the helpers above)
impl ProjectService {
    pub async fn update_project_token(project_id: Uuid, token: String) -> Result<(), OxyError> {
        let client = Self::client_from_token(token.clone())?;
        client.validate_token().await?;
        let db = Self::db().await?;
        let project = Self::load_project(project_id).await?;
        let encrypted = TokenEncryption::encrypt_token(&token)?;
        let mut m: projects::ActiveModel = project.into();
        m.token = Set(Some(encrypted));
        m.updated_at = Set(Self::now().into());
        m.update(&db)
            .await
            .map_err(|e| Self::wrap_db_err("Failed to update project token", e))?;
        Ok(())
    }

    pub async fn get_project(project_id: Uuid) -> Result<Option<projects::Model>, OxyError> {
        Self::with_db(|db| async move {
            Projects::find_by_id(project_id)
                .one(&db)
                .await
                .map_err(|e| Self::wrap_db_err("Failed to get project", e))
        })
        .await
    }

    pub async fn get_branch(branch_id: Uuid) -> Result<Option<branches::Model>, OxyError> {
        Self::with_db(|db| async move {
            Branches::find_by_id(branch_id)
                .one(&db)
                .await
                .map_err(|e| Self::wrap_db_err("Failed to get branch", e))
        })
        .await
    }

    pub async fn get_projects_by_organization(
        organization_id: Uuid,
    ) -> Result<Vec<projects::Model>, OxyError> {
        Self::with_db(|db| async move {
            Projects::find()
                .filter(projects::Column::OrganizationId.eq(organization_id))
                .all(&db)
                .await
                .map_err(|e| Self::wrap_db_err("Failed to get projects", e))
        })
        .await
    }

    pub async fn list_github_repositories(
        project_id: Uuid,
    ) -> Result<Vec<GitHubRepository>, OxyError> {
        let project = Self::load_project(project_id).await?;
        let (client, _token) = Self::github_client_and_token(&project)?;
        client.list_repositories().await
    }

    pub async fn get_project_branches(project_id: Uuid) -> Result<Vec<ProjectBranch>, OxyError> {
        let db = Self::db().await?;

        // Local branches
        let locals = Branches::find()
            .filter(branches::Column::ProjectId.eq(project_id))
            .all(&db)
            .await
            .map_err(|e| Self::wrap_db_err("Failed to get branches", e))?;

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

        // Merge remote branches if possible
        if let Ok(remotes) = Self::fetch_github_branches(project_id).await {
            Self::merge_github_branches(&mut out, remotes);
        }
        Ok(out)
    }

    pub async fn switch_project_branch(
        project_id: Uuid,
        branch_name: String,
    ) -> Result<branches::Model, OxyError> {
        let db = Self::db().await?;
        let project = Self::load_project(project_id).await?;

        let existing = Branches::find()
            .filter(branches::Column::ProjectId.eq(project.id))
            .filter(branches::Column::Name.eq(&branch_name))
            .one(&db)
            .await
            .map_err(|e| Self::wrap_db_err("Failed to find branch", e))?;

        if let Some(b) = existing {
            return Ok(b);
        }

        info!(
            "Branch '{}' not found for project '{}', creating new branch",
            branch_name, project.name
        );

        let new_b = branches::ActiveModel {
            id: Set(Uuid::new_v4()),
            project_id: Set(project.id),
            name: Set(branch_name.clone()),
            revision: Set(String::new()),
            sync_status: Set(entity::branches::SyncStatus::Pending.as_str().to_string()),
            created_at: Set(Self::now().into()),
            updated_at: Set(Self::now().into()),
        }
        .insert(&db)
        .await
        .map_err(|e| Self::wrap_db_err("Failed to create branch", e))?;

        // Trigger initial sync (propagate error so caller can surface it)
        Self::sync_project_branch(project.id, new_b.id).await?;
        Ok(new_b)
    }

    pub async fn switch_project_active_branch(
        project_id: Uuid,
        branch_name: String,
    ) -> Result<branches::Model, OxyError> {
        let db = Self::db().await?;
        let project = Self::load_project(project_id).await?;

        if let Some(active) = Branches::find_by_id(project.active_branch_id)
            .one(&db)
            .await
            .map_err(|e| Self::wrap_db_err("Failed to find branch", e))?
            && active.name == branch_name
        {
            return Ok(active);
        }

        let branch = Self::switch_project_branch(project_id, branch_name).await?;

        let mut pm: projects::ActiveModel = project.into();
        pm.active_branch_id = Set(branch.id);
        pm.updated_at = Set(Self::now().into());
        pm.update(&db)
            .await
            .map_err(|e| Self::wrap_db_err("Failed to update project", e))?;
        Ok(branch)
    }

    async fn fetch_github_branches(project_id: Uuid) -> Result<Vec<ProjectBranch>, OxyError> {
        let project = Self::load_project(project_id).await?;
        let repo_id = Self::project_repo_id(&project)?;
        let (client, _token) = Self::github_client_and_token(&project)?;
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
                    created_at: Self::now().to_string(),
                    updated_at: Self::now().to_string(),
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

    pub async fn delete_project(project_id: Uuid) -> Result<(), OxyError> {
        let db = Self::db().await?;
        Branches::delete_many()
            .filter(branches::Column::ProjectId.eq(project_id))
            .exec(&db)
            .await
            .map_err(|e| Self::wrap_db_err("Failed to delete project branches", e))?;

        Projects::delete_by_id(project_id)
            .exec(&db)
            .await
            .map_err(|e| Self::wrap_db_err("Failed to delete project", e))?;
        Ok(())
    }

    pub async fn sync_project_branch(
        project_id: Uuid,
        branch_id: Uuid,
    ) -> Result<String, OxyError> {
        let db = Self::db().await?;
        let project = Self::load_project(project_id).await?;
        let token = Self::require_token(&project)?;
        let repo_id = Self::project_repo_id(&project)?;
        let client = Self::client_from_token(token.clone())?;

        let branch = Branches::find_by_id(branch_id)
            .one(&db)
            .await
            .map_err(|e| Self::wrap_db_err("Failed to find branch", e))?
            .ok_or_else(|| OxyError::RuntimeError("Branch not found".to_string()))?;

        if branch.project_id != project_id {
            return Err(OxyError::RuntimeError(
                "Branch does not belong to the specified project".to_string(),
            ));
        }

        let _ = Self::set_branch_status(&db, &branch, entity::branches::SyncStatus::Syncing, None)
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

        let updated = Self::set_branch_status(
            &db,
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

    pub async fn get_revision_info(
        project_id: Uuid,
        query_branch: Option<String>,
    ) -> Result<RevisionInfoResponse, OxyError> {
        info!("Getting revision information");
        let db = Self::db().await?;
        let project = Self::load_project(project_id).await?;
        let (client, _token) = Self::github_client_and_token(&project)?;
        let repo_id = Self::project_repo_id(&project)?;

        let branch = Self::find_branch_by_name_or_active(&db, &project, query_branch).await?;

        let latest_hash = client.get_branch_commit_hash(repo_id, &branch.name).await?;
        let latest_commit = client.get_commit_details(repo_id, &latest_hash).await?;
        let current_commit = client.get_commit_details(repo_id, &branch.revision).await?;

        Ok(RevisionInfoResponse {
            current_revision: Some(branch.revision),
            latest_revision: Some(latest_hash),
            current_commit: Some(current_commit),
            latest_commit: Some(latest_commit),
            sync_status: branch.sync_status,
            last_sync_time: Some(branch.updated_at.to_string()),
        })
    }

    pub async fn get_file_from_git(repo_path: &Path, file_path: &str) -> Result<String, OxyError> {
        info!("Getting file content from Git");
        GitOperations::get_file_content(repo_path, file_path, None).await
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

    pub async fn create_project_with_repo_and_pull(
        organization_id: Uuid,
        github_token: String,
        repo_id: i64,
        branch_name: String,
        provider: entity::projects::ProjectProvider,
    ) -> Result<(projects::Model, branches::Model, String), OxyError> {
        let client = Self::client_from_token(github_token.clone())?;
        client.validate_token().await?;
        let repo = client.get_repository(repo_id).await?;
        let encrypted = TokenEncryption::encrypt_token(&github_token)?;

        let db = Self::db().await?;
        let active_branch_id = Uuid::new_v4();

        let project_model = projects::ActiveModel {
            id: Set(Uuid::new_v4()),
            name: Set(repo.full_name.clone()),
            organization_id: Set(organization_id),
            repo_id: Set(Some(repo_id.to_string())),
            active_branch_id: Set(active_branch_id),
            token: Set(Some(encrypted)),
            provider: Set(Some(provider.as_str().to_string())),
            created_at: Set(Self::now().into()),
            updated_at: Set(Self::now().into()),
        };
        let project = project_model
            .insert(&db)
            .await
            .map_err(|e| Self::wrap_db_err("Failed to create project", e))?;

        let latest_commit =
            Self::latest_commit_for_branch(&client, repo_id, &repo, &branch_name).await?;

        let branch_model = branches::ActiveModel {
            id: Set(active_branch_id),
            project_id: Set(project.id),
            name: Set(branch_name.clone()),
            revision: Set(latest_commit),
            sync_status: Set(entity::branches::SyncStatus::Syncing.as_str().to_string()),
            created_at: Set(Self::now().into()),
            updated_at: Set(Self::now().into()),
        };
        let branch = branch_model
            .insert(&db)
            .await
            .map_err(|e| Self::wrap_db_err("Failed to create branch", e))?;

        info!(
            "Created project '{}' with GitHub repository '{}', starting clone/pull",
            project.name, repo.full_name
        );

        let repo_path = Self::ensure_repo_cloned_and_on_branch(
            &repo,
            &branch_name,
            project.id,
            active_branch_id,
            &github_token,
        )
        .await?;
        let local_path = repo_path.to_string_lossy().to_string();

        let res = async {
            if GitOperations::is_git_repository(&repo_path).await {
                GitOperations::pull_repository(&repo_path, Some(&github_token)).await
            } else {
                Ok(())
            }
        }
        .await;

        let mut bm: branches::ActiveModel = branch.clone().into();
        match res {
            Ok(()) => {
                bm.sync_status = Set(entity::branches::SyncStatus::Synced.as_str().to_string());
                bm.updated_at = Set(Self::now().into());
                let updated_branch = bm
                    .update(&db)
                    .await
                    .map_err(|e| Self::wrap_db_err("Failed to update branch after clone", e))?;
                info!(
                    "Successfully cloned/pulled repository for project '{}' branch '{}'",
                    project.name, branch_name
                );
                Ok((project, updated_branch, local_path))
            }
            Err(e) => {
                bm.sync_status = Set(entity::branches::SyncStatus::Failed.as_str().to_string());
                bm.updated_at = Set(Self::now().into());
                if let Err(db_err) = bm.update(&db).await {
                    warn!(
                        "Failed to update branch status after clone failure: {}",
                        db_err
                    );
                }
                Err(e)
            }
        }
    }
}
