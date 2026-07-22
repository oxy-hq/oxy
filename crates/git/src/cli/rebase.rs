use std::path::Path;

use oxy_shared::errors::OxyError;

use crate::cli::{commit, repo, run};
use crate::types::{DirtyEntry, DirtyKind, RecentCommit, ResetOutcome};

/// Returns `true` if `root` is mid-rebase or mid-merge — i.e. `rebase-merge`,
/// `rebase-apply`, or `MERGE_HEAD` exists on disk.
///
/// Covers the entire rebase lifecycle, including the "all conflicts staged,
/// waiting on `--continue`" state where HEAD is still detached — callers
/// rely on this to keep the Resolve / Abort affordances reachable and to
/// refuse pushes that would fatal with `src refspec HEAD@<sha> does not
/// match`.
pub fn is_in_conflict(root: &Path) -> bool {
    let git_dir = repo::resolve_git_dir(root);
    git_dir.join("rebase-merge").exists()
        || git_dir.join("rebase-apply").exists()
        || git_dir.join("MERGE_HEAD").exists()
}

/// Lists every working-tree entry that would be lost by a hard reset.
///
/// Parses `git status --porcelain -z` (NUL-delimited so paths with spaces or
/// quotes are unambiguous). Renames are reported as the destination path.
/// Entries under `.worktrees/` are skipped — those are linked worktrees,
/// infrastructure rather than user changes.
pub async fn working_tree_status(root: &Path) -> Result<Vec<DirtyEntry>, OxyError> {
    let raw = run::run(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )
    .await?;
    if raw.is_empty() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    let mut chunks = raw.split('\0').peekable();
    while let Some(chunk) = chunks.next() {
        if chunk.is_empty() {
            continue;
        }
        // Each entry is "XY <path>" (3+ chars). Renames append a second NUL-
        // separated path which we skip — we only care about the current name.
        if chunk.len() < 3 {
            continue;
        }
        let bytes = chunk.as_bytes();
        let x = bytes[0] as char;
        let y = bytes[1] as char;
        let path = chunk[3..].to_string();

        if x == 'R' || y == 'R' {
            // Rename: next chunk is the original path; consume and ignore.
            chunks.next();
        }

        if path.starts_with(".worktrees/") {
            continue;
        }

        let kind = classify_porcelain(x, y);
        out.push(DirtyEntry { path, kind });
    }
    Ok(out)
}

fn classify_porcelain(x: char, y: char) -> DirtyKind {
    if x == '?' && y == '?' {
        return DirtyKind::Untracked;
    }
    if x == 'U' || y == 'U' || (x == 'A' && y == 'A') || (x == 'D' && y == 'D') {
        return DirtyKind::Conflicted;
    }
    if y == 'D' || x == 'D' {
        return DirtyKind::Deleted;
    }
    if y != ' ' && y != '\0' {
        return DirtyKind::Modified;
    }
    DirtyKind::Staged
}

/// Restore the working tree to `commit` and create a new "Restore to …" commit
/// on top of the current HEAD. History is preserved.
///
/// If an in-progress rebase or merge is active it is aborted first.
///
/// Behavior:
/// - If `force` is false and restoring would discard commits made after
///   `commit` (on a GitHub-connected workspace these may include merged pull
///   requests — see #2512), returns [`ResetOutcome::WouldDiscardCommits`] with
///   the commits themselves; nothing is modified.
/// - If `force` is false and the tree has uncommitted changes, returns
///   [`ResetOutcome::Dirty`] without modifying anything.
/// - If `force` is true, performs a hard reset to `commit` followed by
///   `git clean -fd`, then soft-resets HEAD back to its original tip and
///   creates the restore commit. Both tracked and untracked changes are
///   discarded; the working tree ends up exactly matching `commit`.
pub async fn reset_to_commit(
    root: &Path,
    commit: &str,
    force: bool,
) -> Result<ResetOutcome, OxyError> {
    if commit.contains([';', '|', '&', '`', '$', '(', ')']) {
        return Err(OxyError::ArgumentError(format!(
            "Invalid commit ref: {commit}"
        )));
    }

    let git_dir = repo::resolve_git_dir(root);
    if git_dir.join("MERGE_HEAD").exists() {
        let _ = run::run(root, &["merge", "--abort"]).await;
    } else if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
        let _ = run::run(root, &["rebase", "--abort"]).await;
    }

    if !force {
        // #2512: a hard reset to an older commit throws away every commit made
        // after it. On a GitHub-connected workspace those intervening commits
        // include merged pull requests, so an unguarded restore silently
        // reverts them. Refuse unless the caller explicitly forces it.
        let discarded = commits_after(root, commit).await?;
        if !discarded.is_empty() {
            return Ok(ResetOutcome::WouldDiscardCommits(discarded));
        }

        let dirty = working_tree_status(root).await?;
        if !dirty.is_empty() {
            return Ok(ResetOutcome::Dirty(dirty));
        }
    }

    let head_before = run::run(root, &["rev-parse", "HEAD"]).await?;
    let head_before = head_before.trim().to_string();
    if head_before.is_empty() {
        return Err(OxyError::RuntimeError(
            "Cannot determine current HEAD".to_string(),
        ));
    }

    // Skip the reset dance when the target tree already matches HEAD —
    // `git commit` would fail with "nothing to commit" and we can't reliably
    // catch that (git writes it to stdout while `run` only captures stderr).
    let head_tree = run::run(root, &["rev-parse", "HEAD^{tree}"])
        .await?
        .trim()
        .to_string();
    let target_tree = run::run(root, &["rev-parse", &format!("{commit}^{{tree}}")])
        .await?
        .trim()
        .to_string();
    if !head_tree.is_empty() && head_tree == target_tree {
        return Ok(ResetOutcome::Done);
    }

    run::run(root, &["reset", "--hard", commit]).await?;
    let _ = run::run(root, &["clean", "-fd"]).await;
    run::run(root, &["reset", "--soft", &head_before]).await?;

    let short = if commit.len() > 7 {
        &commit[..7]
    } else {
        commit
    };
    let summary = run::run(root, &["log", "--format=%s", "-n", "1", commit])
        .await
        .unwrap_or_default();
    let summary = summary.trim();
    let msg = if summary.is_empty() {
        format!("Restore to {short}")
    } else {
        format!("Restore to {short}: {summary}")
    };
    run::run(root, &["commit", "-m", &msg]).await?;

    Ok(ResetOutcome::Done)
}

/// Number of commits reachable from `HEAD` but not from `commit`, via
/// `git rev-list <commit>..HEAD --count`.
///
/// When `commit` is an ancestor of HEAD — the restore-to-an-older-commit case
/// — this is exactly how many commits a hard reset back to `commit` would
/// discard. Returns 0 when the two are equal or the count can't be parsed.
/// `commit` is already validated against shell metacharacters by the caller.
/// The commits reachable from HEAD but not from `commit` — i.e. exactly what a
/// restore to `commit` would drop. Newest first, each tagged with whether it
/// exists on origin, so the caller can describe the loss precisely.
///
/// Bounded: a restore far back in history could otherwise materialise thousands
/// of rows into a confirmation dialog. The count that matters for the decision
/// is "more than you want to lose", which the first page already conveys.
async fn commits_after(root: &Path, commit: &str) -> Result<Vec<RecentCommit>, OxyError> {
    let range = format!("{commit}..HEAD");
    let count = run::run(root, &["rev-list", "--count", &range])
        .await?
        .trim()
        .parse::<usize>()
        .unwrap_or(0);
    if count == 0 {
        return Ok(vec![]);
    }
    Ok(commit::get_recent_commits_in_range(root, &range, MAX_DISCARDED_COMMITS_LISTED).await)
}

/// How many discarded commits the guard enumerates for the confirmation.
const MAX_DISCARDED_COMMITS_LISTED: usize = 50;

/// Aborts an in-progress rebase or merge.
pub async fn abort_rebase(root: &Path) -> Result<(), OxyError> {
    let git_dir = repo::resolve_git_dir(root);
    if git_dir.join("MERGE_HEAD").exists() {
        run::run(root, &["merge", "--abort"]).await?;
    } else {
        run::run(root, &["rebase", "--abort"]).await?;
    }
    Ok(())
}

/// Discard ALL working-tree changes (tracked + untracked, including
/// untracked directories).
///
/// Aborts any in-progress rebase or merge first — otherwise `reset --hard
/// HEAD` clears the conflict markers but leaves `.git/rebase-merge/` (or
/// `MERGE_HEAD`) behind, so `is_in_conflict()` stays true.
///
/// Destructive — gated by admin capability at the API layer.
pub async fn discard_all_changes(root: &Path) -> Result<(), OxyError> {
    let git_dir = repo::resolve_git_dir(root);
    if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
        run::run(root, &["rebase", "--abort"]).await?;
    } else if git_dir.join("MERGE_HEAD").exists() {
        run::run(root, &["merge", "--abort"]).await?;
    }
    run::run(root, &["reset", "--hard", "HEAD"]).await?;
    run::run(root, &["clean", "-fd"]).await?;
    Ok(())
}

/// Stages all changes and continues an in-progress rebase or merge.
///
/// Sets `GIT_EDITOR=true` so git never opens an interactive editor.
pub async fn continue_rebase(root: &Path) -> Result<(), OxyError> {
    let git_dir = repo::resolve_git_dir(root);
    // `-u` stages tracked-and-modified files only; untracked files (build
    // artefacts, editor swap files) must not ride into the rebased commit.
    run::run(root, &["add", "-u"]).await?;
    let subcmd = if git_dir.join("MERGE_HEAD").exists() {
        "merge"
    } else {
        "rebase"
    };
    run::run_no_editor(root, &[subcmd, "--continue"]).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn git(dir: &Path, args: &[&str]) -> String {
        run::run(dir, args).await.unwrap_or_default()
    }

    /// Set up a repo where pulling from `origin` will conflict on `shared.txt`,
    /// returning the worktree path with the rebase already paused.
    async fn setup_paused_rebase() -> TempDir {
        let workdir = TempDir::new().unwrap();
        let bare = workdir.path().join("remote.git");
        let local = workdir.path().join("local");
        let other = workdir.path().join("other");

        git(
            workdir.path(),
            &["init", "--bare", "-b", "main", bare.to_str().unwrap()],
        )
        .await;
        git(
            workdir.path(),
            &["clone", bare.to_str().unwrap(), local.to_str().unwrap()],
        )
        .await;

        for repo in [&local, &other] {
            tokio::fs::create_dir_all(repo).await.ok();
        }

        // Configure identity on the local clone so commits succeed.
        git(&local, &["config", "user.email", "a@a"]).await;
        git(&local, &["config", "user.name", "A"]).await;

        // Initial commit on main.
        tokio::fs::write(local.join("shared.txt"), "v1\n")
            .await
            .unwrap();
        git(&local, &["add", "."]).await;
        git(&local, &["commit", "-m", "initial"]).await;
        git(&local, &["push", "-u", "origin", "main"]).await;

        // Advance the remote with a conflicting change via a second clone.
        git(
            workdir.path(),
            &["clone", bare.to_str().unwrap(), other.to_str().unwrap()],
        )
        .await;
        git(&other, &["config", "user.email", "b@b"]).await;
        git(&other, &["config", "user.name", "B"]).await;
        tokio::fs::write(other.join("shared.txt"), "remote-version\n")
            .await
            .unwrap();
        git(&other, &["commit", "-am", "remote change"]).await;
        git(&other, &["push"]).await;

        // Local conflicting commit, then pull --rebase to pause on conflict.
        tokio::fs::write(local.join("shared.txt"), "local-version\n")
            .await
            .unwrap();
        git(&local, &["commit", "-am", "local change"]).await;
        let _ = run::run(&local, &["pull", "--rebase", "origin", "main"]).await;

        // Sanity: we're actually paused mid-rebase.
        assert!(
            is_in_conflict(&local),
            "expected rebase to be paused mid-conflict"
        );

        // Hand-off: rename `local` to the TempDir's top so the caller has a clean root.
        let final_path = workdir.path().join("workdir");
        tokio::fs::rename(&local, &final_path).await.unwrap();
        let dir = TempDir::new_in(workdir.path().parent().unwrap()).unwrap();
        tokio::fs::remove_dir_all(dir.path()).await.unwrap();
        tokio::fs::rename(&final_path, dir.path()).await.unwrap();
        dir
    }

    #[tokio::test]
    async fn continue_rebase_does_not_stage_untracked_files() {
        let dir = setup_paused_rebase().await;
        let root = dir.path();

        // Drop untracked files that should NOT end up in the rebased commit.
        tokio::fs::write(root.join("random.txt"), "noise")
            .await
            .unwrap();
        tokio::fs::create_dir_all(root.join("build")).await.unwrap();
        tokio::fs::write(root.join("build/artifact.bin"), "junk")
            .await
            .unwrap();

        // Resolve the conflict (Use Theirs) — bypass staging::resolve_conflict_file
        // so the test doesn't depend on path canonicalization (TempDir lives under
        // /tmp which is a symlink on macOS).
        git(root, &["checkout", "--ours", "--", "shared.txt"]).await;
        git(root, &["add", "--", "shared.txt"]).await;

        continue_rebase(root).await.unwrap();

        // The rebased commit must contain only shared.txt.
        let stat = git(root, &["show", "HEAD", "--name-only", "--format="]).await;
        let files: Vec<&str> = stat.split_whitespace().collect();
        assert_eq!(
            files,
            vec!["shared.txt"],
            "rebased commit should only contain shared.txt, got: {files:?}"
        );

        // And the untracked files should still exist on disk, just not in the commit.
        assert!(root.join("random.txt").exists());
        assert!(root.join("build/artifact.bin").exists());
    }

    /// Build a repo with a linear three-commit history on `main`, returning the
    /// worktree and the SHA of the first (oldest) commit.
    async fn setup_linear_history() -> (TempDir, String) {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        git(root, &["init", "-b", "main"]).await;
        git(root, &["config", "user.email", "a@a"]).await;
        git(root, &["config", "user.name", "A"]).await;

        tokio::fs::write(root.join("f.txt"), "one\n").await.unwrap();
        git(root, &["add", "."]).await;
        git(root, &["commit", "-m", "c1"]).await;
        let base = git(root, &["rev-parse", "HEAD"]).await.trim().to_string();

        tokio::fs::write(root.join("f.txt"), "two\n").await.unwrap();
        git(root, &["commit", "-am", "c2"]).await;

        tokio::fs::write(root.join("f.txt"), "three\n")
            .await
            .unwrap();
        git(root, &["commit", "-am", "c3"]).await;

        (dir, base)
    }

    #[tokio::test]
    async fn reset_to_ancestor_with_intervening_commits_requires_force() {
        let (dir, base) = setup_linear_history().await;
        let root = dir.path();

        // Two commits (c2, c3) sit between `base` and HEAD — restoring without
        // force must refuse and leave history untouched (#2512).
        let result = reset_to_commit(root, &base, false).await;
        let ResetOutcome::WouldDiscardCommits(discarded) = result.expect("guard is not an error")
        else {
            panic!("expected WouldDiscardCommits guarding intervening commits");
        };

        // The refusal names the commits, so the UI can show what is at stake and
        // offer to force — a bare error string left it with nothing to act on.
        let subjects: Vec<_> = discarded.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(
            subjects,
            vec!["c3", "c2"],
            "discarded commits should be listed newest-first"
        );

        // HEAD is unchanged — still on c3.
        let head_subject = git(root, &["log", "--format=%s", "-n", "1", "HEAD"]).await;
        assert_eq!(head_subject.trim(), "c3");
    }

    #[tokio::test]
    async fn reset_to_ancestor_with_force_succeeds() {
        let (dir, base) = setup_linear_history().await;
        let root = dir.path();

        let outcome = reset_to_commit(root, &base, true).await.unwrap();
        assert!(matches!(outcome, ResetOutcome::Done));

        // Working tree now matches the base commit's content.
        let content = tokio::fs::read_to_string(root.join("f.txt")).await.unwrap();
        assert_eq!(content, "one\n");

        // History is preserved: a new "Restore to …" commit sits on top of c3.
        let subject = git(root, &["log", "--format=%s", "-n", "1", "HEAD"]).await;
        assert!(
            subject.trim().starts_with("Restore to"),
            "expected a restore commit on top, got: {subject:?}"
        );
    }

    #[tokio::test]
    async fn reset_without_intervening_commits_succeeds() {
        let (dir, _base) = setup_linear_history().await;
        let root = dir.path();

        // Restoring to HEAD itself has zero intervening commits, so the guard
        // never fires even with force=false — the target tree already matches
        // HEAD and the call short-circuits to Done.
        let head = git(root, &["rev-parse", "HEAD"]).await.trim().to_string();
        let outcome = reset_to_commit(root, &head, false).await.unwrap();
        assert!(matches!(outcome, ResetOutcome::Done));
    }

    #[tokio::test]
    async fn is_in_conflict_stays_true_after_files_resolved_but_not_continued() {
        let dir = setup_paused_rebase().await;
        let root = dir.path();

        // Paused mid-conflict.
        assert!(is_in_conflict(root));

        // Resolve everything but do not continue. HEAD is still detached and
        // `git push origin HEAD@<sha>` would fatal, so the UI must keep the
        // Resolve affordance reachable — `is_in_conflict` must stay true.
        git(root, &["checkout", "--ours", "--", "shared.txt"]).await;
        git(root, &["add", "--", "shared.txt"]).await;
        assert!(is_in_conflict(root));

        // After `--continue` the rebase is over and the flag clears.
        continue_rebase(root).await.unwrap();
        assert!(!is_in_conflict(root));
    }
}
