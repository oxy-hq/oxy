use std::path::Path;

use oxy_shared::errors::OxyError;
use tracing::info;

use crate::cli::{repo, run, worktree};
use crate::types::Auth;

/// Clone `repo_url` into `destination`, optionally checking out `branch`.
///
/// Auth is injected via `http.extraHeader`; the token is never embedded in
/// the URL or persisted to `.git/config`.
pub async fn clone_repo(
    repo_url: &str,
    destination: &Path,
    branch: Option<&str>,
    auth: &Auth,
) -> Result<(), OxyError> {
    info!(
        "Cloning repository {} to {}",
        repo_url,
        destination.display()
    );

    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| OxyError::IOError(format!("Failed to create parent directory: {e}")))?;
    }

    let dest = destination.to_string_lossy();
    let mut args: Vec<&str> = vec!["clone"];
    if let Some(b) = branch {
        args.extend_from_slice(&["--branch", b]);
    }
    args.push(repo_url);
    args.push(&dest);

    // Clone from a neutral cwd (parent of destination) so git doesn't pick
    // up stray config from the caller's cwd.
    let cwd = destination.parent().unwrap_or_else(|| Path::new("."));
    run::run_authed(cwd, &args, auth).await?;

    info!(
        "Successfully cloned repository to {}",
        destination.display()
    );
    Ok(())
}

/// Clone `repo_url` into `workspace_root` if no `.git` exists yet; otherwise
/// call `ensure_initialized` (git init for a fresh directory, no-op for an
/// existing repo).
///
/// Clones into a temp sibling directory and moves contents in, because
/// [`clone_repo`] requires the destination to not exist while the project
/// root directory may already exist.
pub async fn clone_or_init(
    workspace_root: &Path,
    repo_url: Option<&str>,
    branch: &str,
    token: Option<&str>,
) -> Result<(), OxyError> {
    if repo::is_git_repo(workspace_root) {
        info!(
            "Git repo already exists at {}, skipping clone/init",
            workspace_root.display()
        );
        return Ok(());
    }

    if let Some(url) = repo_url {
        info!(
            "Cloning {} (branch: {}) into {}",
            url,
            branch,
            workspace_root.display()
        );
        let parent = workspace_root
            .parent()
            .ok_or_else(|| OxyError::IOError("Project root has no parent".to_string()))?;
        let tmp_name = format!(
            ".oxy-clone-tmp-{}",
            workspace_root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        );
        let tmp_dest = parent.join(&tmp_name);

        let auth = token.map(Auth::bearer).unwrap_or(Auth::None);
        clone_repo(url, &tmp_dest, Some(branch), &auth).await?;

        let mut read_dir = tokio::fs::read_dir(&tmp_dest)
            .await
            .map_err(|e| OxyError::IOError(format!("Failed to read cloned directory: {e}")))?;
        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|e| OxyError::IOError(format!("Failed to iterate cloned directory: {e}")))?
        {
            let src = entry.path();
            let dst = workspace_root.join(entry.file_name());
            if tokio::fs::rename(&src, &dst).await.is_err() {
                // rename can fail cross-device (Docker mounts); fall back to copy+delete.
                worktree::copy_recursive(&src, &dst).await.map_err(|ce| {
                    OxyError::IOError(format!(
                        "Failed to copy {} to {}: {ce}",
                        src.display(),
                        dst.display()
                    ))
                })?;
                let _ = tokio::fs::remove_dir_all(&src).await;
            }
        }
        tokio::fs::remove_dir(&tmp_dest).await.ok();

        info!(
            "Successfully cloned repository into {}",
            workspace_root.display()
        );
    } else {
        repo::ensure_initialized(workspace_root).await?;
    }

    Ok(())
}
