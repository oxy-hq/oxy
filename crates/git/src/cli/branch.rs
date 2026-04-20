use std::path::Path;

use oxy_shared::errors::OxyError;

use crate::cli::{repo, run, worktree};

/// Validates that `branch` is a safe branch name before use in git
/// commands or filesystem paths.
///
/// Allowed characters: alphanumeric, `.`, `_`, `-`, `/`.
/// Disallowed: `..` sequences, leading `-` or `/`, null bytes.
pub fn validate_branch_name(branch: &str) -> Result<(), OxyError> {
    if branch.is_empty() {
        return Err(OxyError::RuntimeError(
            "Branch name cannot be empty".to_string(),
        ));
    }
    let valid = branch
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
        && !branch.contains("..")
        && !branch.starts_with('-')
        && !branch.starts_with('/');
    if valid {
        Ok(())
    } else {
        Err(OxyError::RuntimeError(format!(
            "Invalid branch name '{branch}': only alphanumeric, '.', '_', '-', '/' allowed; \
             must not start with '-' or '/', must not contain '..'"
        )))
    }
}

/// Returns the name of the currently checked-out branch in `workspace_root`.
/// Returns `"HEAD@{sha}"` when detached.
pub async fn get_current_branch(workspace_root: &Path) -> Result<String, OxyError> {
    let out = run::run(workspace_root, &["branch", "--show-current"]).await?;
    let b = out.trim().to_string();
    if b.is_empty() {
        let sha = run::run(workspace_root, &["rev-parse", "--short", "HEAD"]).await?;
        return Ok(format!("HEAD@{}", sha.trim()));
    }
    Ok(b)
}

/// Returns `true` if `branch` exists as a local branch in `workspace_root`.
pub(crate) async fn branch_exists(workspace_root: &Path, branch: &str) -> Result<bool, OxyError> {
    let out = run::run(
        workspace_root,
        &["branch", "--list", branch, "--format=%(refname:short)"],
    )
    .await?;
    Ok(!out.trim().is_empty())
}

/// Fast-forwards `branch` in `root` from the remote by running
/// `git fetch origin {branch}:{branch}`.
pub async fn fetch_branch_ref(
    root: &Path,
    branch: &str,
    token: Option<&str>,
) -> Result<(), OxyError> {
    validate_branch_name(branch)?;
    let refspec = format!("{branch}:{branch}");
    run::run_with_token(root, &["fetch", "origin", &refspec], token).await?;
    Ok(())
}

/// Lists local branches with their sync status relative to their upstream.
///
/// Returns `(branch_name, sync_status)` where `sync_status` is
/// `"behind"` or `"synced"`.
pub async fn list_branches_with_status(workspace_root: &Path) -> Vec<(String, String)> {
    let default_branch = repo::get_default_branch(workspace_root).await;

    let output = run::run(
        workspace_root,
        &[
            "for-each-ref",
            "--format=%(refname:short)|%(upstream:trackshort)",
            "refs/heads/",
        ],
    )
    .await
    .unwrap_or_default();

    output
        .trim()
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(2, '|');
            let name = parts.next()?.trim().to_string();
            if name.is_empty() {
                return None;
            }
            if name != default_branch {
                let has_worktree = worktree::get_worktree_path(workspace_root, &name)
                    .map(|p| p.exists())
                    .unwrap_or(false);
                if !has_worktree {
                    return None;
                }
            }
            let track = parts.next().unwrap_or("").trim();
            let status = if track == "<" || track == "<>" {
                "behind"
            } else {
                "synced"
            };
            Some((name, status.to_string()))
        })
        .collect()
}

/// Fetches from origin (best-effort) and returns all branch names the user
/// can check out — both local branches and remote-only branches (stripped
/// of the `origin/` prefix, deduplicated).
pub async fn list_all_branches(
    workspace_root: &Path,
    token: Option<&str>,
) -> Result<Vec<String>, OxyError> {
    if repo::has_remote(workspace_root).await {
        let _ = run::run_with_token(workspace_root, &["fetch", "--prune", "origin"], token).await;
    }

    let local_out = run::run(workspace_root, &["branch", "--format=%(refname:short)"])
        .await
        .unwrap_or_default();
    let mut branches: Vec<String> = local_out
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();

    let remote_out = run::run(
        workspace_root,
        &["branch", "-r", "--format=%(refname:short)"],
    )
    .await
    .unwrap_or_default();
    for line in remote_out.lines() {
        let b = line.trim();
        if b.is_empty() {
            continue;
        }
        let name = b.strip_prefix("origin/").unwrap_or(b);
        if name == "HEAD" {
            continue;
        }
        if !branches.iter().any(|local| local == name) {
            branches.push(name.to_string());
        }
    }

    branches.sort();
    Ok(branches)
}

/// Checks out `branch`.  If the branch only exists on the remote, creates
/// a local tracking branch first.
pub async fn checkout_branch(
    workspace_root: &Path,
    branch: &str,
    token: Option<&str>,
) -> Result<(), OxyError> {
    validate_branch_name(branch)?;
    if repo::has_remote(workspace_root).await {
        let _ = run::run_with_token(workspace_root, &["fetch", "--prune", "origin"], token).await;
    }

    let local_exists = branch_exists(workspace_root, branch).await?;
    if local_exists {
        run::run(workspace_root, &["checkout", branch]).await?;
    } else {
        let remote_ref = format!("origin/{}", branch);
        let remote_exists = run::run(workspace_root, &["rev-parse", "--verify", &remote_ref])
            .await
            .is_ok();

        if remote_exists {
            run::run(workspace_root, &["checkout", "-b", branch, &remote_ref]).await?;
        } else {
            run::run(workspace_root, &["checkout", "-b", branch]).await?;
        }
    }
    Ok(())
}

/// Removes the worktree for `branch` (if any) and deletes the local branch ref.
pub async fn delete_branch(workspace_root: &Path, branch: &str) -> Result<(), OxyError> {
    validate_branch_name(branch)?;
    if let Some(wt_path) = worktree::get_worktree_path(workspace_root, branch) {
        run::run(
            workspace_root,
            &["worktree", "remove", "--force", &wt_path.to_string_lossy()],
        )
        .await?;
    }
    run::run(workspace_root, &["branch", "-D", branch]).await?;
    Ok(())
}
