use std::path::Path;

use oxy_shared::errors::OxyError;
use tokio::process::Command;

use crate::cli::auth;
use crate::types::Auth;

/// Run `git <args>` in `cwd`, no auth. Returns captured stdout on success.
pub(crate) async fn run(cwd: &Path, args: &[&str]) -> Result<String, OxyError> {
    run_authed(cwd, args, &Auth::None).await
}

/// Bridge for callers that hold a raw `Option<&str>` token. Converts
/// to [`Auth::Bearer`] internally.
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

/// Like [`run`] but sets `GIT_EDITOR=true` and `GIT_TERMINAL_PROMPT=0`
/// so git never opens an interactive editor or credential prompt.
/// Used by `rebase --continue` / `merge --continue`.
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
        return Err(OxyError::RuntimeError(format!(
            "git {} failed: {stderr}",
            args.join(" ")
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Run `git <args>` in `cwd` with auth injected via `http.extraHeader`.
///
/// Sets `GIT_TERMINAL_PROMPT=0` so that if auth is rejected, git fails
/// fast with a clear error instead of hanging on an interactive
/// `Username for 'https://...':` prompt.
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
        return Err(OxyError::RuntimeError(format!(
            "git {} failed: {stderr}",
            args.join(" ")
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
