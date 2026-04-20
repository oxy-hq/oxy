use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use secrecy::ExposeSecret;
use tokio::process::Command;

use crate::types::Auth;

/// Attach auth to a `git` invocation.
///
/// Uses `-c http.extraHeader=Authorization: Basic <base64("x-access-token:TOKEN")>`
/// so the token is never persisted to `.git/config` or leaked into a remote URL.
///
/// GitHub's git Smart HTTP backend rejects `Authorization: Bearer` for
/// git operations (it only accepts Bearer on the REST API); it requires
/// Basic auth with `x-access-token` as the username. This matches the
/// scheme used by `actions/checkout`.
pub(crate) fn apply(cmd: &mut Command, auth: &Auth) {
    if let Auth::Bearer(token) = auth {
        let encoded = BASE64.encode(format!("x-access-token:{}", token.expose_secret()));
        cmd.arg("-c")
            .arg(format!("http.extraHeader=Authorization: Basic {encoded}"));
    }
}
