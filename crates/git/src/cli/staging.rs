use std::path::Path;

use oxy_shared::errors::OxyError;
use tokio::process::Command;

use crate::cli::{path::validate_file_path, repo, run};

/// Writes resolved content to a file and stages it.
///
/// Called after the user has manually edited conflict markers out of a file.
/// Canonicalises the path and verifies it stays under `root`; rejects symlinks
/// so a swap mid-operation cannot redirect the write.
pub async fn write_and_stage_file(
    root: &Path,
    file_path: &str,
    content: &str,
) -> Result<(), OxyError> {
    let full_path = validate_file_path(root, file_path)?;

    // Reject symlinks — tokio::fs::write would follow them and a swap between
    // canonicalize() and write() is otherwise a TOCTOU.
    if let Ok(meta) = tokio::fs::symlink_metadata(&full_path).await
        && meta.file_type().is_symlink()
    {
        return Err(OxyError::ArgumentError(format!(
            "Refusing to write to symlink: {file_path}"
        )));
    }

    tokio::fs::write(&full_path, content)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to write file: {e}")))?;
    run::run(root, &["add", "--", file_path]).await?;
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
    validate_file_path(root, file_path)?;
    let side = if use_mine { "--theirs" } else { "--ours" };
    run::run(root, &["checkout", side, "--", file_path]).await?;
    run::run(root, &["add", "--", file_path]).await?;
    Ok(())
}

/// Restores conflict markers for a previously-resolved file by
/// reconstructing the three-way merge with `git merge-file -p`.
pub async fn unresolve_conflict_file(root: &Path, file_path: &str) -> Result<(), OxyError> {
    let full_path = validate_file_path(root, file_path)?;

    let git_dir = repo::resolve_git_dir(root);

    let is_rebase = git_dir.join("REBASE_HEAD").exists()
        || git_dir.join("rebase-merge").exists()
        || git_dir.join("rebase-apply").exists();
    let is_merge = git_dir.join("MERGE_HEAD").exists();

    if !is_rebase && !is_merge {
        return Err(OxyError::RuntimeError(
            "Not in an active merge or rebase — cannot restore conflict markers".into(),
        ));
    }

    let (ours_ref, theirs_ref) = if is_rebase {
        ("REBASE_HEAD", "HEAD")
    } else {
        ("HEAD", "MERGE_HEAD")
    };

    let ours_content = run::run(root, &["show", &format!("{ours_ref}:{file_path}")])
        .await
        .unwrap_or_default();
    let theirs_content = run::run(root, &["show", &format!("{theirs_ref}:{file_path}")])
        .await
        .unwrap_or_default();

    let base_hash: Option<String> = run::run(root, &["merge-base", ours_ref, theirs_ref])
        .await
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let base_content = match &base_hash {
        Some(hash) => run::run(root, &["show", &format!("{hash}:{file_path}")])
            .await
            .unwrap_or_default(),
        None => String::new(),
    };

    let tmp_dir = std::env::temp_dir();
    let id = uuid::Uuid::new_v4().simple().to_string();
    let tmp_ours = tmp_dir.join(format!("oxy_ours_{id}"));
    let tmp_base = tmp_dir.join(format!("oxy_base_{id}"));
    let tmp_theirs = tmp_dir.join(format!("oxy_theirs_{id}"));

    // RAII guard — removes the temp files on any return path.
    struct TempFiles<'a>(
        &'a std::path::Path,
        &'a std::path::Path,
        &'a std::path::Path,
    );
    impl Drop for TempFiles<'_> {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(self.0);
            let _ = std::fs::remove_file(self.1);
            let _ = std::fs::remove_file(self.2);
        }
    }
    let _cleanup = TempFiles(&tmp_ours, &tmp_base, &tmp_theirs);

    tokio::fs::write(&tmp_ours, ours_content.as_bytes())
        .await
        .map_err(|e| OxyError::RuntimeError(format!("write temp file: {e}")))?;
    tokio::fs::write(&tmp_base, base_content.as_bytes())
        .await
        .map_err(|e| OxyError::RuntimeError(format!("write temp file: {e}")))?;
    tokio::fs::write(&tmp_theirs, theirs_content.as_bytes())
        .await
        .map_err(|e| OxyError::RuntimeError(format!("write temp file: {e}")))?;

    let base_label: &str = base_hash.as_deref().unwrap_or("base");
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

    let exit_code = output.status.code().unwrap_or(-1);
    if exit_code > 1 {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(OxyError::RuntimeError(format!(
            "git merge-file failed: {}",
            stderr.trim()
        )));
    }

    tokio::fs::write(&full_path, &output.stdout)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("write conflict file: {e}")))?;

    let _ = run::run(root, &["restore", "--staged", "--", file_path]).await;

    Ok(())
}
