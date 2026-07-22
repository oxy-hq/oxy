use axum::response::Json as ResponseJson;
use chrono::Utc;
use reqwest::StatusCode;
use tracing::{error, info};
use uuid::Uuid;

use oxy::adapters::workspace::effective_workspace_path;
use oxy::api_types::{
    BranchType, CommitEntry, ProjectBranch, RecentCommitsResponse, RevisionInfoResponse,
};
use oxy::github::{default_git_client, github_token_for_workspace};
use oxy_git::{GitClient, cli::repo::find_git_root};
use oxy_shared::errors::OxyError;

use super::dto::*;

/// Returns the value to put in `WorkspaceDetailsResponse.storage_key`.
/// `None` (cloud) yields the workspace UUID; `Some(path)` (local) yields
/// `local:{hash}` over the canonical, absolute, OS-encoded path bytes so
/// two `--local` sessions in different directories on the same dev port
/// don't collide and a dev alternating `--local` and cloud on the same
/// origin keeps separate keyspaces. Falls back through raw / current_dir
/// joins when `canonicalize` fails so the helper is total.
pub fn compute_workspace_storage_key(
    workspace_id: Uuid,
    local_path: Option<&std::path::Path>,
) -> String {
    match local_path {
        Some(path) => {
            use sha2::{Digest, Sha256};
            let normalized = std::fs::canonicalize(path).unwrap_or_else(|_| {
                if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    std::env::current_dir()
                        .map(|cwd| cwd.join(path))
                        .unwrap_or_else(|_| path.to_path_buf())
                }
            });
            let digest = Sha256::digest(normalized.as_os_str().as_encoded_bytes());
            format!("local:{}", &hex::encode(digest)[..16])
        }
        None => workspace_id.to_string(),
    }
}

pub(super) async fn workspace_root(
    ws: &entity::workspaces::Model,
) -> Result<std::path::PathBuf, StatusCode> {
    effective_workspace_path(ws, None).await.map_err(|e| {
        error!("Failed to resolve workspace path for {}: {}", ws.id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub(super) async fn git_fetch(
    worktree: &std::path::Path,
    branch: &str,
    workspace: &entity::workspaces::Model,
) -> Result<String, OxyError> {
    let git = default_git_client();
    if !git.has_remote(worktree).await {
        return Err(OxyError::RuntimeError(
            "No remote configured. Set GIT_REPOSITORY_URL to enable fetch.".to_string(),
        ));
    }
    let token = github_token_for_workspace(workspace).await?;
    git.fetch_remote_ref(worktree, branch, token.as_deref())
        .await?;
    Ok("Fetched latest from remote".to_string())
}

pub(super) async fn git_pull(
    worktree: &std::path::Path,
    branch: &str,
    workspace: &entity::workspaces::Model,
) -> Result<oxy_git::PullOutcome, OxyError> {
    let git = default_git_client();
    if !git.has_remote(worktree).await {
        return Err(OxyError::RuntimeError(
            "No remote configured. Set GIT_REPOSITORY_URL to enable pull.".to_string(),
        ));
    }
    let token = github_token_for_workspace(workspace).await?;
    git.pull_from_remote(worktree, branch, token.as_deref())
        .await
}

pub(super) async fn git_push(
    worktree: &std::path::Path,
    message: &str,
    workspace: &entity::workspaces::Model,
) -> Result<String, OxyError> {
    let git = default_git_client();
    // Mid-rebase push either fatals on detached HEAD (`HEAD@<sha>` refspec)
    // or commits garbage on top of the paused state.
    if git.is_in_conflict(worktree).await {
        return Err(OxyError::RuntimeError(
            "Cannot push during an in-progress rebase. Resolve conflicts or abort first.".into(),
        ));
    }
    if !message.is_empty() {
        git.commit_changes(worktree, message).await?;
    }
    if git.has_remote(worktree).await {
        let token = github_token_for_workspace(workspace).await?;
        git.push_to_remote(worktree, token.as_deref()).await?;
        Ok("Changes pushed to remote".to_string())
    } else {
        Ok("Changes committed successfully".to_string())
    }
}

pub(super) async fn git_force_push(
    worktree: &std::path::Path,
    workspace: &entity::workspaces::Model,
) -> Result<String, OxyError> {
    let git = default_git_client();
    if git.is_in_conflict(worktree).await {
        return Err(OxyError::RuntimeError(
            "Cannot force-push during an in-progress rebase. Resolve conflicts or abort first."
                .into(),
        ));
    }
    let token = github_token_for_workspace(workspace).await?;
    git.force_push_to_remote(worktree, token.as_deref()).await?;
    Ok("Force push successful".to_string())
}

pub(super) async fn git_revision_info(
    worktree: &std::path::Path,
    branch: &str,
) -> RevisionInfoResponse {
    let git = default_git_client();
    let (sha, message) = git.get_branch_commit(worktree, branch).await;
    let current_commit = if sha.is_empty() {
        String::new()
    } else {
        format!("{} - {}", &sha[..sha.len().min(7)], message)
    };

    let (tracking_sha, remote_url) = tokio::join!(
        git.get_tracking_ref_sha(worktree, branch),
        git.get_remote_url(worktree)
    );

    // `latest_sha`/`latest_commit` are display-only; empty string signals
    // "no upstream tracked yet" to the FE.
    let (latest_sha, latest_commit) = match tracking_sha.as_deref() {
        None => (String::new(), String::new()),
        Some(t) if t == sha => (sha.clone(), current_commit.clone()),
        Some(t) => {
            let (lsha, lmsg) = git.get_commit_by_sha(worktree, t).await;
            let display = if lsha.is_empty() {
                String::new()
            } else {
                format!("{} - {}", &lsha[..lsha.len().min(7)], lmsg)
            };
            (t.to_string(), display)
        }
    };

    let is_in_conflict = git.is_in_conflict(worktree).await;
    let (ahead_count, behind_count) = compute_ahead_behind(
        worktree,
        &sha,
        tracking_sha.as_deref(),
        remote_url.is_some(),
    )
    .await;

    let uncommitted_count = git
        .working_tree_status(worktree)
        .await
        .map(|entries| entries.len() as u64)
        .unwrap_or(0);

    let git_subfolder = find_git_root(worktree).and_then(|git_root| {
        // Canonicalize both paths so symlinks and `..` components don't
        // cause strip_prefix to return an empty or incorrect result.
        let canon_worktree = worktree
            .canonicalize()
            .unwrap_or_else(|_| worktree.to_path_buf());
        let canon_root = git_root.canonicalize().unwrap_or(git_root);
        canon_worktree
            .strip_prefix(&canon_root)
            .ok()
            .and_then(|p| p.to_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.replace('\\', "/"))
    });

    RevisionInfoResponse {
        base_sha: sha.clone(),
        head_sha: sha.clone(),
        current_revision: sha.clone(),
        latest_revision: latest_sha,
        current_commit,
        latest_commit,
        ahead_count,
        behind_count,
        uncommitted_count,
        is_in_conflict,
        last_sync_time: None,
        remote_url,
        git_subfolder,
    }
}

/// Compute `(ahead, behind)` of `local_sha` versus the upstream tracking
/// ref, with fallback logic for branches that have never been pushed.
///
/// - `tracking_sha = Some(t)`: returns `local..t` and `t..local` counts.
/// - `tracking_sha = None` with a remote: branch was forked locally but
///   never pushed; estimate `ahead` as commits unique to this branch vs
///   the default branch (size of the first push), `behind = 0`.
/// - No remote: returns `(0, 0)`.
pub(super) async fn compute_ahead_behind(
    worktree: &std::path::Path,
    local_sha: &str,
    tracking_sha: Option<&str>,
    has_remote: bool,
) -> (u64, u64) {
    if local_sha.is_empty() {
        return (0, 0);
    }
    let git = default_git_client();
    if let Some(t) = tracking_sha {
        return git.get_ahead_behind_counts(worktree, local_sha, t).await;
    }
    if !has_remote {
        return (0, 0);
    }
    let default_branch = git.get_default_branch(worktree).await;
    let (default_sha, _) = git.get_branch_commit(worktree, &default_branch).await;
    if default_sha.is_empty() || default_sha == local_sha {
        return (0, 0);
    }
    let (ahead, _behind_default) = git
        .get_ahead_behind_counts(worktree, local_sha, &default_sha)
        .await;
    (ahead, 0)
}

/// Builds the workspace branch list with merged local + remote names so
/// remote-only branches show up in the picker. Each branch carries its
/// `BranchOrigin` for FE badging.
pub(super) async fn git_list_branches(
    root: &std::path::Path,
    workspace: Option<&entity::workspaces::Model>,
    workspace_id: Uuid,
) -> Vec<ProjectBranch> {
    let git = default_git_client();
    let token = if let Some(ws) = workspace {
        github_token_for_workspace(ws).await.ok().flatten()
    } else {
        None
    };
    let infos = git.list_branches_with_origin(root, token.as_deref()).await;
    let now = Utc::now().to_string();
    infos
        .into_iter()
        .map(|info| {
            let branch_type = match info.origin {
                oxy_git::BranchOrigin::RemoteOnly => BranchType::Remote,
                _ => BranchType::Local,
            };
            ProjectBranch {
                id: Uuid::nil(),
                name: info.name,
                revision: String::new(),
                workspace_id,
                branch_type,
                origin: info.origin.into(),
                created_at: now.clone(),
                updated_at: now.clone(),
            }
        })
        .collect()
}

/// Switches the IDE-local branch. Materialises a local ref via
/// `ensure_local_ref` (no checkout) before `worktree add` so the main
/// worktree's HEAD and working tree are untouched.
pub(super) async fn git_switch_branch(
    root: &std::path::Path,
    branch: &str,
    workspace: Option<&entity::workspaces::Model>,
    workspace_id: Uuid,
) -> Result<ProjectBranch, OxyError> {
    let git = default_git_client();
    git.ensure_initialized(root).await?;

    let token = if let Some(ws) = workspace {
        github_token_for_workspace(ws).await.ok().flatten()
    } else {
        None
    };
    let resolved_origin = git.ensure_local_ref(root, branch, token.as_deref()).await?;

    git.get_or_create_worktree(root, branch).await?;
    let now = Utc::now().to_string();
    Ok(ProjectBranch {
        id: Uuid::nil(),
        workspace_id,
        branch_type: BranchType::Local,
        name: branch.to_string(),
        revision: String::new(),
        origin: resolved_origin.into(),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub(super) async fn git_delete_branch(
    root: &std::path::Path,
    branch: &str,
) -> Result<(), OxyError> {
    let git = default_git_client();
    let default_branch = git.get_default_branch(root).await;
    if branch == default_branch {
        return Err(OxyError::RuntimeError(format!(
            "Cannot delete the default branch '{default_branch}'"
        )));
    }
    // Lenient validation so legacy `--` branches (predating the strict
    // rule) can still be cleaned up.
    git.delete_branch(root, branch).await
}

pub(super) async fn git_recent_commits(
    worktree: &std::path::Path,
    limit: usize,
    offset: usize,
) -> RecentCommitsResponse {
    let git = default_git_client();
    // Fetch one extra row so `has_more` is known without a second `git log`.
    let raw = git.get_recent_commits(worktree, limit + 1, offset).await;
    let has_more = raw.len() > limit;
    let commits = raw.into_iter().take(limit).map(commit_entry).collect();
    RecentCommitsResponse { commits, has_more }
}

pub(super) fn commit_entry(c: oxy_git::RecentCommit) -> CommitEntry {
    CommitEntry {
        hash: c.hash,
        short_hash: c.short_hash,
        message: c.subject,
        author: c.author,
        date: c.relative_date,
        on_remote: c.on_remote,
    }
}

/// Resolve the branch name from `?branch=`, falling back to the repo's default.
pub(super) async fn resolve_branch(query_branch: Option<String>, root: &std::path::Path) -> String {
    match query_branch.filter(|b| !b.is_empty()) {
        Some(b) => b,
        None => default_git_client().get_default_branch(root).await,
    }
}

/// Build a `WorkspaceDetailsResponse` in one of the two "no git visible"
/// shapes. Shared between the missing-directory branch and the
/// local-mode short-circuit so a future field addition only lands in
/// one place.
pub(super) fn no_git_response(
    workspace_id: Uuid,
    name: &str,
    now: String,
    workspace_error: Option<String>,
    requires_local_setup: bool,
    current_user_role: String,
    storage_key: String,
) -> ResponseJson<WorkspaceDetailsResponse> {
    let mode = GitMode::None;
    ResponseJson(WorkspaceDetailsResponse {
        id: workspace_id,
        name: name.to_string(),
        workspace_id: Uuid::nil(),
        created_at: now.clone(),
        updated_at: now,
        active_branch: None,
        workspace_error,
        git_mode: mode,
        capabilities: mode.into(),
        default_branch: "main".to_string(),
        protected_branches: vec!["main".to_string()],
        requires_local_setup,
        current_user_role,
        storage_key,
    })
}

/// Response builder for the local-mode "no config.yml yet" case. Exposed
/// publicly so integration tests can assert the shape without spinning up
/// the full router + DB.
pub fn build_workspace_details_response_for_uninitialized_local(
    workspace_id: Uuid,
    name: &str,
    current_user_role: String,
    storage_key: String,
) -> ResponseJson<WorkspaceDetailsResponse> {
    let now = chrono::Utc::now().to_string();
    no_git_response(
        workspace_id,
        name,
        now,
        None,
        true,
        current_user_role,
        storage_key,
    )
}

/// Count files whose name ends with `.<suffix>.yml` under `dir` (recursive, skips hidden dirs).
pub(super) fn count_yml_suffix(dir: &std::path::Path, suffix: &str) -> usize {
    let pattern = format!(".{suffix}.yml");
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                let hidden = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with('.'))
                    .unwrap_or(false);
                if !hidden {
                    count += count_yml_suffix(&path, suffix);
                }
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.ends_with(&pattern))
                .unwrap_or(false)
            {
                count += 1;
            }
        }
    }
    count
}

/// Best-effort removal of a workspace's schedule rows, to be called *after* the
/// workspace has been deleted. Schedules carry a plain `workspace_id` with no FK,
/// so nothing cascades — an orphaned `health_eval` row keeps firing health-eval
/// tasks for the deleted workspace and piles them up in the dead-letter queue
/// (`monitor_scan` rows do the same). Shared by every workspace-removal path
/// (this handler, the admin delete, and org deletion) so they stay in sync.
/// Logged, never fatal.
pub(crate) async fn cleanup_workspace_schedules(
    db: &sea_orm::DatabaseConnection,
    workspace_id: Uuid,
) {
    match agentic_pipeline::scheduler::delete_workspace_schedules(db, workspace_id).await {
        Ok(n) if n > 0 => info!("Removed {} schedule(s) for workspace {}", n, workspace_id),
        Ok(_) => {}
        Err(e) => tracing::warn!(
            "Failed to delete schedules for workspace {}: {}",
            workspace_id,
            e
        ),
    }
}
