use secrecy::SecretString;
use serde::{Deserialize, Serialize};

/// Authentication for remote git operations.
///
/// `Bearer` is injected as an HTTP header via `-c http.extraHeader`; the
/// token is never persisted to `.git/config` or embedded in the remote URL.
pub enum Auth {
    None,
    Bearer(SecretString),
}

impl Auth {
    pub fn bearer(token: impl Into<String>) -> Self {
        Self::Bearer(SecretString::from(token.into()))
    }
}

/// Per-file status entry returned by [`crate::GitClient::diff_numstat_summary`].
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileStatus {
    pub path: String,
    pub status: String,
    pub insert: u32,
    pub delete: u32,
}
