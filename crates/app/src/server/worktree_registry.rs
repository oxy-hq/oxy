//! Worktree lifecycle registry — the first concrete step of the plan-2
//! "ephemeral workspace environment" broker (design:
//! `internal-docs/ephemeral-workspace-environments.md`,
//! Stage 3a), brought forward to today's single-`ide` topology.
//!
//! ## Why
//! The `ide` singleton creates a git worktree per branch under
//! `<state_dir>/workspaces/<id>/.worktrees/<branch>` and today **never reaps
//! them**: `git worktree remove` runs only on explicit branch deletion. Every
//! branch ever opened leaves a full-repo working copy on the pod's disk
//! forever — unbounded growth on a single instance, with no registry, no
//! tracking, no idle teardown (the directory *is* the registry).
//!
//! ## What
//! This registry tracks per-worktree access and reaps worktrees that are both
//! **idle** (no request resolved to them within the idle window) and **clean**
//! (`git status --porcelain` empty). Clean is the safety invariant: a clean
//! worktree has every change committed, and commits live in the repo's shared
//! object store, so `git worktree remove` discards only the working copy. The
//! branch ref and its history survive; the next request re-materialises the
//! worktree via `get_or_create_worktree`. Worktrees with uncommitted work are
//! never reaped.
//!
//! ## Where it's going
//! When the workspace environment becomes its own deployable (plan-2 Stage 3),
//! the same get-or-create + touch + idle-reap contract drives a per-workspace
//! pod instead of a local worktree — only the backend changes. This module is
//! that broker's lifecycle, proven against the cheap in-place backend first so
//! the topology change lands on a tested contract ("fewest bugs while we
//! migrate").

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime};

use dashmap::DashMap;
use tokio_util::sync::CancellationToken;

use oxy_git::cli::worktree::{WORKTREES_DIR, is_worktree_clean, list_worktrees, remove_worktree};

use axum::Json;
use axum::extract::Path as AxumPath;
use uuid::Uuid;

/// Idle window before a clean worktree is eligible for reaping
/// (override: `OXY_WORKTREE_IDLE_TIMEOUT_SECS`). Conservative by default — a
/// reaped worktree only costs a sub-second re-materialise on next access.
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(6 * 60 * 60);
/// Interval between reap sweeps (override: `OXY_WORKTREE_SWEEP_INTERVAL_SECS`).
const DEFAULT_SWEEP_INTERVAL: Duration = Duration::from_secs(30 * 60);

/// Process-wide registry. Worktrees are a single-process (`ide`) concern, so a
/// process global — like `role_manifest`'s `PROCESS_ROLE` — is the right scope:
/// the request middleware touches it, the reaper loop reads it.
static REGISTRY: OnceLock<Arc<WorktreeRegistry>> = OnceLock::new();

/// Returns the process-wide registry, initialising it on first use.
pub fn registry() -> Arc<WorktreeRegistry> {
    REGISTRY
        .get_or_init(|| Arc::new(WorktreeRegistry::new()))
        .clone()
}

/// Tracks last-access per worktree path and decides what is safe to reap.
pub struct WorktreeRegistry {
    /// worktree path → last time a request resolved to it.
    access: DashMap<PathBuf, SystemTime>,
    /// Process start. Floors every last-access so nothing is reaped within one
    /// idle window of startup, when we have no access history yet — a restart
    /// must not make every worktree look "idle since the epoch".
    started_at: SystemTime,
}

/// Outcome of one sweep — logged for operability, asserted in tests.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReapStats {
    pub scanned: usize,
    pub reaped: usize,
    pub skipped_recent: usize,
    pub skipped_dirty: usize,
    pub errors: usize,
}

/// One worktree's live lifecycle state, for the `/worktrees` diagnostic.
#[derive(Debug, serde::Serialize)]
pub struct WorktreeStatus {
    pub branch: Option<String>,
    pub path: String,
    pub idle_secs: u64,
    pub clean: bool,
    /// True if the next reaper sweep would reclaim this worktree (idle + clean).
    pub would_reap: bool,
}

impl WorktreeRegistry {
    fn new() -> Self {
        Self {
            access: DashMap::new(),
            started_at: SystemTime::now(),
        }
    }

    /// Records that `path` was used now. Called from the workspace middleware
    /// whenever a request resolves to a worktree path.
    pub fn touch(&self, path: &Path) {
        self.access.insert(path.to_path_buf(), SystemTime::now());
    }

    /// Effective last-access: the max of tracked access, the worktree dir's
    /// mtime (catches a just-created worktree not yet touched), and
    /// `started_at` (the post-restart grace floor).
    fn last_access(&self, path: &Path) -> SystemTime {
        let tracked = self.access.get(path).map(|t| *t);
        let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        [tracked, mtime, Some(self.started_at)]
            .into_iter()
            .flatten()
            .max()
            .unwrap_or(self.started_at)
    }

    /// True if `path` has not been accessed within `idle`.
    fn is_idle(&self, path: &Path, now: SystemTime, idle: Duration) -> bool {
        now.duration_since(self.last_access(path))
            .unwrap_or(Duration::ZERO)
            >= idle
    }

    /// Live state of every non-main worktree under `workspace_root`, for the
    /// diagnostic endpoint. Mirrors the reaper's decision so `would_reap` is
    /// exactly what the next sweep will act on.
    pub async fn status_for_workspace(
        &self,
        workspace_root: &Path,
        idle: Duration,
    ) -> Vec<WorktreeStatus> {
        let now = SystemTime::now();
        let Ok(entries) = list_worktrees(workspace_root).await else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for wt in entries.iter().filter(|w| !w.is_main) {
            let idle_secs = now
                .duration_since(self.last_access(&wt.path))
                .unwrap_or(Duration::ZERO)
                .as_secs();
            let is_idle = self.is_idle(&wt.path, now, idle);
            let clean = is_worktree_clean(&wt.path).await.unwrap_or(false);
            out.push(WorktreeStatus {
                branch: wt.branch.clone(),
                path: wt.path.display().to_string(),
                idle_secs,
                clean,
                would_reap: is_idle && clean,
            });
        }
        out
    }

    /// Sweeps every workspace under `workspaces_root`, reaping non-main
    /// worktrees that are both idle (> `idle`) and clean.
    pub async fn reap_idle(&self, workspaces_root: &Path, idle: Duration) -> ReapStats {
        let mut stats = ReapStats::default();
        let now = SystemTime::now();
        let Ok(mut dir) = tokio::fs::read_dir(workspaces_root).await else {
            return stats;
        };
        while let Ok(Some(ws)) = dir.next_entry().await {
            let root = ws.path();
            // Cheap pre-filter: only a workspace with a `.worktrees/` dir can
            // have anything to reap. (Subdir-workspace repos keep `.worktrees`
            // at the real repo root, above this path — they're skipped, which
            // is safe: we never reap them, only miss them. Noted as a known
            // limitation the pod-backed successor removes.)
            if root.join(WORKTREES_DIR).is_dir() {
                self.reap_workspace(&root, now, idle, &mut stats).await;
            }
        }
        stats
    }

    async fn reap_workspace(
        &self,
        root: &Path,
        now: SystemTime,
        idle: Duration,
        stats: &mut ReapStats,
    ) {
        let entries = match list_worktrees(root).await {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(error = %e, root = %root.display(), "worktree reaper: list failed");
                stats.errors += 1;
                return;
            }
        };
        for wt in entries.iter().filter(|w| !w.is_main) {
            stats.scanned += 1;
            if !self.is_idle(&wt.path, now, idle) {
                stats.skipped_recent += 1;
                continue;
            }
            match is_worktree_clean(&wt.path).await {
                Ok(true) => self.reap_one(root, &wt.path, stats).await,
                Ok(false) => stats.skipped_dirty += 1,
                Err(e) => {
                    tracing::warn!(error = %e, path = %wt.path.display(), "worktree reaper: status failed");
                    stats.errors += 1;
                }
            }
        }
    }

    async fn reap_one(&self, root: &Path, path: &Path, stats: &mut ReapStats) {
        match remove_worktree(root, path).await {
            Ok(()) => {
                self.access.remove(path);
                stats.reaped += 1;
            }
            Err(e) => {
                // e.g. went dirty since the clean-check (no `--force`), or a git
                // op holds it — leave it; the next sweep retries.
                tracing::warn!(error = %e, path = %path.display(), "worktree reaper: remove failed");
                stats.errors += 1;
            }
        }
    }
}

/// Spawns the periodic reaper. Call **only** on the `ide` role — worktrees live
/// on the ide singleton's local disk; no other role has them. Honors `shutdown`
/// for a clean SIGTERM exit, matching the recovery / camera loops.
pub fn spawn_worktree_reaper(registry: Arc<WorktreeRegistry>, shutdown: CancellationToken) {
    let workspaces_root = oxy::state_dir::get_state_dir().join("workspaces");
    let idle = env_duration_secs("OXY_WORKTREE_IDLE_TIMEOUT_SECS").unwrap_or(DEFAULT_IDLE_TIMEOUT);
    let interval =
        env_duration_secs("OXY_WORKTREE_SWEEP_INTERVAL_SECS").unwrap_or(DEFAULT_SWEEP_INTERVAL);
    tracing::info!(
        idle_secs = idle.as_secs(),
        interval_secs = interval.as_secs(),
        workspaces_root = %workspaces_root.display(),
        "worktree reaper: started (ide role)"
    );
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(interval);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        tick.tick().await; // drain the immediate first tick — part of the startup grace window.
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    tracing::info!("worktree reaper: shutdown");
                    break;
                }
                _ = tick.tick() => {
                    let stats = registry.reap_idle(&workspaces_root, idle).await;
                    if stats.reaped > 0 || stats.errors > 0 {
                        tracing::info!(
                            scanned = stats.scanned,
                            reaped = stats.reaped,
                            skipped_recent = stats.skipped_recent,
                            skipped_dirty = stats.skipped_dirty,
                            errors = stats.errors,
                            "worktree reaper: swept",
                        );
                    }
                }
            }
        }
    });
}

fn env_duration_secs(key: &str) -> Option<Duration> {
    std::env::var(key)
        .ok()?
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

/// The idle window the reaper is using (env override or default). Exposed so
/// the diagnostic reports `would_reap` against the SAME threshold the sweep uses.
pub fn configured_idle_timeout() -> Duration {
    env_duration_secs("OXY_WORKTREE_IDLE_TIMEOUT_SECS").unwrap_or(DEFAULT_IDLE_TIMEOUT)
}

/// `GET /api/{workspace_id}/worktrees` — the workspace's live worktree
/// lifecycle (branch, idle seconds, clean, would-reap). IdeOnly: the registry
/// is ide-local, so the serve fleet forwards here; workspace access is already
/// gated by the workspace middleware.
pub async fn get_worktree_status(
    AxumPath(workspace_id): AxumPath<Uuid>,
) -> Json<Vec<WorktreeStatus>> {
    let root = oxy::state_dir::get_state_dir()
        .join("workspaces")
        .join(workspace_id.to_string());
    Json(
        registry()
            .status_for_workspace(&root, configured_idle_timeout())
            .await,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_started_at(t: SystemTime) -> WorktreeRegistry {
        WorktreeRegistry {
            access: DashMap::new(),
            started_at: t,
        }
    }

    /// The idle decision honors both the tracked access time and the
    /// post-restart `started_at` grace floor. Uses a non-existent path so
    /// `std::fs::metadata` fails and the decision is purely time-driven.
    #[test]
    fn idle_decision_respects_access_and_started_floor() {
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let h = |secs| t0 + Duration::from_secs(secs);
        let idle = Duration::from_secs(6 * 3600);
        let reg = registry_started_at(t0);
        let p = PathBuf::from("/nonexistent/.worktrees/x");

        // Never touched → last_access floored at started_at. Within the window
        // it's not idle (restart grace); past the window it is.
        assert!(
            !reg.is_idle(&p, h(3600), idle),
            "1h after start: grace, not idle"
        );
        assert!(
            reg.is_idle(&p, h(7 * 3600), idle),
            "7h after start, untouched: idle"
        );

        // Touched at +5h → measured from the touch, not from start.
        reg.access.insert(p.clone(), h(5 * 3600));
        assert!(
            !reg.is_idle(&p, h(6 * 3600), idle),
            "1h after touch: not idle"
        );
        assert!(reg.is_idle(&p, h(12 * 3600), idle), "7h after touch: idle");
    }

    /// End-to-end against a real repo: a clean idle worktree is reaped while a
    /// dirty one is kept, and the reaped worktree's branch ref survives (the
    /// non-destructive invariant). `idle = ZERO` makes everything idle so the
    /// test is timing-independent.
    #[tokio::test]
    async fn reaps_clean_idle_worktree_keeps_dirty_and_preserves_branch() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspaces_root = tmp.path().join("workspaces");
        let repo = workspaces_root.join("ws1");
        std::fs::create_dir_all(&repo).expect("mkdir repo");

        git(&repo, &["init", "-q"]).await;
        git(&repo, &["config", "user.email", "t@example.com"]).await;
        git(&repo, &["config", "user.name", "Test"]).await;
        std::fs::write(repo.join("f.txt"), "hi").expect("seed file");
        git(&repo, &["add", "."]).await;
        git(&repo, &["commit", "-qm", "init"]).await;

        let clean_wt = repo.join(WORKTREES_DIR).join("clean");
        let dirty_wt = repo.join(WORKTREES_DIR).join("dirty");
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                clean_wt.to_str().unwrap(),
                "-b",
                "clean",
            ],
        )
        .await;
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                dirty_wt.to_str().unwrap(),
                "-b",
                "dirty",
            ],
        )
        .await;
        // Make the dirty worktree dirty (untracked file → non-empty status).
        std::fs::write(dirty_wt.join("scratch.txt"), "wip").expect("dirty file");

        let reg = registry_started_at(SystemTime::UNIX_EPOCH);
        let stats = reg.reap_idle(&workspaces_root, Duration::ZERO).await;

        assert_eq!(
            stats.reaped, 1,
            "exactly the clean worktree is reaped: {stats:?}"
        );
        assert_eq!(
            stats.skipped_dirty, 1,
            "the dirty worktree is kept: {stats:?}"
        );
        assert!(!clean_wt.exists(), "clean worktree dir removed");
        assert!(dirty_wt.exists(), "dirty worktree dir kept");
        // Non-destructive: the reaped worktree's branch ref still exists.
        let branches = git(&repo, &["branch", "--list", "clean"]).await;
        assert!(
            branches.contains("clean"),
            "reaped worktree's branch ref survives: {branches:?}"
        );
    }

    async fn git(cwd: &Path, args: &[&str]) -> String {
        let out = tokio::process::Command::new("git")
            .current_dir(cwd)
            .args(args)
            .output()
            .await
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
}
