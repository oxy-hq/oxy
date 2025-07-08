use crate::errors::OxyError;
use std::env;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{error, info, warn};

/// Git operations for repository management
pub struct GitOperations;

impl GitOperations {
    /// Clone a repository to a local directory
    pub async fn clone_repository(
        repo_url: &str,
        destination: &Path,
        branch: Option<&str>,
    ) -> Result<(), OxyError> {
        info!(
            "Cloning repository {} to {}",
            repo_url,
            destination.display()
        );

        // Ensure parent directory exists
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                OxyError::IOError(format!("Failed to create parent directory: {}", e))
            })?;
        }

        let mut cmd = Command::new("git");
        cmd.arg("clone");

        // Add branch specification if provided
        if let Some(branch) = branch {
            cmd.args(["--branch", branch]);
        }

        // Add clone URL and destination
        cmd.arg(repo_url).arg(destination);

        let output = cmd
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git clone: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Git clone failed: {}", stderr);
            return Err(OxyError::RuntimeError(format!(
                "Git clone failed: {}",
                stderr
            )));
        }

        info!(
            "Successfully cloned repository to {}",
            destination.display()
        );
        Ok(())
    }

    /// Pull latest changes for an existing repository
    pub async fn pull_repository(repo_path: &Path) -> Result<(), OxyError> {
        info!(
            "Pulling latest changes for repository at {}",
            repo_path.display()
        );

        if !repo_path.exists() {
            return Err(OxyError::RuntimeError(format!(
                "Repository directory does not exist: {}",
                repo_path.display()
            )));
        }

        let output = Command::new("git")
            .current_dir(repo_path)
            .args(["pull", "origin"])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git pull: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Git pull failed: {}", stderr);
            return Err(OxyError::RuntimeError(format!(
                "Git pull failed: {}",
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        info!("Git pull completed: {}", stdout.trim());
        Ok(())
    }

    /// Get the current branch of a repository
    pub async fn get_current_branch(repo_path: &Path) -> Result<String, OxyError> {
        if !repo_path.exists() {
            return Err(OxyError::RuntimeError(format!(
                "Repository directory does not exist: {}",
                repo_path.display()
            )));
        }

        let output = Command::new("git")
            .current_dir(repo_path)
            .args(["branch", "--show-current"])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get current branch: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(OxyError::RuntimeError(format!(
                "Get current branch failed: {}",
                stderr
            )));
        }

        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(branch)
    }

    /// Get git status of a repository
    pub async fn get_status(repo_path: &Path) -> Result<String, OxyError> {
        if !repo_path.exists() {
            return Err(OxyError::RuntimeError(format!(
                "Repository directory does not exist: {}",
                repo_path.display()
            )));
        }

        let output = Command::new("git")
            .current_dir(repo_path)
            .args(["status", "--porcelain"])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get git status: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(OxyError::RuntimeError(format!(
                "Git status failed: {}",
                stderr
            )));
        }

        let status = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(status)
    }

    /// Check if a directory is a git repository
    pub async fn is_git_repository(path: &Path) -> bool {
        let git_dir = path.join(".git");
        git_dir.exists()
    }

    /// Get the remote URL of a repository
    pub async fn get_remote_url(repo_path: &Path) -> Result<String, OxyError> {
        if !repo_path.exists() {
            return Err(OxyError::RuntimeError(format!(
                "Repository directory does not exist: {}",
                repo_path.display()
            )));
        }

        let output = Command::new("git")
            .current_dir(repo_path)
            .args(["remote", "get-url", "origin"])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get remote URL: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(OxyError::RuntimeError(format!(
                "Get remote URL failed: {}",
                stderr
            )));
        }

        let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(url)
    }

    /// Get the default repositories directory for storing cloned repositories
    pub fn get_repositories_directory() -> Result<PathBuf, OxyError> {
        // Try environment variable first
        if let Ok(repos_dir) = env::var("OXY_REPOS_DIR") {
            return Ok(PathBuf::from(repos_dir));
        }

        // Fall back to default location: ~/.local/share/oxy/repos
        let home_dir = env::var("HOME")
            .or_else(|_| env::var("USERPROFILE"))
            .map_err(|_| {
                OxyError::ConfigurationError("Unable to determine home directory".to_string())
            })?;

        Ok(PathBuf::from(home_dir)
            .join(".local")
            .join("share")
            .join("oxy")
            .join("repos"))
    }

    /// Get the local path for a specific repository by ID
    pub fn get_repository_path(repo_id: i64) -> Result<PathBuf, OxyError> {
        let repos_dir = Self::get_repositories_directory()?;
        Ok(repos_dir.join(repo_id.to_string()))
    }

    /// Ensure git is available on the system
    pub async fn check_git_availability() -> Result<(), OxyError> {
        let output = Command::new("git")
            .arg("--version")
            .output()
            .await
            .map_err(|e| {
                OxyError::ConfigurationError(format!(
                    "Git is not available on this system. Please install Git: {}",
                    e
                ))
            })?;

        if !output.status.success() {
            return Err(OxyError::ConfigurationError(
                "Git is not properly installed on this system".to_string(),
            ));
        }

        let version = String::from_utf8_lossy(&output.stdout);
        info!("Git is available: {}", version.trim());
        Ok(())
    }

    /// Configure git for the first time if needed
    pub async fn ensure_git_config() -> Result<(), OxyError> {
        // Check if user.name is configured
        let name_output = Command::new("git")
            .args(["config", "user.name"])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to check git config: {}", e)))?;

        if !name_output.status.success() {
            warn!("Git user.name is not configured. Setting default value.");
            let _ = Command::new("git")
                .args(["config", "--global", "user.name", "Oxy User"])
                .output()
                .await;
        }

        // Check if user.email is configured
        let email_output = Command::new("git")
            .args(["config", "user.email"])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to check git config: {}", e)))?;

        if !email_output.status.success() {
            warn!("Git user.email is not configured. Setting default value.");
            let _ = Command::new("git")
                .args(["config", "--global", "user.email", "user@oxy.local"])
                .output()
                .await;
        }

        Ok(())
    }

    /// Pull repository automatically (called by webhook handler)
    pub async fn auto_pull_repository(repo_path: &Path) -> Result<String, OxyError> {
        info!(
            "Auto-pulling repository changes for {}",
            repo_path.display()
        );

        // First, check if this is a git repository
        if !Self::is_git_repository(repo_path).await {
            return Err(OxyError::RuntimeError(format!(
                "Directory is not a git repository: {}",
                repo_path.display()
            )));
        }

        // Check current status before pulling
        let status_before = Self::get_status(repo_path).await?;
        if !status_before.trim().is_empty() {
            warn!(
                "Repository has uncommitted changes, proceeding with pull anyway: {}",
                status_before.trim()
            );
        }

        // Perform the pull
        Self::pull_repository(repo_path).await?;

        // Get status after pull to see what changed
        let current_branch = Self::get_current_branch(repo_path).await?;

        let message = format!(
            "Successfully pulled latest changes from {} branch",
            current_branch
        );

        info!("{}", message);
        Ok(message)
    }

    /// Get the last sync time (last fetch/pull time)
    pub async fn get_last_sync_time(repo_path: &Path) -> Result<String, OxyError> {
        // Get the modification time of the .git/FETCH_HEAD file
        let fetch_head_path = repo_path.join(".git").join("FETCH_HEAD");

        match tokio::fs::metadata(&fetch_head_path).await {
            Ok(metadata) => {
                let modified = metadata.modified().map_err(|e| {
                    OxyError::RuntimeError(format!("Failed to get file modification time: {}", e))
                })?;

                let datetime: chrono::DateTime<chrono::Utc> = modified.into();
                Ok(datetime.to_rfc3339())
            }
            Err(_) => {
                // If FETCH_HEAD doesn't exist, check the last commit time
                let output = Command::new("git")
                    .arg("log")
                    .arg("-1")
                    .arg("--format=%cI")
                    .current_dir(repo_path)
                    .output()
                    .await
                    .map_err(|e| {
                        OxyError::RuntimeError(format!("Failed to execute git log: {}", e))
                    })?;

                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    Ok(output_str.trim().to_string())
                } else {
                    Err(OxyError::RuntimeError(
                        "Failed to get last sync time".to_string(),
                    ))
                }
            }
        }
    }

    /// Get the current commit hash of the repository
    pub async fn get_current_commit_hash(repo_path: &Path) -> Result<String, OxyError> {
        let output = tokio::process::Command::new("git")
            .arg("rev-parse")
            .arg("HEAD")
            .current_dir(repo_path)
            .output()
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!("Failed to execute git rev-parse: {}", e))
            })?;

        if output.status.success() {
            let commit_hash = String::from_utf8_lossy(&output.stdout);
            Ok(commit_hash.trim().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(OxyError::RuntimeError(format!(
                "Failed to get current commit hash: {}",
                stderr
            )))
        }
    }

    /// Get the latest commit hash from a remote branch
    pub async fn get_remote_commit_hash(
        repo_path: &Path,
        branch: &str,
    ) -> Result<String, OxyError> {
        let output = tokio::process::Command::new("git")
            .arg("ls-remote")
            .arg("origin")
            .arg(format!("refs/heads/{}", branch))
            .current_dir(repo_path)
            .output()
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!("Failed to execute git ls-remote: {}", e))
            })?;

        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = output_str.lines().next() {
                let commit_hash = line.split_whitespace().next().unwrap_or("");
                if !commit_hash.is_empty() {
                    Ok(commit_hash.to_string())
                } else {
                    Err(OxyError::RuntimeError(
                        "Empty commit hash returned".to_string(),
                    ))
                }
            } else {
                Err(OxyError::RuntimeError(
                    "No commit found for branch".to_string(),
                ))
            }
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(OxyError::RuntimeError(format!(
                "Failed to get remote commit hash: {}",
                stderr
            )))
        }
    }
}
