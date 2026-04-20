use std::path::Path;

use oxy_shared::errors::OxyError;
use tracing::info;

use crate::cli::{config, run};

/// Stages all changes in `root` and creates a commit with `message`.
///
/// Returns the short commit SHA, or an empty string when there was nothing
/// to commit.
pub async fn commit_changes(root: &Path, message: &str) -> Result<String, OxyError> {
    config::ensure_user_config().await?;

    run::run(root, &["add", "-A"]).await?;

    let status = run::run(root, &["status", "--porcelain"]).await?;
    if status.trim().is_empty() {
        info!("No changes to commit in {}", root.display());
        return Ok(String::new());
    }

    run::run(root, &["commit", "-m", message]).await?;

    let sha = run::run(root, &["rev-parse", "--short", "HEAD"]).await?;
    let sha = sha.trim().to_string();
    info!("Committed '{}' in {} ({})", message, root.display(), sha);
    Ok(sha)
}

/// Returns the human-readable relative date of the HEAD commit (e.g. "3 hours ago").
/// Returns `None` when the repo has no commits or is not a git repo.
pub async fn get_head_commit_relative_date(root: &Path) -> Option<String> {
    match run::run(root, &["log", "-1", "--format=%ar"]).await {
        Ok(out) => {
            let s = out.trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        }
        Err(_) => None,
    }
}

/// Returns the N most recent commits on the current branch.
///
/// Each entry: (full hash, short hash, subject, author, relative date).
pub async fn get_recent_commits(
    root: &Path,
    n: usize,
) -> Vec<(String, String, String, String, String)> {
    let n_str = n.to_string();
    match run::run(
        root,
        &["log", &format!("-{n_str}"), "--format=%H|%h|%s|%an|%ar"],
    )
    .await
    {
        Ok(out) => out
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() {
                    return None;
                }
                let parts: Vec<&str> = line.splitn(5, '|').collect();
                if parts.len() < 5 {
                    return None;
                }
                Some((
                    parts[0].to_string(),
                    parts[1].to_string(),
                    parts[2].to_string(),
                    parts[3].to_string(),
                    parts[4].to_string(),
                ))
            })
            .collect(),
        Err(_) => vec![],
    }
}

/// Returns `(full_sha, subject)` for a specific commit, or `("", "")` if missing.
pub async fn get_commit_by_sha(root: &Path, sha: &str) -> (String, String) {
    if sha.is_empty() {
        return (String::new(), String::new());
    }
    match run::run(root, &["log", "-1", "--format=%H|%s", sha]).await {
        Ok(out) => split_sha_subject(&out),
        Err(_) => (String::new(), String::new()),
    }
}

/// Returns the tip commit SHA and subject line for `branch` by reading
/// `refs/heads/{branch}` directly.  Returns `("", "")` when the branch does
/// not exist locally.
pub async fn get_branch_commit(root: &Path, branch: &str) -> (String, String) {
    let refspec = format!("refs/heads/{branch}");
    match run::run(root, &["log", "-1", "--format=%H|%s", &refspec]).await {
        Ok(out) => split_sha_subject(&out),
        Err(_) => (String::new(), String::new()),
    }
}

fn split_sha_subject(out: &str) -> (String, String) {
    let line = out.trim();
    if line.is_empty() {
        return (String::new(), String::new());
    }
    let mut parts = line.splitn(2, '|');
    let sha = parts.next().unwrap_or("").to_string();
    let msg = parts.next().unwrap_or("").to_string();
    (sha, msg)
}
