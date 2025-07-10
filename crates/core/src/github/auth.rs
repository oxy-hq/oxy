use crate::utils::get_encryption_key;
use std::env;

/// Environment variables for GitHub integration
pub struct GitHubEnv;

impl GitHubEnv {
    /// Get the GitHub API base URL from environment, defaults to https://api.github.com
    pub fn github_api_url() -> String {
        env::var("GITHUB_API_URL").unwrap_or_else(|_| "https://api.github.com".to_string())
    }

    /// Get the encryption key for GitHub tokens from environment
    /// Falls back to a default key for development (NOT secure for production)
    pub fn encryption_key() -> [u8; 32] {
        get_encryption_key()
    }
}
