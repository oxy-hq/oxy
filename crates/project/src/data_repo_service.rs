use oxy::config::model::Repository;
use oxy_shared::errors::OxyError;
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Prefix used in file paths to identify repository files.
pub const DATA_REPO_PREFIX: char = '@';

/// Resolves the filesystem path for a repository without cloning.
/// - `path` repos: resolved relative to project root (or absolute if given).
/// - `git_url` repos: returns `{project_path}/.repositories/{name}/`.
pub fn resolve_data_repo_path(project_path: &Path, repo: &Repository) -> Result<PathBuf, OxyError> {
    if let Some(local_path) = &repo.path {
        let p = Path::new(local_path);
        let resolved = if p.is_absolute() {
            p.to_path_buf()
        } else {
            project_path.join(p)
        };
        Ok(resolved)
    } else if repo.git_url.is_some() {
        Ok(project_path.join(".repositories").join(&repo.name))
    } else {
        Err(OxyError::ConfigurationError(format!(
            "repository '{}' must specify either `path` or `git_url`",
            repo.name
        )))
    }
}

/// Ensures the repository is available on disk.
/// - Local path repos: just validates the directory exists.
/// - Git URL repos: clones on first access, fetches + resets on subsequent calls (best-effort).
///
/// Returns the resolved path.
pub async fn ensure_data_repo_available(
    project_path: &Path,
    repo: &Repository,
) -> Result<PathBuf, OxyError> {
    let resolved = resolve_data_repo_path(project_path, repo)?;

    if let Some(git_url) = &repo.git_url {
        if !resolved.exists() {
            // Clone the repo
            let mut cmd = Command::new("git");
            cmd.arg("clone").arg("--depth").arg("1");
            if let Some(branch) = &repo.branch {
                cmd.arg("--branch").arg(branch);
            }
            cmd.arg(git_url).arg(&resolved);

            let output = cmd
                .output()
                .await
                .map_err(|e| OxyError::RuntimeError(format!("Failed to run git clone: {e}")))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(OxyError::RuntimeError(format!(
                    "git clone failed for '{}': {stderr}",
                    repo.name
                )));
            }
        } else {
            // Best-effort fetch + reset to keep it up to date
            let _ = Command::new("git")
                .args(["fetch", "--depth", "1", "origin"])
                .current_dir(&resolved)
                .output()
                .await;
            // FETCH_HEAD is always set by the preceding `git fetch` and points
            // to exactly what was fetched — more reliable than `origin/HEAD`
            // which is often unset after a shallow clone.
            let _ = Command::new("git")
                .args(["reset", "--hard", "FETCH_HEAD"])
                .current_dir(&resolved)
                .output()
                .await;
        }
    } else if !resolved.exists() {
        return Err(OxyError::ConfigurationError(format!(
            "repository '{}' path does not exist: {}",
            repo.name,
            resolved.display()
        )));
    }

    Ok(resolved)
}

/// Parses a decoded file path and splits off the repository name if present.
/// Returns `Some((repo_name, relative_path))` for `@repo-name/path/to/file`,
/// or `None` for regular project files.
pub fn parse_data_repo_path(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix(DATA_REPO_PREFIX)?;
    let slash = rest.find('/')?;
    Some((&rest[..slash], &rest[slash + 1..]))
}
