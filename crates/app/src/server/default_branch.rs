//! Per-workspace default-branch resolver.
//!
//! The compile boundary serves Postgres-backed reads only when the
//! request is for the workspace's *default* branch (typically `main`,
//! sometimes `master` or a custom name). Non-default branches read
//! from FS — the IDE working copy is the freshest source by definition.
//!
//! The default branch is discovered from the local checkout via
//! `GitClient::get_default_branch` (which reads `refs/remotes/origin/HEAD`
//! under the hood). That's a sub-millisecond operation on a healthy clone
//! but still a syscall; we cache the result per-workspace with a small
//! TTL so the hybrid reader can call it on every request without cost.
//!
//! When the workspace is missing, its path is unset, or the lookup
//! errors, `resolve_default_branch` returns `None`. The hybrid reader
//! treats that as "I can't classify the branch — fall through to FS"
//! (`is_default_branch` returns `false` on `None`). Reading from FS
//! is the safer answer here: the branch-aware contract exists so
//! feature-branch users see their working-copy edits, and silently
//! serving the promoted main revision to a request that can't be
//! classified would violate that contract. The cache + GitClient
//! both fail to FS, never to stale Postgres.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use oxy_git::GitClient;
use sea_orm::{DatabaseConnection, EntityTrait};
use uuid::Uuid;

/// How long a per-workspace default-branch entry is reused before we
/// re-discover it from the git client. Short enough to pick up a
/// `git symbolic-ref` change inside a few seconds (rare op), long
/// enough that the typical request burst is one syscall amortised.
const CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Clone)]
struct CachedBranch {
    name: String,
    fetched_at: Instant,
}

static CACHE: OnceLock<RwLock<HashMap<Uuid, CachedBranch>>> = OnceLock::new();

fn cache() -> &'static RwLock<HashMap<Uuid, CachedBranch>> {
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Returns the workspace's default branch name (e.g. `"main"`). `None`
/// when the workspace doesn't exist, has no path, or the git lookup
/// fails — caller should treat that as "use the safer fallback path."
pub async fn resolve_default_branch(db: &DatabaseConnection, workspace_id: Uuid) -> Option<String> {
    if let Some(hit) = read_cache(workspace_id) {
        return Some(hit);
    }

    let workspace_row = entity::workspaces::Entity::find_by_id(workspace_id)
        .one(db)
        .await
        .ok()
        .flatten()?;
    let path = workspace_row.path.as_deref()?;
    let workspace_path = std::path::Path::new(path);

    let client = oxy::github::default_git_client();
    let branch = client.get_default_branch(workspace_path).await;
    if branch.is_empty() {
        return None;
    }
    write_cache(workspace_id, &branch);
    Some(branch)
}

fn read_cache(workspace_id: Uuid) -> Option<String> {
    let guard = cache().read().ok()?;
    let entry = guard.get(&workspace_id)?;
    if entry.fetched_at.elapsed() > CACHE_TTL {
        return None;
    }
    Some(entry.name.clone())
}

fn write_cache(workspace_id: Uuid, branch: &str) {
    if let Ok(mut guard) = cache().write() {
        guard.insert(
            workspace_id,
            CachedBranch {
                name: branch.to_string(),
                fetched_at: Instant::now(),
            },
        );
    }
}
