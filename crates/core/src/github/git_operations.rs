use crate::errors::OxyError;
use crate::state_dir::get_state_dir;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileStatus {
    pub path: String,
    pub status: String,
    pub insert: u32,
    pub delete: u32,
}

/// Git operations for repository management
pub struct GitOperations;

impl GitOperations {
    /// Clone a repository to a local directory
    pub async fn clone_repository(
        repo_url: &str,
        destination: &Path,
        branch: Option<&str>,
        token: Option<&str>,
    ) -> Result<(), OxyError> {
        info!(
            "Cloning repository {} to {}",
            repo_url,
            destination.display()
        );

        // Ensure parent directory exists
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                OxyError::IOError(format!("Failed to create parent directory: {e}"))
            })?;
        }

        let mut cmd = Command::new("git");
        cmd.arg("clone");

        // Add branch specification if provided
        if let Some(branch) = branch {
            cmd.args(["--branch", branch]);
        }

        // Prepare the repository URL with token if provided
        let clone_url = if let Some(token) = token {
            if repo_url.starts_with("https://github.com/") {
                repo_url.replace(
                    "https://github.com/",
                    &format!("https://x-access-token:{}@github.com/", token),
                )
            } else if repo_url.starts_with("https://") {
                // For other HTTPS URLs, insert token after https://
                repo_url.replacen("https://", &format!("https://{}@", token), 1)
            } else {
                // For non-HTTPS URLs, use as-is
                repo_url.to_string()
            }
        } else {
            repo_url.to_string()
        };

        // Add clone URL and destination
        cmd.arg(&clone_url).arg(destination);

        let output = cmd
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git clone: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Git clone failed: {}", stderr);
            return Err(OxyError::RuntimeError(format!(
                "Git clone failed: {stderr}"
            )));
        }

        info!(
            "Successfully cloned repository to {}",
            destination.display()
        );
        Ok(())
    }

    /// Pull latest changes for an existing repository with force reset
    pub async fn pull_repository(repo_path: &Path, token: Option<&str>) -> Result<(), OxyError> {
        info!(
            "Force resetting and pulling latest changes for repository at {}",
            repo_path.display()
        );

        // Update remote URL with authentication if token is provided
        if let Some(token) = token {
            Self::update_remote_url_with_auth(repo_path, token).await?;
        }

        if !repo_path.exists() {
            return Err(OxyError::RuntimeError(format!(
                "Repository directory does not exist: {}",
                repo_path.display()
            )));
        }

        // Get current branch name
        let current_branch = Self::get_current_branch(repo_path).await?;

        // Fetch latest changes from remote
        let fetch_output = Command::new("git")
            .current_dir(repo_path)
            .args(["fetch", "origin"])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git fetch: {e}")))?;

        if !fetch_output.status.success() {
            let stderr = String::from_utf8_lossy(&fetch_output.stderr);
            warn!("Git fetch failed: {}", stderr);
            return Err(OxyError::RuntimeError(format!(
                "Git fetch failed: {stderr}"
            )));
        }

        // Force reset to remote branch (discards all local changes)
        let reset_output = Command::new("git")
            .current_dir(repo_path)
            .args(["reset", "--hard", &format!("origin/{}", current_branch)])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git reset: {e}")))?;

        if !reset_output.status.success() {
            let stderr = String::from_utf8_lossy(&reset_output.stderr);
            warn!("Git reset failed: {}", stderr);
            return Err(OxyError::RuntimeError(format!(
                "Git reset failed: {stderr}"
            )));
        }

        // Clean untracked files and directories
        let clean_output = Command::new("git")
            .current_dir(repo_path)
            .args(["clean", "-fd"])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git clean: {e}")))?;

        if !clean_output.status.success() {
            let stderr = String::from_utf8_lossy(&clean_output.stderr);
            warn!("Git clean failed: {}", stderr);
        }

        // Pull latest changes from remote
        let pull_output = Command::new("git")
            .current_dir(repo_path)
            .args(["pull", "origin", &current_branch])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git pull: {e}")))?;

        if !pull_output.status.success() {
            let stderr = String::from_utf8_lossy(&pull_output.stderr);
            warn!("Git pull failed: {}", stderr);
            return Err(OxyError::RuntimeError(format!("Git pull failed: {stderr}")));
        }

        let reset_stdout = String::from_utf8_lossy(&reset_output.stdout);
        let clean_stdout = String::from_utf8_lossy(&clean_output.stdout);
        let pull_stdout = String::from_utf8_lossy(&pull_output.stdout);
        info!(
            "Force reset and pull completed: reset: {}, clean: {}, pull: {}",
            reset_stdout.trim(),
            clean_stdout.trim(),
            pull_stdout.trim()
        );
        Ok(())
    }

    pub async fn push_repository(
        repo_path: &Path,
        token: Option<&str>,
        message: &str,
    ) -> Result<(), OxyError> {
        info!(
            "Auto-committing and force pushing all changes for repository at {}",
            repo_path.display()
        );

        // Update remote URL with authentication if token is provided
        if let Some(token) = token {
            Self::update_remote_url_with_auth(repo_path, token).await?;
        }

        let _ = Command::new("git")
            .args(["config", "--global", "push.autoSetupRemote", "true"])
            .output()
            .await;

        if !repo_path.exists() {
            return Err(OxyError::RuntimeError(format!(
                "Repository directory does not exist: {}",
                repo_path.display()
            )));
        }

        // Stage all changes (including new files and deletions)
        let add_output = Command::new("git")
            .current_dir(repo_path)
            .args(["add", "-A"])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git add: {e}")))?;

        if !add_output.status.success() {
            let stderr = String::from_utf8_lossy(&add_output.stderr);
            warn!("Git add failed: {}", stderr);
            return Err(OxyError::RuntimeError(format!("Git add failed: {stderr}")));
        }

        // Check if there are any changes to commit
        let status_output = Command::new("git")
            .current_dir(repo_path)
            .args(["status", "--porcelain"])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to check git status: {e}")))?;

        let status = String::from_utf8_lossy(&status_output.stdout);
        if status.trim().is_empty() {
            info!("No changes to commit");
        } else {
            // Commit all staged changes
            let commit_output = Command::new("git")
                .current_dir(repo_path)
                .args(["commit", "-m", message])
                .output()
                .await
                .map_err(|e| {
                    OxyError::RuntimeError(format!("Failed to execute git commit: {e}"))
                })?;

            if !commit_output.status.success() {
                let stderr = String::from_utf8_lossy(&commit_output.stderr);
                warn!("Git commit failed: {}", stderr);
                return Err(OxyError::RuntimeError(format!(
                    "Git commit failed: {stderr}"
                )));
            }

            let commit_stdout = String::from_utf8_lossy(&commit_output.stdout);
            info!("Git commit completed: {}", commit_stdout.trim());
        }

        // Force push to origin
        let push_output = Command::new("git")
            .current_dir(repo_path)
            .args(["push", "origin", "--force"])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git push: {e}")))?;

        if !push_output.status.success() {
            let stderr = String::from_utf8_lossy(&push_output.stderr);
            warn!("Git force push failed: {}", stderr);
            return Err(OxyError::RuntimeError(format!(
                "Git force push failed: {stderr}"
            )));
        }

        let push_stdout = String::from_utf8_lossy(&push_output.stdout);
        info!("Git force push completed: {}", push_stdout.trim());
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
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get current branch: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(OxyError::RuntimeError(format!(
                "Get current branch failed: {stderr}"
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
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get git status: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(OxyError::RuntimeError(format!(
                "Git status failed: {stderr}"
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

    /// Get the default repositories directory for storing cloned repositories
    pub fn get_repositories_directory() -> Result<PathBuf, OxyError> {
        // Try environment variable first
        let state_dir = get_state_dir();

        Ok(PathBuf::from(state_dir).join("repos"))
    }

    /// Get the local path for a specific repository by ID
    pub fn get_repository_path(project_id: Uuid, branch: Uuid) -> Result<PathBuf, OxyError> {
        let repos_dir = Self::get_repositories_directory()?;
        Ok(repos_dir
            .join(project_id.to_string())
            .join(branch.to_string()))
    }

    /// Ensure git is available on the system
    pub async fn check_git_availability() -> Result<(), OxyError> {
        let output = Command::new("git")
            .arg("--version")
            .output()
            .await
            .map_err(|e| {
                OxyError::ConfigurationError(format!(
                    "Git is not available on this system. Please install Git: {e}"
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
            .map_err(|e| OxyError::RuntimeError(format!("Failed to check git config: {e}")))?;

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
            .map_err(|e| OxyError::RuntimeError(format!("Failed to check git config: {e}")))?;

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
    pub async fn auto_pull_repository(
        repo_path: &Path,
        token: Option<&str>,
    ) -> Result<String, OxyError> {
        info!(
            "Auto-pulling repository changes for {}",
            repo_path.display()
        );

        // Update remote URL with authentication if token is provided
        if let Some(token) = token {
            Self::update_remote_url_with_auth(repo_path, token).await?;
        }
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
        Self::pull_repository(repo_path, token).await?;

        // Get status after pull to see what changed
        let current_branch = Self::get_current_branch(repo_path).await?;

        let message = format!("Successfully pulled latest changes from {current_branch} branch");

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
                    OxyError::RuntimeError(format!("Failed to get file modification time: {e}"))
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
                        OxyError::RuntimeError(format!("Failed to execute git log: {e}"))
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
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git rev-parse: {e}")))?;

        if output.status.success() {
            let commit_hash = String::from_utf8_lossy(&output.stdout);
            Ok(commit_hash.trim().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(OxyError::RuntimeError(format!(
                "Failed to get current commit hash: {stderr}"
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
            .arg(format!("refs/heads/{branch}"))
            .current_dir(repo_path)
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git ls-remote: {e}")))?;

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
                "Failed to get remote commit hash: {stderr}"
            )))
        }
    }

    pub async fn switch_branch(
        repo_path: &Path,
        branch: &str,
        token: &str,
    ) -> Result<(), OxyError> {
        info!(
            "Switching to branch '{}' in repository at {}",
            branch,
            repo_path.display()
        );

        if !repo_path.exists() {
            return Err(OxyError::RuntimeError(format!(
                "Repository directory does not exist: {}",
                repo_path.display()
            )));
        }

        Self::update_remote_url_with_auth(repo_path, token).await?;

        let fetch_output = Command::new("git")
            .current_dir(repo_path)
            .args(["fetch", "origin"])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git fetch: {e}")))?;

        if !fetch_output.status.success() {
            let stderr = String::from_utf8_lossy(&fetch_output.stderr);
            warn!("Git fetch failed: {}", stderr);
        }

        let output = Command::new("git")
            .current_dir(repo_path)
            .args(["checkout", "-B", branch, &format!("origin/{branch}")])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git checkout: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(OxyError::RuntimeError(format!(
                "Failed to switch to branch '{}': {stderr}",
                branch
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        info!(
            "Successfully switched to branch '{}': {}",
            branch,
            stdout.trim()
        );
        Ok(())
    }

    pub async fn status_short(repo_path: &Path) -> Result<String, OxyError> {
        if !repo_path.exists() {
            return Err(OxyError::RuntimeError(format!(
                "Repository directory does not exist: {}",
                repo_path.display()
            )));
        }

        let output = Command::new("git")
            .current_dir(repo_path)
            .args(["status", "--short"])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git status: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(OxyError::RuntimeError(format!(
                "Git status failed: {stderr}"
            )));
        }

        let status = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(status)
    }

    pub async fn diff_numstat(repo_path: &Path) -> Result<String, OxyError> {
        if !repo_path.exists() {
            return Err(OxyError::RuntimeError(format!(
                "Repository directory does not exist: {}",
                repo_path.display()
            )));
        }

        let output = Command::new("git")
            .current_dir(repo_path)
            .args(["diff", "--numstat"])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git diff: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(OxyError::RuntimeError(format!("Git diff failed: {stderr}")));
        }

        let diff = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(diff)
    }

    pub async fn diff_numstat_summary(repo_path: &Path) -> Result<Vec<FileStatus>, OxyError> {
        if !repo_path.exists() {
            return Err(OxyError::RuntimeError(format!(
                "Repository directory does not exist: {}",
                repo_path.display()
            )));
        }

        let status_output = Command::new("git")
            .current_dir(repo_path)
            .args(["status", "--short", "--untracked-files=all"])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git status: {e}")))?;

        if !status_output.status.success() {
            let stderr = String::from_utf8_lossy(&status_output.stderr);
            return Err(OxyError::RuntimeError(format!(
                "Git status failed: {stderr}"
            )));
        }

        let diff_output = Command::new("git")
            .current_dir(repo_path)
            .args(["diff", "--numstat"])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git diff: {e}")))?;

        if !diff_output.status.success() {
            let stderr = String::from_utf8_lossy(&diff_output.stderr);
            return Err(OxyError::RuntimeError(format!("Git diff failed: {stderr}")));
        }

        let status_str = String::from_utf8_lossy(&status_output.stdout);
        let diff_str = String::from_utf8_lossy(&diff_output.stdout);

        let mut diff_stats: HashMap<String, (u32, u32)> = HashMap::new();
        for line in diff_str.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let insertions = parts[0].parse::<u32>().unwrap_or(0);
                let deletions = parts[1].parse::<u32>().unwrap_or(0);
                let file_path = parts[2..].join(" ");
                diff_stats.insert(file_path, (insertions, deletions));
            }
        }

        let mut result = Vec::new();
        for line in status_str.lines() {
            if line.trim().is_empty() {
                continue;
            }

            if line.len() >= 3 {
                let status_chars = &line[0..2];
                let file_path = line[3..].trim().to_string();

                // Map git status codes to single character status
                let status = match status_chars {
                    "M " | " M" | "MM" => "M", // Modified
                    "A " | " A" | "AM" => "A", // Added
                    "D " | " D" | "AD" => "D", // Deleted
                    "R " | " R" => "M",        // Renamed -> treat as Modified
                    "C " | " C" => "A",        // Copied -> treat as Added
                    "??" => "A",               // Untracked -> treat as Added
                    _ => "M",                  // Default to Modified for other cases
                }
                .to_string();

                let (insert, delete) = diff_stats.get(&file_path).unwrap_or(&(0, 0));

                result.push(FileStatus {
                    path: file_path,
                    status,
                    insert: *insert,
                    delete: *delete,
                });
            }
        }

        Ok(result)
    }

    pub async fn get_file_content(
        repo_path: &Path,
        file_path: &str,
        commit: Option<&str>,
    ) -> Result<String, OxyError> {
        if !repo_path.exists() {
            return Err(OxyError::RuntimeError(format!(
                "Repository directory does not exist: {}",
                repo_path.display()
            )));
        }

        let commit_ref = commit.unwrap_or("HEAD");
        let show_ref = format!("{}:{}", commit_ref, file_path);

        info!(
            "Getting file content for '{}' from commit '{}' in repository at {}",
            file_path,
            commit_ref,
            repo_path.display()
        );

        let output = Command::new("git")
            .current_dir(repo_path)
            .args(["show", &show_ref])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git show: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(OxyError::RuntimeError(format!(
                "Failed to get file content for '{}' at commit '{}': {stderr}",
                file_path, commit_ref
            )));
        }

        let content = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(content)
    }

    /// Initialize a new git repository, create initial files, and push to remote
    pub async fn init_and_push_repository(
        repo_path: &Path,
        remote_url: &str,
        token: Option<&str>,
    ) -> Result<(), OxyError> {
        info!(
            "Initializing and pushing new repository at {} to {}",
            repo_path.display(),
            remote_url
        );

        // Ensure parent directory exists
        if let Some(parent) = repo_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                OxyError::IOError(format!("Failed to create repository directory: {e}"))
            })?;
        }

        // Initialize git repository
        let init_output = Command::new("git")
            .arg("init")
            .current_dir(repo_path)
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git init: {e}")))?;

        if !init_output.status.success() {
            let stderr = String::from_utf8_lossy(&init_output.stderr);
            return Err(OxyError::RuntimeError(format!("Git init failed: {stderr}")));
        }

        // Add all files
        let add_output = Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git add: {e}")))?;

        if !add_output.status.success() {
            let stderr = String::from_utf8_lossy(&add_output.stderr);
            return Err(OxyError::RuntimeError(format!("Git add failed: {stderr}")));
        }

        // Initial commit
        let commit_output = Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git commit: {e}")))?;

        if !commit_output.status.success() {
            let stderr = String::from_utf8_lossy(&commit_output.stderr);
            return Err(OxyError::RuntimeError(format!(
                "Git commit failed: {stderr}"
            )));
        }

        // Rename branch to main
        let branch_output = Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(repo_path)
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git branch: {e}")))?;

        if !branch_output.status.success() {
            let stderr = String::from_utf8_lossy(&branch_output.stderr);
            return Err(OxyError::RuntimeError(format!(
                "Git branch rename failed: {stderr}"
            )));
        }

        // Prepare the repository URL with token if provided
        let remote_url_with_auth = if let Some(token) = token {
            if remote_url.starts_with("https://github.com/") {
                remote_url.replace(
                    "https://github.com/",
                    &format!("https://x-access-token:{}@github.com/", token),
                )
            } else if remote_url.starts_with("https://") {
                // For other HTTPS URLs, insert token after https://
                remote_url.replacen("https://", &format!("https://{}@", token), 1)
            } else {
                // For non-HTTPS URLs, use as-is
                remote_url.to_string()
            }
        } else {
            remote_url.to_string()
        };

        // Add remote origin
        let remote_output = Command::new("git")
            .args(["remote", "add", "origin", &remote_url_with_auth])
            .current_dir(repo_path)
            .output()
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!("Failed to execute git remote add: {e}"))
            })?;

        if !remote_output.status.success() {
            let stderr = String::from_utf8_lossy(&remote_output.stderr);
            return Err(OxyError::RuntimeError(format!(
                "Git remote add failed: {stderr}"
            )));
        }

        // Push to origin
        let push_output = Command::new("git")
            .args(["push", "-u", "origin", "main"])
            .current_dir(repo_path)
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to execute git push: {e}")))?;

        if !push_output.status.success() {
            let stderr = String::from_utf8_lossy(&push_output.stderr);
            return Err(OxyError::RuntimeError(format!("Git push failed: {stderr}")));
        }

        info!(
            "Successfully initialized and pushed repository to {}",
            remote_url
        );

        Ok(())
    }

    /// Update the remote origin URL with authentication token
    async fn update_remote_url_with_auth(repo_path: &Path, token: &str) -> Result<(), OxyError> {
        // Get current remote URL
        let remote_output = Command::new("git")
            .current_dir(repo_path)
            .args(["remote", "get-url", "origin"])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get remote URL: {e}")))?;

        if !remote_output.status.success() {
            let stderr = String::from_utf8_lossy(&remote_output.stderr);
            return Err(OxyError::RuntimeError(format!(
                "Failed to get remote URL: {stderr}"
            )));
        }

        let current_url = String::from_utf8_lossy(&remote_output.stdout)
            .trim()
            .to_string();

        // Remove existing authentication if present
        let clean_url = if current_url.contains("@github.com") {
            if let Some(at_pos) = current_url.find("@github.com") {
                if let Some(protocol_end) = current_url.find("://") {
                    format!("https://github.com{}", &current_url[at_pos + 11..])
                } else {
                    current_url
                }
            } else {
                current_url
            }
        } else {
            current_url
        };

        // Add authentication to the URL
        let authenticated_url = if clean_url.starts_with("https://github.com/") {
            clean_url.replace(
                "https://github.com/",
                &format!("https://x-access-token:{}@github.com/", token),
            )
        } else if clean_url.starts_with("https://") {
            // For other HTTPS URLs, insert token after https://
            clean_url.replacen("https://", &format!("https://{}@", token), 1)
        } else {
            // For non-HTTPS URLs, use as-is
            clean_url
        };

        // Update the remote URL
        let set_url_output = Command::new("git")
            .current_dir(repo_path)
            .args(["remote", "set-url", "origin", &authenticated_url])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to set remote URL: {e}")))?;

        if !set_url_output.status.success() {
            let stderr = String::from_utf8_lossy(&set_url_output.stderr);
            return Err(OxyError::RuntimeError(format!(
                "Failed to set remote URL: {stderr}"
            )));
        }

        info!("Updated remote URL with authentication");
        Ok(())
    }
}
