use std::collections::HashMap;
use std::path::{Path, PathBuf};

use oxy_shared::errors::OxyError;
use tracing::info;

use crate::cli::{config, run};

/// Per-workspace cache for the default branch name, keyed on workspace root.
/// Computed on first call per workspace.  Multi-tenant deployments host
/// workspaces with different default branches, so a single process-global
/// value is insufficient.
static DEFAULT_BRANCH: std::sync::OnceLock<std::sync::Mutex<HashMap<PathBuf, String>>> =
    std::sync::OnceLock::new();

/// Returns `true` if `workspace_root` contains a `.git` directory or file.
pub fn is_git_repo(workspace_root: &Path) -> bool {
    workspace_root.join(".git").exists()
}

/// Initialises a git repository at `workspace_root` if one does not already
/// exist, then creates an initial commit so the repo has at least one
/// reachable commit on `main`.
///
/// No-op when `.git` already exists.
pub async fn ensure_initialized(workspace_root: &Path) -> Result<(), OxyError> {
    if is_git_repo(workspace_root) {
        info!(
            "Local git repo already exists at {}, skipping init",
            workspace_root.display()
        );
        return Ok(());
    }

    info!(
        "Initialising local git repo at {}",
        workspace_root.display()
    );

    run::run(workspace_root, &["init", "-b", "main"]).await?;
    config::ensure_user_config().await?;
    run::run(workspace_root, &["add", "-A"]).await?;

    // Empty project — initial commit is allowed to fail.
    let _ = run::run(
        workspace_root,
        &["commit", "-m", "Initial commit: Oxy project"],
    )
    .await;

    info!("Local git repo initialised at {}", workspace_root.display());
    Ok(())
}

/// Returns `true` if `workspace_root` has at least one configured git remote.
pub async fn has_remote(workspace_root: &Path) -> bool {
    run::run(workspace_root, &["remote"])
        .await
        .map(|out| !out.trim().is_empty())
        .unwrap_or(false)
}

/// Resolves the actual git directory for `root`.
///
/// For a regular repo, this is `root/.git/`.
/// For a git worktree, `root/.git` is a file containing `gitdir: <path>` —
/// we read that path so callers can find worktree-specific state.
pub(crate) fn resolve_git_dir(root: &Path) -> PathBuf {
    let dot_git = root.join(".git");
    if dot_git.is_file()
        && let Ok(content) = std::fs::read_to_string(&dot_git)
        && let Some(rel) = content.trim().strip_prefix("gitdir: ")
    {
        let resolved = root.join(rel);
        if let Ok(canonical) = resolved.canonicalize() {
            return canonical;
        }
    }
    dot_git
}

/// Returns the default branch name for `workspace_root`.
///
/// Resolution order:
/// 1. `GIT_DEFAULT_BRANCH` env var
/// 2. `git symbolic-ref --short refs/remotes/origin/HEAD`
/// 3. `"main"`
pub async fn get_default_branch(workspace_root: &Path) -> String {
    let cache = DEFAULT_BRANCH.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    if let Some(cached) = cache.lock().unwrap().get(workspace_root) {
        return cached.clone();
    }
    let value = if let Ok(b) = std::env::var("GIT_DEFAULT_BRANCH")
        && !b.is_empty()
    {
        b
    } else {
        match run::run(
            workspace_root,
            &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
        )
        .await
        {
            Ok(out) => {
                let s = out.trim().to_string();
                s.strip_prefix("origin/").map(str::to_string).unwrap_or(s)
            }
            Err(_) => "main".to_string(),
        }
    };
    cache
        .lock()
        .unwrap()
        .insert(workspace_root.to_path_buf(), value.clone());
    value
}
