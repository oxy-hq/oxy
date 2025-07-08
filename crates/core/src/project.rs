use crate::{
    errors::OxyError,
    github::{git_operations::GitOperations, service::GitHubService},
};
use std::{path::PathBuf, sync::RwLock};

trait ProjectManager: Send + Sync {
    fn resolve_project_path(&self) -> Result<PathBuf, OxyError>;
}

#[derive(Debug, Clone)]
struct GitProjectManager {
    repo_id: i64,
}

impl GitProjectManager {
    pub fn new(repo_id: i64) -> Self {
        Self { repo_id }
    }
}

impl ProjectManager for GitProjectManager {
    fn resolve_project_path(&self) -> Result<PathBuf, OxyError> {
        let repo_path = GitOperations::get_repository_path(self.repo_id)?;

        // Check if the repository exists locally and has a config.yml
        if repo_path.exists() {
            let config_path = repo_path.join("config.yml");
            if config_path.exists() {
                return Ok(repo_path);
            }
        }

        Err(OxyError::ConfigurationError(format!(
            "Repository path not found or invalid: {}",
            repo_path.display()
        )))
    }
}

#[derive(Debug, Clone)]
struct LocalProjectManager {}

impl LocalProjectManager {
    pub fn new() -> Self {
        Self {}
    }
}

impl ProjectManager for LocalProjectManager {
    fn resolve_project_path(&self) -> Result<PathBuf, OxyError> {
        let mut current_dir = std::env::current_dir().expect("Could not get current directory");

        for _ in 0..10 {
            let config_path = current_dir.join("config.yml");
            if config_path.exists() {
                return Ok(current_dir);
            }

            if !current_dir.pop() {
                break;
            }
        }

        Err(OxyError::RuntimeError(
            "Could not find config.yml".to_string(),
        ))
    }
}

#[derive(Debug)]
enum ProjectManagerType {
    Git(GitProjectManager),
    Local(LocalProjectManager),
}

impl ProjectManager for ProjectManagerType {
    fn resolve_project_path(&self) -> Result<PathBuf, OxyError> {
        match self {
            ProjectManagerType::Git(manager) => manager.resolve_project_path(),
            ProjectManagerType::Local(manager) => manager.resolve_project_path(),
        }
    }
}

static PROJECT_MANAGER_INSTANCE: RwLock<Option<ProjectManagerType>> = RwLock::new(None);

pub fn set_git_project_manager(repo_id: i64) {
    let mut manager = PROJECT_MANAGER_INSTANCE
        .write()
        .expect("Failed to acquire write lock");
    *manager = Some(ProjectManagerType::Git(GitProjectManager::new(repo_id)));
}

pub fn set_local_project_manager() {
    let mut manager = PROJECT_MANAGER_INSTANCE
        .write()
        .expect("Failed to acquire write lock");
    *manager = Some(ProjectManagerType::Local(LocalProjectManager::new()));
}

pub fn resolve_project_path() -> Result<PathBuf, OxyError> {
    let manager = PROJECT_MANAGER_INSTANCE
        .read()
        .map_err(|_| OxyError::ConfigurationError("Failed to acquire read lock".to_string()))?;

    let manager = manager.as_ref().ok_or(OxyError::ConfigurationError(
        "Project manager not set".to_string(),
    ))?;

    manager.resolve_project_path()
}

/// Initialize project manager automatically based on readonly mode and available configurations
pub async fn initialize_project_manager(readonly_mode: bool) -> Result<(), OxyError> {
    if readonly_mode {
        // Try to initialize from GitHub settings
        match initialize_from_github().await {
            Ok(()) => return Ok(()),
            Err(e) => {
                tracing::debug!("Failed to initialize from GitHub: {}", e);
                // Fall through to try local mode
            }
        }
    }

    // Try to initialize local project manager
    initialize_local().await
}

/// Initialize project manager from GitHub settings
pub async fn initialize_from_github() -> Result<(), OxyError> {
    let settings = GitHubService::get_settings().await?;

    if let Some(settings) = settings {
        if let Some(repo_id) = settings.selected_repo_id {
            set_git_project_manager(repo_id);
            return Ok(());
        }
    }

    Err(OxyError::ConfigurationError(
        "No GitHub repository configured".to_string(),
    ))
}

/// Initialize local project manager
pub async fn initialize_local() -> Result<(), OxyError> {
    // Create a temporary local manager to check if we can find a local project
    let temp_manager = LocalProjectManager::new();

    // Try to resolve the project path first
    temp_manager.resolve_project_path()?;

    // If successful, set the local project manager
    set_local_project_manager();
    Ok(())
}

/// Reset the project manager (useful for testing or changing modes)
pub fn reset_project_manager() {
    let mut manager = PROJECT_MANAGER_INSTANCE
        .write()
        .expect("Failed to acquire write lock");
    *manager = None;
}

/// Check if project manager is initialized
pub fn is_project_manager_initialized() -> bool {
    PROJECT_MANAGER_INSTANCE
        .read()
        .expect("Failed to acquire read lock")
        .is_some()
}
