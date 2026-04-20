use std::path::Path;

use oxy_shared::errors::OxyError;

use crate::cli::{repo, run};

/// Returns `true` if `root` is mid-rebase or mid-merge with conflicts.
pub fn is_in_conflict(root: &Path) -> bool {
    let git_dir = repo::resolve_git_dir(root);
    git_dir.join("rebase-merge").exists()
        || git_dir.join("rebase-apply").exists()
        || git_dir.join("MERGE_HEAD").exists()
}

/// Restores all tracked files to their state at `commit` and creates a new
/// "Restore to …" commit on top of the current HEAD.  History is preserved.
///
/// If an in-progress rebase or merge is active it is aborted first.
pub async fn reset_to_commit(root: &Path, commit: &str) -> Result<(), OxyError> {
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

    let short = if commit.len() > 7 {
        &commit[..7]
    } else {
        commit
    };
    let log = run::run(root, &["log", "--format=%s", "-n", "1", commit])
        .await
        .unwrap_or_default();
    let summary = log.trim();

    run::run(root, &["checkout", commit, "--", "."]).await?;

    let msg = if summary.is_empty() {
        format!("Restore to {short}")
    } else {
        format!("Restore to {short}: {summary}")
    };
    match run::run(root, &["commit", "-m", &msg]).await {
        Ok(_) => {}
        Err(e) if e.to_string().contains("nothing to commit") => {}
        Err(e) => return Err(e),
    }

    Ok(())
}

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

/// Stages all changes and continues an in-progress rebase or merge.
///
/// Sets `GIT_EDITOR=true` so git never opens an interactive editor.
pub async fn continue_rebase(root: &Path) -> Result<(), OxyError> {
    let git_dir = repo::resolve_git_dir(root);
    run::run(root, &["add", "-A"]).await?;
    let subcmd = if git_dir.join("MERGE_HEAD").exists() {
        "merge"
    } else {
        "rebase"
    };
    run::run_no_editor(root, &[subcmd, "--continue"]).await?;
    Ok(())
}
