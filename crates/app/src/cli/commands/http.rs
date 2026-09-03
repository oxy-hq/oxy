//! The CLI's shared HTTP plumbing: one bearer-resolution rule and one client.
//!
//! Extracted from `cli/commands/api.rs` when that command moved out to the
//! TypeScript `oxyc` (`sdk/cli`). The `api` command was its accidental owner —
//! `assume.rs` reached into it for both functions — so deleting the file
//! outright would have taken the auth story for every remaining command with
//! it. They live here now, where nothing has to import a command to get them.
//!
//! One place for the timeout (and any future proxy / TLS settings), and one
//! error message for "you are not logged in", so a caller sees the same
//! instruction whichever command they happened to run.

use oxy_shared::errors::OxyError;

use super::login;

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.trim().is_empty())
}

/// Bearer for `target`: `OXY_TOKEN` (or `token_env`) first — that is the CI
/// path — then the `oxy login` cache for that host.
///
/// The error names the env in the login command it suggests, because the
/// common failure is being logged into production and calling dev, where a
/// bare "run oxy login" sends you to re-authenticate against the host you
/// already have.
pub(super) fn resolve_bearer(target: &str, token_env: &str, env: &str) -> Result<String, OxyError> {
    env_var(token_env)
        .or_else(|| login::load_token(target))
        .ok_or_else(|| {
            OxyError::ConfigurationError(format!(
                "not authenticated for {target}. Run `oxy login --env {env}` (or set {token_env})."
            ))
        })
}

/// The CLI's HTTP client.
pub(super) fn http_client() -> Result<reqwest::Client, OxyError> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| OxyError::RuntimeError(format!("http client init: {e}")))
}
