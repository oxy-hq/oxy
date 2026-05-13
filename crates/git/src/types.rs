use secrecy::SecretString;
use serde::{Deserialize, Serialize};

/// Authentication for remote git operations.
///
/// `Bearer` is injected as an HTTP header via `-c http.extraHeader`; the
/// token is never persisted to `.git/config` or embedded in the remote URL.
pub enum Auth {
    None,
    Bearer(SecretString),
}

impl Auth {
    pub fn bearer(token: impl Into<String>) -> Self {
        Self::Bearer(SecretString::from(token.into()))
    }
}

/// Per-file status entry returned by [`crate::GitClient::diff_numstat_summary`].
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileStatus {
    pub path: String,
    pub status: String,
    pub insert: u32,
    pub delete: u32,
}

/// Working-tree change classification used by the dirty-tree guard before a
/// destructive operation (e.g. restore-to-commit).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DirtyKind {
    /// Tracked file with unstaged changes (porcelain Y != ' ').
    Modified,
    /// Tracked file with staged changes (porcelain X != ' ').
    Staged,
    /// Untracked file (porcelain `??`).
    Untracked,
    /// Tracked file deleted in the working tree.
    Deleted,
    /// Conflicted file (porcelain U?, ?U, AA, DD).
    Conflicted,
}

/// A single working-tree entry that would be discarded by a hard reset.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DirtyEntry {
    pub path: String,
    pub kind: DirtyKind,
}

/// Outcome of `reset_to_commit`. When `force=false` and the working tree has
/// changes, the caller gets back the file list so the UI can prompt for
/// confirmation; otherwise the restore commit was created.
#[derive(Debug)]
pub enum ResetOutcome {
    /// Restore commit was created (or there was nothing to commit).
    Done,
    /// Working tree is dirty and `force` was not set; no changes were made.
    Dirty(Vec<DirtyEntry>),
}

/// Where a branch lives. Returned by
/// [`crate::GitClient::list_branches_with_origin`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchOrigin {
    /// Branch exists locally but not as `origin/<name>` (never pushed).
    LocalOnly,
    /// Branch exists as `origin/<name>` but has no local checkout yet.
    RemoteOnly,
    /// Branch exists locally **and** on origin.
    Both,
}

/// A branch as known to git, with its origin and (when applicable) its sync
/// status relative to the upstream tracking ref.
#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub name: String,
    pub origin: BranchOrigin,
    /// `Some("behind"|"synced")` when `origin == Both`; `None` otherwise
    /// (no tracking ref to compare against).
    pub sync_status: Option<String>,
}

/// Outcome of [`crate::GitClient::ensure_local_ref`]. By construction the
/// function materialises a local ref before returning, so `RemoteOnly` is
/// impossible — that variant is excluded at the type level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalRefOrigin {
    /// Branch exists locally but not as `origin/<name>`.
    LocalOnly,
    /// Branch exists locally **and** on origin.
    Both,
}

impl From<LocalRefOrigin> for BranchOrigin {
    fn from(origin: LocalRefOrigin) -> Self {
        match origin {
            LocalRefOrigin::LocalOnly => BranchOrigin::LocalOnly,
            LocalRefOrigin::Both => BranchOrigin::Both,
        }
    }
}
