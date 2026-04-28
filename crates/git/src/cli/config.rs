use oxy_shared::errors::OxyError;
use tokio::process::Command;
use tracing::warn;

/// Ensure global `user.name` / `user.email` are set so git commits don't fail.
///
/// Only writes defaults when the value is missing — never overwrites an
/// existing config.
pub async fn ensure_user_config() -> Result<(), OxyError> {
    let name = Command::new("git")
        .args(["config", "user.name"])
        .output()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to check git config: {e}")))?;

    if !name.status.success() {
        warn!("Git user.name is not configured. Setting default value.");
        let _ = Command::new("git")
            .args(["config", "--global", "user.name", "Oxygen User"])
            .output()
            .await;
    }

    let email = Command::new("git")
        .args(["config", "user.email"])
        .output()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to check git config: {e}")))?;

    if !email.status.success() {
        warn!("Git user.email is not configured. Setting default value.");
        let _ = Command::new("git")
            .args(["config", "--global", "user.email", "user@oxy.local"])
            .output()
            .await;
    }

    Ok(())
}
