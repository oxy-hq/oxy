use std::path::Path;

use oxy_shared::errors::OxyError;
use tokio::process::Command;

use crate::cli::{auth, redact};
use crate::types::Auth;

/// Run `git <args>` in `cwd`, no auth. Returns captured stdout on success.
pub(crate) async fn run(cwd: &Path, args: &[&str]) -> Result<String, OxyError> {
    run_authed(cwd, args, &Auth::None).await
}

pub(crate) async fn run_with_token(
    cwd: &Path,
    args: &[&str],
    token: Option<&str>,
) -> Result<String, OxyError> {
    match token {
        Some(t) => run_authed(cwd, args, &Auth::bearer(t)).await,
        None => run(cwd, args).await,
    }
}

/// Like [`run`] but sets `GIT_EDITOR=true` so git never opens an editor —
/// used by `rebase --continue` / `merge --continue`.
pub(crate) async fn run_no_editor(cwd: &Path, args: &[&str]) -> Result<String, OxyError> {
    let output = tokio::process::Command::new("git")
        .current_dir(cwd)
        .env("GIT_EDITOR", "true")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(args)
        .output()
        .await
        .map_err(|e| {
            OxyError::RuntimeError(format!("Failed to execute git {}: {e}", args.join(" ")))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = redact::redact_secrets(&stderr);
        return Err(OxyError::RuntimeError(format!(
            "git {} failed: {stderr}",
            args.join(" ")
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Run `git <args>` in `cwd` with auth injected via `http.extraHeader`.
/// `GIT_TERMINAL_PROMPT=0` makes rejected auth fail fast instead of
/// hanging on a `Username for 'https://...':` prompt.
pub(crate) async fn run_authed(
    cwd: &Path,
    args: &[&str],
    auth_: &Auth,
) -> Result<String, OxyError> {
    let mut cmd = Command::new("git");
    cmd.current_dir(cwd);
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    auth::apply(&mut cmd, auth_);
    cmd.args(args);

    let output = cmd.output().await.map_err(|e| {
        OxyError::RuntimeError(format!("Failed to execute git {}: {e}", args.join(" ")))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = redact::redact_secrets(&stderr);
        return Err(OxyError::RuntimeError(format!(
            "git {} failed: {stderr}",
            args.join(" ")
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
