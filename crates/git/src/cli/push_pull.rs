use std::path::Path;

use oxy_shared::errors::OxyError;
use tracing::info;

use crate::cli::{branch, run};

/// Push the current branch in `root` to its upstream remote.
///
/// `push.autoSetupRemote=true` is passed transiently so the first push of a
/// new branch creates the upstream tracking ref without permanently mutating
/// `~/.gitconfig`.
pub async fn push_to_remote(root: &Path, token: Option<&str>) -> Result<(), OxyError> {
    let b = branch::get_current_branch(root).await?;
    info!("Pushing branch '{}' in {} to remote", b, root.display());

    run::run_with_token(
        root,
        &["-c", "push.autoSetupRemote=true", "push", "origin", &b],
        token,
    )
    .await?;
    info!("Push successful");
    Ok(())
}

/// Force-pushes the current branch using `--force-with-lease`.
pub async fn force_push_to_remote(root: &Path, token: Option<&str>) -> Result<(), OxyError> {
    let b = branch::get_current_branch(root).await?;
    info!(
        "Force-pushing branch '{}' in {} to remote",
        b,
        root.display()
    );
    run::run_with_token(
        root,
        &[
            "-c",
            "push.autoSetupRemote=true",
            "push",
            "--force-with-lease",
            "origin",
            &b,
        ],
        token,
    )
    .await?;
    info!("Force push successful");
    Ok(())
}

/// `git fetch origin <branch>` — non-destructive: updates only the
/// remote-tracking ref `refs/remotes/origin/<branch>` (and `FETCH_HEAD`).
/// The local branch is never touched, so this is safe to call from a
/// worktree that is currently on `branch` regardless of divergence.
pub async fn fetch_remote_ref(
    root: &Path,
    branch: &str,
    token: Option<&str>,
) -> Result<(), OxyError> {
    branch::validate_branch_name(branch)?;
    info!("Fetching origin/{} in {}", branch, root.display());
    run::run_with_token(root, &["fetch", "origin", branch], token).await?;
    Ok(())
}

/// `git pull --rebase origin <branch>` inside a worktree.
///
/// Runs entirely inside `worktree_root` so rebase state is scoped to the
/// worktree's own gitdir and doesn't block other worktrees.
pub async fn pull_from_remote(
    worktree_root: &Path,
    branch: &str,
    token: Option<&str>,
) -> Result<(), OxyError> {
    info!("Pulling {} in {}", branch, worktree_root.display());
    run::run_with_token(
        worktree_root,
        &["pull", "--rebase", "origin", branch],
        token,
    )
    .await?;
    info!("Pull successful");
    Ok(())
}

/// Returns `true` if `local_sha` is behind `remote_sha`.  `remote_sha` not
/// being in the local object store is treated as "behind".
pub async fn is_behind_remote(root: &Path, local_sha: &str, remote_sha: &str) -> bool {
    get_ahead_behind_counts(root, local_sha, remote_sha).await.1 > 0
}

/// Returns `(ahead_count, behind_count)` of `local_sha` relative to `remote_sha`.
/// Returns `(0, 0)` if either SHA is empty. If `remote_sha` is missing from the
/// local object store, treats as fully behind.
pub async fn get_ahead_behind_counts(root: &Path, local_sha: &str, remote_sha: &str) -> (u64, u64) {
    if local_sha.is_empty() || remote_sha.is_empty() {
        return (0, 0);
    }
    if local_sha == remote_sha {
        return (0, 0);
    }
    let range = format!("{local_sha}...{remote_sha}");
    match run::run(root, &["rev-list", "--left-right", "--count", &range]).await {
        Ok(output) => {
            let mut parts = output.split_whitespace();
            let ahead = parts
                .next()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let behind = parts
                .next()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            (ahead, behind)
        }
        // rev-list failed (parse error, transient git, unreachable remote
        // SHA). Returning `(0, 1)` would surface a phantom ↓1 with an
        // enabled Pull button that just fails again; `(0, 0)` lets an
        // explicit Fetch recover real counts.
        Err(err) => {
            tracing::warn!(
                "get_ahead_behind_counts: rev-list failed for {range}: {err}; reporting (0, 0)"
            );
            (0, 0)
        }
    }
}

/// Returns the SHA that `origin/{branch}` points to (locally cached; no
/// network call).
pub async fn get_tracking_ref_sha(root: &Path, branch: &str) -> Option<String> {
    let tracking_ref = format!("origin/{branch}");
    run::run(root, &["rev-parse", &tracking_ref])
        .await
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Returns the URL of the `origin` remote, or `None` if not configured.
pub async fn get_remote_url(workspace_root: &Path) -> Option<String> {
    run::run(workspace_root, &["remote", "get-url", "origin"])
        .await
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
