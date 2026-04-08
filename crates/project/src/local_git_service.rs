//! Local-mode git branching service.
//!
//! Provides git operations for local (non-cloud) deployments using git worktrees,
//! enabling per-user branch isolation.  Oxy owns the full git lifecycle: initial
//! clone (or init), per-branch worktrees, auto-commit on save, explicit commit,
//! and push/pull when a remote is configured.
//!
//! # Remote authentication
//!
//! Remote operations are authenticated via the following environment variables
//! (in priority order):
//!
//! 1. **GitHub App** — `GITHUB_APP_ID` + `GITHUB_APP_PRIVATE_KEY`.  The
//!    installation ID is discovered automatically via `GET /app/installations`.
//!    If you have multiple installations you can pin one with the optional
//!    `GITHUB_APP_INSTALLATION_ID` override.
//! 2. **Personal access token** — `GITHUB_TOKEN` (fallback).
//!
//! Tokens are **never persisted** to `.git/config`; they are passed via the
//! `http.extraHeader` git config argument on each individual command.
//!
//! # Architecture
//!
//! ```text
//! project/
//!   .git/
//!   agents/
//!   workflows/
//!   .worktrees/
//!     user--alice/      ← git worktree for branch user/alice
//!     feature--my-wf/   ← git worktree for branch feature/my-workflow
//! ```

use oxy::github::{FileStatus, GitHubAppAuth, GitOperations};
use oxy_shared::errors::OxyError;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{info, warn};

/// Directory name for git worktrees inside the project root.
const WORKTREES_DIR: &str = ".worktrees";

/// Process-lifetime cache for the default branch name.
/// Computed once on first call; subsequent calls are free.
static DEFAULT_BRANCH: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();

/// Git operations for local (non-cloud) Oxy deployments.
///
/// Uses the system `git` binary via `tokio::process::Command` — the same
/// approach as [`oxy::github::GitOperations`] — so no `git2` crate is
/// needed here and the library stays slim.
pub struct LocalGitService;

impl LocalGitService {
    // ─── Repository helpers ────────────────────────────────────────────────

    /// Returns `true` if `workspace_root` contains a `.git` directory.
    pub fn is_git_repo(workspace_root: &Path) -> bool {
        workspace_root.join(".git").exists()
    }

    /// Initialises a git repository at `workspace_root` if one does not already
    /// exist, then creates an initial commit so the repo has at least one
    /// reachable commit on `main`.
    ///
    /// This is a **no-op** when `.git` already exists, making it safe to call
    /// even when git-sync has already cloned the repo.
    pub async fn ensure_initialized(workspace_root: &Path) -> Result<(), OxyError> {
        if Self::is_git_repo(workspace_root) {
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

        Self::run_git(workspace_root, &["init", "-b", "main"]).await?;
        GitOperations::ensure_git_config().await?;
        Self::run_git(workspace_root, &["add", "-A"]).await?;

        // Allow the initial commit to fail if there is nothing to commit
        // (empty project directory).
        let _ = Self::run_git(
            workspace_root,
            &["commit", "-m", "Initial commit: Oxy project"],
        )
        .await;

        info!("Local git repo initialised at {}", workspace_root.display());
        Ok(())
    }

    // ─── Branch inspection ─────────────────────────────────────────────────

    /// Returns the name of the currently checked-out branch in `workspace_root`.
    pub async fn get_current_branch(workspace_root: &Path) -> Result<String, OxyError> {
        let out = Self::run_git(workspace_root, &["branch", "--show-current"]).await?;
        let branch = out.trim().to_string();
        if branch.is_empty() {
            // Detached HEAD — fall back to rev-parse
            let sha = Self::run_git(workspace_root, &["rev-parse", "--short", "HEAD"]).await?;
            return Ok(format!("HEAD@{}", sha.trim()));
        }
        Ok(branch)
    }

    /// Returns the HEAD commit's full SHA and subject line in `root`.
    ///
    /// Returns `("", "")` when the repo has no commits yet.
    pub async fn get_head_commit(root: &Path) -> (String, String) {
        match Self::run_git(root, &["log", "-1", "--format=%H|%s"]).await {
            Ok(out) => {
                let line = out.trim().to_string();
                if line.is_empty() {
                    return (String::new(), String::new());
                }
                let mut parts = line.splitn(2, '|');
                let sha = parts.next().unwrap_or("").to_string();
                let msg = parts.next().unwrap_or("").to_string();
                (sha, msg)
            }
            Err(_) => (String::new(), String::new()),
        }
    }

    /// Returns the human-readable relative date of the HEAD commit (e.g. "3 hours ago").
    /// Returns `None` when the repo has no commits or is not a git repo.
    pub async fn get_head_commit_relative_date(root: &Path) -> Option<String> {
        match Self::run_git(root, &["log", "-1", "--format=%ar"]).await {
            Ok(out) => {
                let s = out.trim().to_string();
                if s.is_empty() { None } else { Some(s) }
            }
            Err(_) => None,
        }
    }

    /// Returns the N most recent commits on the current branch.
    ///
    /// Each entry contains the full hash, short hash, commit subject, author name,
    /// and a human-readable relative date (e.g. "3 hours ago").
    pub async fn get_recent_commits(
        root: &Path,
        n: usize,
    ) -> Vec<(String, String, String, String, String)> {
        let n_str = n.to_string();
        match Self::run_git(
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
                        parts[0].to_string(), // full hash
                        parts[1].to_string(), // short hash
                        parts[2].to_string(), // subject
                        parts[3].to_string(), // author
                        parts[4].to_string(), // relative date
                    ))
                })
                .collect(),
            Err(_) => vec![],
        }
    }

    /// Returns the short SHA and subject line for a specific commit object.
    ///
    /// Returns `("", "")` when `sha` is not in the local object store.
    pub async fn get_commit_by_sha(root: &Path, sha: &str) -> (String, String) {
        if sha.is_empty() {
            return (String::new(), String::new());
        }
        match Self::run_git(root, &["log", "-1", "--format=%H|%s", sha]).await {
            Ok(out) => {
                let line = out.trim().to_string();
                if line.is_empty() {
                    return (String::new(), String::new());
                }
                let mut parts = line.splitn(2, '|');
                let full_sha = parts.next().unwrap_or("").to_string();
                let msg = parts.next().unwrap_or("").to_string();
                (full_sha, msg)
            }
            Err(_) => (String::new(), String::new()),
        }
    }

    /// Returns the tip commit SHA and subject line for `branch` by reading
    /// `refs/heads/{branch}` directly.
    ///
    /// Unlike `get_head_commit`, this works even when `root` is checked out
    /// on a *different* branch — e.g. `workspace_root` is on "test" but the
    /// caller wants info about the "main" branch.
    ///
    /// Returns `("", "")` when the branch does not exist locally.
    pub async fn get_branch_commit(root: &Path, branch: &str) -> (String, String) {
        let refspec = format!("refs/heads/{branch}");
        match Self::run_git(root, &["log", "-1", "--format=%H|%s", &refspec]).await {
            Ok(out) => {
                let line = out.trim().to_string();
                if line.is_empty() {
                    return (String::new(), String::new());
                }
                let mut parts = line.splitn(2, '|');
                let sha = parts.next().unwrap_or("").to_string();
                let msg = parts.next().unwrap_or("").to_string();
                (sha, msg)
            }
            Err(_) => (String::new(), String::new()),
        }
    }

    /// Fast-forwards `branch` in `root` from the remote by running
    /// `git fetch origin {branch}:{branch}`.
    ///
    /// This updates the local branch ref without requiring it to be checked
    /// out, so it works even when `root` is on a different branch.
    /// Fails (non-fast-forward) if the local branch has diverged from remote.
    pub async fn fetch_branch_ref(
        root: &Path,
        branch: &str,
        token: Option<&str>,
    ) -> Result<(), OxyError> {
        let refspec = format!("{branch}:{branch}");
        Self::run_git_authed(root, &["fetch", "origin", &refspec], token).await?;
        Ok(())
    }

    /// Lists all local branch names in `workspace_root`.
    pub async fn list_local_branches(workspace_root: &Path) -> Result<Vec<String>, OxyError> {
        let out = Self::run_git(workspace_root, &["branch", "--format=%(refname:short)"]).await?;
        Ok(out
            .lines()
            .map(str::trim)
            .map(str::to_string)
            .filter(|s| !s.is_empty())
            .collect())
    }

    /// Lists local branches with their sync status relative to their upstream.
    ///
    /// Uses a single `git for-each-ref` call to query git's built-in tracking
    /// state — no per-branch subprocess calls needed.
    ///
    /// Returns `(branch_name, sync_status)` pairs where `sync_status` is one of:
    /// - `"behind"` — upstream has commits the local branch hasn't pulled
    /// - `"synced"` — up to date, ahead, or no upstream configured
    pub async fn list_branches_with_status(workspace_root: &Path) -> Vec<(String, String)> {
        let default_branch = Self::get_default_branch(workspace_root).await;

        let output = Self::run_git(
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
                // Only include the default branch or branches that have an
                // active worktree on disk.  This hides refs that were fetched
                // from the remote but never checked out locally.
                if name != default_branch {
                    let has_worktree = Self::get_worktree_path(workspace_root, &name)
                        .map(|p| p.exists())
                        .unwrap_or(false);
                    if !has_worktree {
                        return None;
                    }
                }
                // %(upstream:trackshort): "<" = behind, ">" = ahead,
                // "<>" = diverged, "=" = in sync, "" = no upstream
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

    // ─── Worktree management ───────────────────────────────────────────────

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

    /// Converts a branch name to a safe directory name.
    ///
    /// `/` is encoded as `--` so that distinct branch names always map to
    /// distinct directory names (e.g. `user/alice` → `user--alice` cannot
    /// collide with the literal branch `user-alice`).
    ///
    /// Examples:
    /// - `user/alice`         → `user--alice`
    /// - `feature/my-wf`     → `feature--my-wf`
    /// - `user-alice`         → `user-alice`
    pub fn branch_to_dir_name(branch: &str) -> String {
        branch.replace('/', "--")
    }

    /// Returns the worktree path for `branch` if it exists on disk.
    pub fn get_worktree_path(workspace_root: &Path, branch: &str) -> Option<PathBuf> {
        if branch.is_empty() {
            return None;
        }
        let dir = Self::branch_to_dir_name(branch);
        let path = workspace_root.join(WORKTREES_DIR).join(&dir);
        if path.exists() { Some(path) } else { None }
    }

    /// Returns the worktree path for `branch`, creating the worktree (and the
    /// branch, if it does not already exist) when necessary.
    ///
    /// The branch is always forked from `HEAD` of the main project directory,
    /// so the new branch starts with a clean copy of the current project state.
    pub async fn get_or_create_worktree(
        workspace_root: &Path,
        branch: &str,
    ) -> Result<PathBuf, OxyError> {
        let default_branch = Self::get_default_branch(workspace_root).await;
        if branch.is_empty() || branch == default_branch {
            return Ok(workspace_root.to_path_buf());
        }

        Self::validate_branch_name(branch)?;

        let dir_name = Self::branch_to_dir_name(branch);
        let worktree_path = workspace_root.join(WORKTREES_DIR).join(&dir_name);

        if worktree_path.exists() {
            return Ok(worktree_path);
        }

        // Ensure .worktrees/ directory exists
        tokio::fs::create_dir_all(workspace_root.join(WORKTREES_DIR))
            .await
            .map_err(|e| OxyError::IOError(format!("Failed to create .worktrees dir: {e}")))?;

        GitOperations::ensure_git_config().await?;

        // Determine whether the branch already exists locally
        let branch_exists = Self::branch_exists(workspace_root, branch).await?;

        let result = if branch_exists {
            // Attach existing branch to a new worktree
            Self::run_git(
                workspace_root,
                &["worktree", "add", &worktree_path.to_string_lossy(), branch],
            )
            .await
        } else {
            // Create a new branch and worktree in one step
            Self::run_git(
                workspace_root,
                &[
                    "worktree",
                    "add",
                    "-b",
                    branch,
                    &worktree_path.to_string_lossy(),
                ],
            )
            .await
        };

        match result {
            Ok(_) => {}
            Err(e) => {
                // If the worktree already exists on disk (concurrent request),
                // treat it as success rather than propagating the error.
                if worktree_path.exists() {
                    info!(
                        "Worktree at {} already exists (concurrent creation), using it",
                        worktree_path.display()
                    );
                } else {
                    return Err(e);
                }
            }
        }

        info!(
            "Created git worktree '{}' at {}",
            branch,
            worktree_path.display()
        );
        Ok(worktree_path)
    }

    /// Remaps `path` from `workspace_root` to `worktree_root` by replacing the
    /// path prefix.
    ///
    /// If `path` is not under `workspace_root`, it is returned unchanged.
    pub fn remap_path_to_worktree(
        workspace_root: &Path,
        worktree_root: &Path,
        path: &Path,
    ) -> PathBuf {
        if let Ok(relative) = path.strip_prefix(workspace_root) {
            worktree_root.join(relative)
        } else {
            path.to_path_buf()
        }
    }

    // ─── Committing ────────────────────────────────────────────────────────

    /// Stages all changes in `root` and creates a commit with `message`.
    ///
    /// Returns the short commit SHA, or an empty string when there was nothing
    /// to commit.
    pub async fn commit_changes(root: &Path, message: &str) -> Result<String, OxyError> {
        GitOperations::ensure_git_config().await?;

        Self::run_git(root, &["add", "-A"]).await?;

        let status = Self::run_git(root, &["status", "--porcelain"]).await?;
        if status.trim().is_empty() {
            info!("No changes to commit in {}", root.display());
            return Ok(String::new());
        }

        Self::run_git(root, &["commit", "-m", message]).await?;

        let sha = Self::run_git(root, &["rev-parse", "--short", "HEAD"]).await?;
        let sha = sha.trim().to_string();
        info!("Committed '{}' in {} ({})", message, root.display(), sha);
        Ok(sha)
    }

    /// Stages `file_path` and auto-commits with a generated message.
    ///

    // ─── Diff ──────────────────────────────────────────────────────────────

    /// Returns the diff summary (file-level insert/delete counts) for `root`.
    ///
    /// Delegates to [`GitOperations::diff_numstat_summary`] which works on
    /// any git repository regardless of whether it has a remote.
    pub async fn diff_numstat_summary(root: &Path) -> Result<Vec<FileStatus>, OxyError> {
        GitOperations::diff_numstat_summary(root).await
    }

    /// Returns file-level insert/delete counts for commits that are ahead of
    /// the configured upstream (`@{upstream}...HEAD`).
    ///
    /// Returns an empty vec when no upstream is configured (the branch has
    /// never been pushed) rather than propagating an error.
    pub async fn diff_numstat_ahead(root: &Path) -> Result<Vec<FileStatus>, OxyError> {
        let range = "@{upstream}...HEAD";

        // If the branch has no upstream git exits non-zero — treat as "nothing ahead".
        let numstat = match Self::run_git(root, &["diff", "--numstat", range]).await {
            Ok(out) => out,
            Err(_) => return Ok(vec![]),
        };
        let name_status = match Self::run_git(root, &["diff", "--name-status", range]).await {
            Ok(out) => out,
            Err(_) => return Ok(vec![]),
        };

        // Build a lookup: destination path → (insertions, deletions).
        // numstat lines are tab-separated: "<ins>\t<del>\t<path>"
        let mut stat_map: HashMap<String, (u32, u32)> = HashMap::new();
        for line in numstat.trim().lines() {
            let parts: Vec<&str> = line.splitn(3, '\t').collect();
            if parts.len() >= 3 {
                let ins = parts[0].trim().parse::<u32>().unwrap_or(0);
                let del = parts[1].trim().parse::<u32>().unwrap_or(0);
                stat_map.insert(parts[2].trim().to_string(), (ins, del));
            }
        }

        // Parse name-status lines: "<STATUS>\t<path>"
        // Renames/copies have three columns: "<R|C><score>\t<old>\t<new>"
        let mut result = Vec::new();
        for line in name_status.trim().lines() {
            if line.trim().is_empty() {
                continue;
            }
            let mut cols = line.splitn(3, '\t');
            let status_char = cols.next().unwrap_or("").trim();
            let path = match status_char.chars().next() {
                Some('R') | Some('C') => {
                    cols.next(); // skip source path
                    cols.next().unwrap_or("").trim().to_string()
                }
                _ => cols.next().unwrap_or("").trim().to_string(),
            };
            if path.is_empty() {
                continue;
            }
            let status = match status_char.chars().next() {
                Some('A') => "A",
                Some('D') => "D",
                _ => "M",
            }
            .to_string();
            let (ins, del) = stat_map.get(&path).copied().unwrap_or((0, 0));
            result.push(FileStatus {
                path,
                status,
                insert: ins,
                delete: del,
            });
        }
        Ok(result)
    }

    // ─── Remote operations ─────────────────────────────────────────────────

    /// Try to obtain an access token for remote git operations.
    ///
    /// Resolution order:
    /// 1. GitHub App — `GITHUB_APP_ID` + `GITHUB_APP_PRIVATE_KEY`.
    ///    The installation ID is resolved by (a) reading the optional
    ///    `GITHUB_APP_INSTALLATION_ID` override or (b) auto-discovering via
    ///    `GET /app/installations` (takes the first result).
    /// 2. Personal access token — `GITHUB_TOKEN`
    ///
    /// Returns `None` when no credentials are configured (local-only mode).
    pub async fn get_remote_token() -> Option<String> {
        // Try GitHub App first
        if let (Ok(app_id), Ok(private_key)) = (
            env::var("GITHUB_APP_ID"),
            env::var("GITHUB_APP_PRIVATE_KEY"),
        ) {
            match GitHubAppAuth::new(app_id, private_key) {
                Ok(auth) => {
                    // Prefer an explicit override; otherwise auto-discover.
                    let installation_id = if let Ok(id) = env::var("GITHUB_APP_INSTALLATION_ID") {
                        Some(id)
                    } else {
                        match auth.list_installations().await {
                            Ok(list) if !list.is_empty() => {
                                if list.len() > 1 {
                                    warn!(
                                        "Multiple GitHub App installations found; using the first one ({}). \
                                         Set GITHUB_APP_INSTALLATION_ID to pin a specific installation.",
                                        list[0].id
                                    );
                                }
                                Some(list[0].id.to_string())
                            }
                            Ok(_) => {
                                warn!("GitHub App has no installations; cannot obtain token");
                                None
                            }
                            Err(e) => {
                                warn!("Failed to list GitHub App installations: {}", e);
                                None
                            }
                        }
                    };

                    if let Some(id) = installation_id {
                        match auth.get_installation_token(&id).await {
                            Ok(token) => {
                                info!("Obtained GitHub App installation token for git operations");
                                return Some(token);
                            }
                            Err(e) => {
                                warn!("Failed to get GitHub App installation token: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to create GitHubAppAuth: {}", e);
                }
            }
        }

        // Fallback to personal access token
        if let Ok(token) = env::var("GITHUB_TOKEN") {
            info!("Using GITHUB_TOKEN for git operations");
            return Some(token);
        }

        None
    }

    /// Queries the remote for `branch`'s HEAD SHA without downloading any objects
    /// (`git ls-remote`).
    ///
    /// Authentication priority:
    /// 1. Configured credentials — `get_remote_token()` (GitHub App → GITHUB_TOKEN).
    /// 2. Host machine's git credential helpers / SSH keys — used automatically when
    ///    no Oxy credentials are configured.
    ///
    /// Returns `None` when there is no remote, the branch doesn't exist remotely,
    /// or the network call fails.
    pub async fn get_remote_head(root: &Path, branch: &str) -> Option<String> {
        if !Self::has_remote(root).await {
            return None;
        }

        let token = Self::get_remote_token().await;
        match Self::run_git_authed(
            root,
            &["ls-remote", "--quiet", "origin", branch],
            token.as_deref(),
        )
        .await
        {
            Ok(output) => output
                .split_whitespace()
                .next()
                .map(str::to_string)
                .filter(|s| !s.is_empty()),
            Err(_) => None,
        }
    }

    /// Returns `true` if `local_sha` is behind `remote_sha`.
    ///
    /// Uses `git rev-list --count {local_sha}..{remote_sha}` to count commits the
    /// remote has that local does not.  Passing the branch tip SHA explicitly
    /// (rather than `HEAD`) works correctly even when `root` is checked out on a
    /// different branch.
    ///
    /// If `remote_sha` is not in the local object store the command fails, which
    /// is treated as "behind" (the remote has changes we have never downloaded).
    pub async fn is_behind_remote(root: &Path, local_sha: &str, remote_sha: &str) -> bool {
        if remote_sha.is_empty() || local_sha.is_empty() {
            return false;
        }
        let range = format!("{local_sha}..{remote_sha}");
        match Self::run_git(root, &["rev-list", "--count", &range]).await {
            Ok(output) => output.trim().parse::<u64>().unwrap_or(1) > 0,
            // SHA not in local object store → we haven't fetched it yet → behind
            Err(_) => true,
        }
    }

    /// Returns the SHA that `origin/{branch}` (the local tracking ref) points to.
    ///
    /// Unlike `get_remote_head`, this makes **no network call** — it reads the
    /// locally-cached ref that was last updated by `git fetch` / `git pull`.
    /// Returns `None` when no tracking ref exists (branch never fetched).
    pub async fn get_tracking_ref_sha(root: &Path, branch: &str) -> Option<String> {
        let tracking_ref = format!("origin/{branch}");
        Self::run_git(root, &["rev-parse", &tracking_ref])
            .await
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Returns the URL of the `origin` remote, or `None` if not configured.
    pub async fn get_remote_url(workspace_root: &Path) -> Option<String> {
        Self::run_git(workspace_root, &["remote", "get-url", "origin"])
            .await
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Resolves the actual git directory for `root`.
    ///
    /// For a regular repo, this is `root/.git/`.
    /// For a git worktree, `root/.git` is a file containing `gitdir: <path>` —
    /// we read that path so callers can find worktree-specific state (rebase-merge, etc.).
    fn resolve_git_dir(root: &Path) -> PathBuf {
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

    /// Returns `true` if `root` is mid-rebase **or** mid-merge with conflicts.
    ///
    /// Works for both regular repos and git worktrees — worktrees store their
    /// rebase state in `.git/worktrees/{name}/rebase-merge/`, not in the
    /// worktree's own `.git` file path.  Merge conflicts produce a `MERGE_HEAD`
    /// file in the same gitdir.
    pub fn is_in_conflict(root: &Path) -> bool {
        let git_dir = Self::resolve_git_dir(root);
        git_dir.join("rebase-merge").exists()
            || git_dir.join("rebase-apply").exists()
            || git_dir.join("MERGE_HEAD").exists()
    }

    /// Restores all tracked files to their state at `commit` and creates a new
    /// "Restore to …" commit on top of the current HEAD.  History is preserved
    /// and never rewritten — this is a forward commit, not a destructive reset.
    ///
    /// If an in-progress rebase or merge is active it is aborted first so that
    /// state files (rebase-merge/, MERGE_HEAD) are cleaned up.
    pub async fn reset_to_commit(root: &Path, commit: &str) -> Result<(), OxyError> {
        // Basic guard: reject refs that look like shell injection attempts.
        if commit.contains([';', '|', '&', '`', '$', '(', ')']) {
            return Err(OxyError::ArgumentError(format!(
                "Invalid commit ref: {commit}"
            )));
        }
        // Abort any in-progress operation so state files are removed.
        let git_dir = Self::resolve_git_dir(root);
        if git_dir.join("MERGE_HEAD").exists() {
            let _ = Self::run_git(root, &["merge", "--abort"]).await;
        } else if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
            let _ = Self::run_git(root, &["rebase", "--abort"]).await;
        }

        // Fetch the one-line summary of the target commit for the message.
        let short = if commit.len() > 7 {
            &commit[..7]
        } else {
            commit
        };
        let log = Self::run_git(root, &["log", "--format=%s", "-n", "1", commit])
            .await
            .unwrap_or_default();
        let summary = log.trim();

        // Restore all tracked paths to the state they had at `commit`.
        // Files absent in `commit` but present in HEAD are staged as deletions;
        // files present in `commit` but absent in HEAD are staged as additions.
        Self::run_git(root, &["checkout", commit, "--", "."]).await?;

        // Commit the restore. If there is nothing to commit the repo is already
        // at that state, which is not an error.
        let msg = if summary.is_empty() {
            format!("Restore to {short}")
        } else {
            format!("Restore to {short}: {summary}")
        };
        match Self::run_git(root, &["commit", "-m", &msg]).await {
            Ok(_) => {}
            Err(e) if e.to_string().contains("nothing to commit") => {}
            Err(e) => return Err(e),
        }

        Ok(())
    }

    /// Aborts an in-progress rebase or merge, restoring HEAD to its previous state.
    pub async fn abort_rebase(root: &Path) -> Result<(), OxyError> {
        let git_dir = Self::resolve_git_dir(root);
        if git_dir.join("MERGE_HEAD").exists() {
            Self::run_git(root, &["merge", "--abort"]).await?;
        } else {
            Self::run_git(root, &["rebase", "--abort"]).await?;
        }
        Ok(())
    }

    /// Writes resolved content to a file and stages it.
    ///
    /// Called after the user has manually edited the conflict markers out of
    /// the file in the merge editor.  Writes `content` directly to
    /// `root/<file_path>` then runs `git add` to mark it resolved.
    pub async fn write_and_stage_file(
        root: &Path,
        file_path: &str,
        content: &str,
    ) -> Result<(), OxyError> {
        let full_path = root.join(file_path);
        // Guard against path traversal (e.g. "../../etc/shadow")
        let canonical = full_path
            .canonicalize()
            .or_else(|_| {
                // File may not exist yet; canonicalize parent instead
                full_path
                    .parent()
                    .ok_or(std::io::Error::other("no parent"))
                    .and_then(|p| p.canonicalize())
                    .map(|p| p.join(full_path.file_name().unwrap_or_default()))
            })
            .map_err(|e| OxyError::ArgumentError(format!("Invalid file path: {e}")))?;
        if !canonical.starts_with(root) {
            return Err(OxyError::ArgumentError(format!(
                "File path escapes project root: {file_path}"
            )));
        }
        let full_path = canonical;
        tokio::fs::write(&full_path, content)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to write file: {e}")))?;
        Self::run_git(root, &["add", "--", file_path]).await?;
        Ok(())
    }

    /// Resolves a conflicted file by accepting one side, then stages it.
    ///
    /// During `git pull --rebase` the roles of --ours / --theirs are inverted:
    /// - `--theirs` = the local commit being replayed = "Use Mine"
    /// - `--ours`   = the upstream base commit       = "Use Theirs"
    pub async fn resolve_conflict_file(
        root: &Path,
        file_path: &str,
        use_mine: bool,
    ) -> Result<(), OxyError> {
        let side = if use_mine { "--theirs" } else { "--ours" };
        Self::run_git(root, &["checkout", side, "--", file_path]).await?;
        Self::run_git(root, &["add", "--", file_path]).await?;
        Ok(())
    }

    /// Restores conflict markers for a previously-resolved file.
    ///
    /// `git restore --conflict=merge` only works while the original conflict
    /// stages (1/2/3) still exist in the index.  Once `git add` has been run
    /// (which collapses them to stage 0) that command fails.  Instead we
    /// reconstruct the three-way merge using `git show` + `git merge-file -p`.
    pub async fn unresolve_conflict_file(root: &Path, file_path: &str) -> Result<(), OxyError> {
        // Guard against path traversal
        let full_path = root.join(file_path);
        if let Ok(canonical) = full_path.canonicalize()
            && !canonical.starts_with(root)
        {
            return Err(OxyError::ArgumentError(format!(
                "File path escapes project root: {file_path}"
            )));
        }

        let git_dir = Self::resolve_git_dir(root);

        let is_rebase = git_dir.join("REBASE_HEAD").exists()
            || git_dir.join("rebase-merge").exists()
            || git_dir.join("rebase-apply").exists();
        let is_merge = git_dir.join("MERGE_HEAD").exists();

        if !is_rebase && !is_merge {
            return Err(OxyError::RuntimeError(
                "Not in an active merge or rebase — cannot restore conflict markers".into(),
            ));
        }

        // During `git pull --rebase`: REBASE_HEAD = local commit, HEAD = upstream.
        // During `git merge`:         HEAD = current branch, MERGE_HEAD = incoming.
        let (ours_ref, theirs_ref) = if is_rebase {
            ("REBASE_HEAD", "HEAD")
        } else {
            ("HEAD", "MERGE_HEAD")
        };

        let ours_content = Self::run_git(root, &["show", &format!("{ours_ref}:{file_path}")])
            .await
            .unwrap_or_default();
        let theirs_content = Self::run_git(root, &["show", &format!("{theirs_ref}:{file_path}")])
            .await
            .unwrap_or_default();

        let base_hash = Self::run_git(root, &["merge-base", ours_ref, theirs_ref])
            .await
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let base_content = if !base_hash.is_empty() {
            Self::run_git(root, &["show", &format!("{base_hash}:{file_path}")])
                .await
                .unwrap_or_default()
        } else {
            String::new()
        };

        // Write temp files for git merge-file — use UUID for collision-free names
        let tmp_dir = std::env::temp_dir();
        let id = uuid::Uuid::new_v4().simple().to_string();
        let tmp_ours = tmp_dir.join(format!("oxy_ours_{id}"));
        let tmp_base = tmp_dir.join(format!("oxy_base_{id}"));
        let tmp_theirs = tmp_dir.join(format!("oxy_theirs_{id}"));

        tokio::fs::write(&tmp_ours, ours_content.as_bytes())
            .await
            .map_err(|e| OxyError::RuntimeError(format!("write temp file: {e}")))?;
        tokio::fs::write(&tmp_base, base_content.as_bytes())
            .await
            .map_err(|e| OxyError::RuntimeError(format!("write temp file: {e}")))?;
        tokio::fs::write(&tmp_theirs, theirs_content.as_bytes())
            .await
            .map_err(|e| OxyError::RuntimeError(format!("write temp file: {e}")))?;

        // `git merge-file -p` writes the merged (possibly conflicted) output to
        // stdout.  Exit code: 0 = clean merge, 1 = conflict markers written (the
        // expected case here), >1 = hard error.
        let base_label: &str = if base_hash.is_empty() {
            "base"
        } else {
            &base_hash
        };
        let output = Command::new("git")
            .args([
                "merge-file",
                "-p",
                "-L",
                ours_ref,
                "-L",
                base_label,
                "-L",
                theirs_ref,
                tmp_ours.to_str().unwrap_or(""),
                tmp_base.to_str().unwrap_or(""),
                tmp_theirs.to_str().unwrap_or(""),
            ])
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("spawn git merge-file: {e}")))?;

        // Always clean up temp files
        let _ = tokio::fs::remove_file(&tmp_ours).await;
        let _ = tokio::fs::remove_file(&tmp_base).await;
        let _ = tokio::fs::remove_file(&tmp_theirs).await;

        let exit_code = output.status.code().unwrap_or(-1);
        if exit_code > 1 {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(OxyError::RuntimeError(format!(
                "git merge-file failed: {}",
                stderr.trim()
            )));
        }

        // Write the conflict-marked content back to the working tree
        let full_path = root.join(file_path);
        tokio::fs::write(&full_path, &output.stdout)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("write conflict file: {e}")))?;

        // Unstage so the file shows as "U" (unmerged) again — not fatal
        let _ = Self::run_git(root, &["restore", "--staged", "--", file_path]).await;

        Ok(())
    }

    /// Stages all changes and continues an in-progress rebase or merge.
    ///
    /// Call this after the user has manually resolved all conflict markers.
    /// Sets GIT_EDITOR=true so git never opens an interactive editor for
    /// commit messages (`:` is a shell builtin and cannot be exec'd directly).
    pub async fn continue_rebase(root: &Path) -> Result<(), OxyError> {
        let git_dir = Self::resolve_git_dir(root);
        Self::run_git(root, &["add", "-A"]).await?;
        let subcmd = if git_dir.join("MERGE_HEAD").exists() {
            "merge"
        } else {
            "rebase"
        };
        Self::run_git_no_editor(root, &[subcmd, "--continue"]).await?;
        Ok(())
    }

    /// Like [`run_git`] but sets `GIT_EDITOR=true` and `GIT_TERMINAL_PROMPT=0`
    /// so git never opens an interactive editor or credential prompt.
    async fn run_git_no_editor(cwd: &Path, args: &[&str]) -> Result<String, OxyError> {
        let output = Command::new("git")
            .current_dir(cwd)
            .env("GIT_EDITOR", "true")
            .env("GIT_TERMINAL_PROMPT", "0")
            .args(args)
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to spawn git: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(OxyError::RuntimeError(format!(
                "git {} failed: {}",
                args.first().unwrap_or(&""),
                stderr.trim()
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Returns the default branch name for `workspace_root`.
    ///
    /// Resolution order:
    /// 1. `GIT_DEFAULT_BRANCH` environment variable (if set and non-empty)
    /// 2. `git symbolic-ref --short refs/remotes/origin/HEAD` (strips "origin/" prefix)
    /// 3. Falls back to `"main"`
    ///
    /// The result is cached for the process lifetime; git remote HEAD almost never
    /// changes at runtime.
    pub async fn get_default_branch(workspace_root: &Path) -> String {
        DEFAULT_BRANCH
            .get_or_init(|| async {
                if let Ok(b) = std::env::var("GIT_DEFAULT_BRANCH")
                    && !b.is_empty()
                {
                    return b;
                }
                match Self::run_git(
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
            })
            .await
            .clone()
    }

    /// Removes the worktree for `branch` (if any) and deletes the local branch ref.
    ///
    /// This is safe to call on branches that have no worktree — it will only
    /// delete the local branch ref in that case.  Refuses to delete the
    /// default branch.
    pub async fn delete_branch(workspace_root: &Path, branch: &str) -> Result<(), OxyError> {
        // Remove the worktree first — git won't delete a branch that is checked
        // out in a worktree unless we remove the worktree first.
        if let Some(wt_path) = Self::get_worktree_path(workspace_root, branch) {
            Self::run_git(
                workspace_root,
                &["worktree", "remove", "--force", &wt_path.to_string_lossy()],
            )
            .await?;
        }
        // Delete the local branch ref (force — the caller is responsible for
        // ensuring the branch is safe to delete).
        Self::run_git(workspace_root, &["branch", "-D", branch]).await?;
        Ok(())
    }

    /// Returns `true` if `workspace_root` has at least one configured git remote.
    pub async fn has_remote(workspace_root: &Path) -> bool {
        Self::run_git(workspace_root, &["remote"])
            .await
            .map(|out| !out.trim().is_empty())
            .unwrap_or(false)
    }

    /// Fetches from origin (if a remote exists) and returns all branch names
    /// the user can check out — both local branches and remote-only branches
    /// (stripped of the `origin/` prefix, deduplicated).
    pub async fn list_all_branches(
        workspace_root: &Path,
        token: Option<&str>,
    ) -> Result<Vec<String>, OxyError> {
        // Best-effort fetch so remote branches are up to date.
        if Self::has_remote(workspace_root).await {
            let _ =
                Self::run_git_authed(workspace_root, &["fetch", "--prune", "origin"], token).await;
        }

        // Local branches
        let local_out = Self::run_git(workspace_root, &["branch", "--format=%(refname:short)"])
            .await
            .unwrap_or_default();
        let mut branches: Vec<String> = local_out
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();

        // Remote branches (strip "origin/" prefix, skip HEAD)
        let remote_out = Self::run_git(
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

    /// Checks out `branch` in `workspace_root`.  If the branch only exists on
    /// the remote, creates a local tracking branch first.
    pub async fn checkout_branch(
        workspace_root: &Path,
        branch: &str,
        token: Option<&str>,
    ) -> Result<(), OxyError> {
        // Best-effort fetch so remote refs are available.
        if Self::has_remote(workspace_root).await {
            let _ =
                Self::run_git_authed(workspace_root, &["fetch", "--prune", "origin"], token).await;
        }

        let local_exists = Self::branch_exists(workspace_root, branch).await?;
        if local_exists {
            Self::run_git(workspace_root, &["checkout", branch]).await?;
        } else {
            // Check whether the remote has this branch so we can set up tracking.
            let remote_ref = format!("origin/{}", branch);
            let remote_exists =
                Self::run_git(workspace_root, &["rev-parse", "--verify", &remote_ref])
                    .await
                    .is_ok();

            if remote_exists {
                // Create a local tracking branch from the remote.
                Self::run_git(workspace_root, &["checkout", "-b", branch, &remote_ref]).await?;
            } else {
                // New branch with no remote counterpart — create from HEAD.
                Self::run_git(workspace_root, &["checkout", "-b", branch]).await?;
            }
        }
        Ok(())
    }

    /// Clone `repo_url` into `workspace_root` if no `.git` exists yet; otherwise
    /// call [`ensure_initialized`] (git init for a fresh directory, no-op for
    /// an existing repo).
    ///
    /// This replaces the git-sync sidecar for initial repository setup.
    pub async fn clone_or_init(
        workspace_root: &Path,
        repo_url: Option<&str>,
        branch: &str,
        token: Option<&str>,
    ) -> Result<(), OxyError> {
        if Self::is_git_repo(workspace_root) {
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
            // clone_repository expects the destination to not exist yet, but the
            // project root directory may already exist.  We clone into a temp
            // sibling and then move the contents.
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

            GitOperations::clone_repository(url, &tmp_dest, Some(branch), token).await?;

            // Move files from tmp_dest into workspace_root.
            // Falls back to copy+delete when rename fails (e.g. cross-device in Docker).
            let mut read_dir = tokio::fs::read_dir(&tmp_dest)
                .await
                .map_err(|e| OxyError::IOError(format!("Failed to read cloned directory: {e}")))?;
            while let Some(entry) = read_dir.next_entry().await.map_err(|e| {
                OxyError::IOError(format!("Failed to iterate cloned directory: {e}"))
            })? {
                let src = entry.path();
                let dst = workspace_root.join(entry.file_name());
                if let Err(_rename_err) = tokio::fs::rename(&src, &dst).await {
                    // rename can fail cross-device (e.g. Docker volume mounts);
                    // fall back to recursive copy + delete.
                    Self::copy_recursive(&src, &dst).await.map_err(|ce| {
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
            Self::ensure_initialized(workspace_root).await?;
        }

        Ok(())
    }

    /// Push the current branch in `root` to its upstream remote.
    ///
    /// Passes `push.autoSetupRemote=true` as a transient `-c` flag so the first
    /// push of a new branch automatically creates the upstream tracking ref without
    /// permanently mutating `~/.gitconfig`.
    pub async fn push_to_remote(root: &Path, token: Option<&str>) -> Result<(), OxyError> {
        let branch = Self::get_current_branch(root).await?;
        info!(
            "Pushing branch '{}' in {} to remote",
            branch,
            root.display()
        );

        Self::run_git_authed(
            root,
            &["-c", "push.autoSetupRemote=true", "push", "origin", &branch],
            token,
        )
        .await?;
        info!("Push successful");
        Ok(())
    }

    /// Force-pushes the current branch to remote using `--force-with-lease`.
    ///
    /// `--force-with-lease` is safer than `--force`: it only succeeds if the
    /// remote ref matches our locally-cached tracking ref, preventing accidental
    /// overwrite of commits pushed by others since our last fetch.
    pub async fn force_push_to_remote(root: &Path, token: Option<&str>) -> Result<(), OxyError> {
        let branch = Self::get_current_branch(root).await?;
        info!(
            "Force-pushing branch '{}' in {} to remote",
            branch,
            root.display()
        );
        Self::run_git_authed(
            root,
            &[
                "-c",
                "push.autoSetupRemote=true",
                "push",
                "--force-with-lease",
                "origin",
                &branch,
            ],
            token,
        )
        .await?;
        info!("Force push successful");
        Ok(())
    }

    /// Pull the latest changes for `branch` from within `worktree_root`.
    ///
    /// Runs `git pull --rebase origin <branch>` entirely inside the worktree.
    /// Git routes rebase state through the worktree's own gitdir
    /// (`.git/worktrees/<name>/rebase-merge/`), so conflicts never leak into
    /// the main repo's `.git/rebase-merge/` and don't block other worktrees.
    pub async fn pull_from_remote(
        worktree_root: &Path,
        branch: &str,
        token: Option<&str>,
    ) -> Result<(), OxyError> {
        info!("Pulling {} in {}", branch, worktree_root.display());
        Self::run_git_authed(
            worktree_root,
            &["pull", "--rebase", "origin", branch],
            token,
        )
        .await?;
        info!("Pull successful");
        Ok(())
    }

    // ─── Private helpers ───────────────────────────────────────────────────

    /// Returns `true` if `branch` exists as a local branch in `workspace_root`.
    async fn branch_exists(workspace_root: &Path, branch: &str) -> Result<bool, OxyError> {
        let out = Self::run_git(
            workspace_root,
            &["branch", "--list", branch, "--format=%(refname:short)"],
        )
        .await?;
        Ok(!out.trim().is_empty())
    }

    /// Runs a git command in `cwd`, returning stdout on success or an
    /// [`OxyError::RuntimeError`] containing stderr on failure.
    async fn run_git(cwd: &Path, args: &[&str]) -> Result<String, OxyError> {
        Self::run_git_authed(cwd, args, None).await
    }

    /// Runs a git command in `cwd` with optional token authentication.
    ///
    /// When `token` is provided it is passed via `-c http.extraHeader=Authorization: Bearer {token}`
    /// which is transient and **never written to `.git/config`**.
    async fn run_git_authed(
        cwd: &Path,
        args: &[&str],
        token: Option<&str>,
    ) -> Result<String, OxyError> {
        let mut cmd = Command::new("git");
        cmd.current_dir(cwd);

        if let Some(t) = token {
            cmd.args(["-c", &format!("http.extraHeader=Authorization: Bearer {t}")]);
        }

        cmd.args(args);

        let output = cmd
            .output()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to spawn git: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(OxyError::RuntimeError(format!(
                "git {} failed: {}",
                args.first().unwrap_or(&""),
                stderr.trim()
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Recursively copies `src` to `dst` using `tokio::fs`.
    ///
    /// Used as a fallback when `rename` fails with EXDEV (cross-device).
    async fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
        if src.is_dir() {
            tokio::fs::create_dir_all(dst).await?;
            let mut entries = tokio::fs::read_dir(src).await?;
            while let Some(entry) = entries.next_entry().await? {
                let child_dst = dst.join(entry.file_name());
                Box::pin(Self::copy_recursive(&entry.path(), &child_dst)).await?;
            }
        } else {
            tokio::fs::copy(src, dst).await?;
        }
        Ok(())
    }
}
