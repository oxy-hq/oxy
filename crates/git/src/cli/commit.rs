use std::collections::HashSet;
use std::path::Path;

use oxy_shared::errors::OxyError;
use tracing::info;

use crate::cli::{config, run};
use crate::types::RecentCommit;

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

/// Returns up to `n` recent commits starting `offset` commits back from
/// HEAD on the current branch, newest first.
///
/// Dates are **committer** dates (`%cr`), matching the order `git log` walks —
/// see [`RecentCommit`] for why the author date is wrong here. Each entry is
/// also tagged with whether it exists on the upstream tracking ref.
pub async fn get_recent_commits(root: &Path, n: usize, offset: usize) -> Vec<RecentCommit> {
    let skip_arg = format!("--skip={offset}");
    log_commits(root, &[&format!("-{n}"), &skip_arg]).await
}

/// The commits in a `git log` revision range (e.g. `abc123..HEAD`), newest
/// first, capped at `limit`. Same shape and upstream tagging as
/// [`get_recent_commits`].
pub async fn get_recent_commits_in_range(
    root: &Path,
    range: &str,
    limit: usize,
) -> Vec<RecentCommit> {
    log_commits(root, &[&format!("-{limit}"), range]).await
}

/// Shared `git log` invocation + parsing for the commit-listing helpers.
async fn log_commits(root: &Path, extra_args: &[&str]) -> Vec<RecentCommit> {
    let mut args = vec!["log", "--format=%H|%h|%s|%an|%cr"];
    args.extend_from_slice(extra_args);
    let Ok(out) = run::run(root, &args).await else {
        return vec![];
    };

    let unpushed = unpushed_shas(root).await;
    out.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let parts: Vec<&str> = line.splitn(5, '|').collect();
            if parts.len() < 5 {
                return None;
            }
            let hash = parts[0].to_string();
            let on_remote = unpushed.as_ref().map(|set| !set.contains(&hash));
            Some(RecentCommit {
                hash,
                short_hash: parts[1].to_string(),
                subject: parts[2].to_string(),
                author: parts[3].to_string(),
                relative_date: parts[4].to_string(),
                on_remote,
            })
        })
        .collect()
}

/// SHAs reachable from HEAD but not from the upstream tracking ref — i.e. the
/// commits that have never been pushed.
///
/// `None` when there is no upstream (no remote, or a branch that has never been
/// pushed at all), which the caller renders as "unknown" rather than guessing:
/// marking every commit as unpushed on a remote-less workspace would be noise.
async fn unpushed_shas(root: &Path) -> Option<HashSet<String>> {
    // `@{u}` resolves the tracking ref of whatever branch is checked out, so
    // this stays correct on branches whose upstream isn't `origin/<same-name>`.
    // A repo with no upstream exits non-zero here — that's the `None` case.
    run::run(root, &["rev-list", "@{u}..HEAD"])
        .await
        .ok()
        .map(|out| {
            out.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn git(dir: &Path, args: &[&str]) -> String {
        run::run(dir, args).await.unwrap_or_default()
    }

    async fn commit_file(repo: &Path, name: &str, body: &str, msg: &str, authored: Option<&str>) {
        tokio::fs::write(repo.join(name), body).await.unwrap();
        git(repo, &["add", "-A"]).await;
        match authored {
            // `--date` sets the AUTHOR date only; the committer date stays "now".
            // That is exactly the skew a rebase produces, so it lets the test
            // reproduce a rewritten history without sleeping.
            Some(d) => git(repo, &["commit", "--date", d, "-m", msg]).await,
            None => git(repo, &["commit", "-m", msg]).await,
        };
    }

    /// A clone whose local-only commit was authored 4 weeks ago, sitting on top
    /// of an upstream commit made moments ago — the shape `pull --rebase`
    /// leaves behind, and the one that produced the bug report's screenshot.
    async fn setup_rebased_history() -> (TempDir, std::path::PathBuf) {
        let workdir = TempDir::new().unwrap();
        let bare = workdir.path().join("remote.git");
        let local = workdir.path().join("local");

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
        git(&local, &["config", "user.email", "a@a"]).await;
        git(&local, &["config", "user.name", "Oxygen User"]).await;

        commit_file(&local, "base.txt", "v1\n", "base", None).await;
        git(&local, &["push", "-u", "origin", "main"]).await;

        // A local-only commit authored 4 weeks ago and never pushed — the
        // "Auto-commit: Oxygen changes" of the report.
        commit_file(
            &local,
            "junk.txt",
            "x\n",
            "Auto-commit: Oxygen changes",
            Some("2026-06-23T10:00:00Z"),
        )
        .await;

        // Meanwhile someone merges a fix upstream…
        let other = workdir.path().join("other");
        git(
            workdir.path(),
            &["clone", bare.to_str().unwrap(), other.to_str().unwrap()],
        )
        .await;
        git(&other, &["config", "user.email", "b@b"]).await;
        git(&other, &["config", "user.name", "Nick Reshetnikov"]).await;
        commit_file(&other, "fix.txt", "fix\n", "fix(reconcile): abs-band", None).await;
        git(&other, &["push", "origin", "main"]).await;

        // …and the workspace pulls, rebasing the local-only commit on top.
        git(&local, &["pull", "--rebase", "origin", "main"]).await;

        (workdir, local)
    }

    /// Regression: the history list used `%ar` (author date) while `git log`
    /// orders by committer date, so a rebased commit rendered as "4 weeks ago"
    /// directly above one labelled "9 minutes ago" and read as a sort bug.
    /// Dates must descend in the order the rows are returned.
    #[tokio::test]
    async fn relative_dates_descend_with_display_order_after_a_rebase() {
        let (_guard, local) = setup_rebased_history().await;
        let commits = get_recent_commits(&local, 10, 0).await;

        let auto = commits
            .iter()
            .find(|c| c.subject.starts_with("Auto-commit"))
            .expect("rebased local commit present");
        let fix = commits
            .iter()
            .find(|c| c.subject.starts_with("fix(reconcile)"))
            .expect("upstream fix present");

        // The rebase replayed the local commit on top of the fix, so it must
        // sort above it…
        let auto_idx = commits.iter().position(|c| c.hash == auto.hash).unwrap();
        let fix_idx = commits.iter().position(|c| c.hash == fix.hash).unwrap();
        assert!(
            auto_idx < fix_idx,
            "rebased local commit should sort above the upstream commit it was replayed onto"
        );

        // …and its label must reflect the rewrite, not the 4-week-old author
        // date. With `%ar` this read "4 weeks ago"; with `%cr` it is fresh.
        assert!(
            !auto.relative_date.contains("week"),
            "expected a committer-dated label for a just-rebased commit, got {:?} — \
             this is the `%ar` bug: an author-dated row above a newer one",
            auto.relative_date
        );
    }

    /// The never-pushed commit is what strands a workspace (it blocks
    /// fast-forward pulls and makes restore refuse), so it must be flagged.
    #[tokio::test]
    async fn local_only_commits_are_marked_not_on_remote() {
        let (_guard, local) = setup_rebased_history().await;
        let commits = get_recent_commits(&local, 10, 0).await;

        let auto = commits
            .iter()
            .find(|c| c.subject.starts_with("Auto-commit"))
            .unwrap();
        assert_eq!(
            auto.on_remote,
            Some(false),
            "the local-only auto-commit must be flagged as unpushed"
        );

        let fix = commits
            .iter()
            .find(|c| c.subject.starts_with("fix(reconcile)"))
            .unwrap();
        assert_eq!(
            fix.on_remote,
            Some(true),
            "a commit that exists on origin must not be flagged as unpushed"
        );
    }
}
