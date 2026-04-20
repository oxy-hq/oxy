pub mod app_auth;
pub mod auth;
pub mod client;
pub mod types;

pub use app_auth::*;
pub use auth::*;
pub use client::*;
pub use types::*;

use oxy_git::cli::CliGitClient;
use oxy_shared::errors::OxyError;

/// Construct the default git client.
///
/// All call sites that previously used `GitOperations` should migrate to
/// this factory, then to an injected `GitClient` from app state.
pub fn default_git_client() -> CliGitClient {
    CliGitClient::new()
}

/// Resolve a usable GitHub token for a namespace row.
///
/// PAT namespaces return the stored OAuth token directly; GitHub App
/// namespaces mint a short-lived installation token.
pub async fn github_token_for_namespace(
    ns: &entity::git_namespaces::Model,
) -> Result<String, OxyError> {
    if ns.slug == "pat" {
        if ns.oauth_token.is_empty() {
            return Err(OxyError::RuntimeError(
                "PAT namespace has empty token".to_string(),
            ));
        }
        return Ok(ns.oauth_token.clone());
    }
    let app_auth = GitHubAppAuth::from_env()?;
    app_auth
        .get_installation_token(&ns.installation_id.to_string())
        .await
}

/// Resolve an optional GitHub token for a workspace.
///
/// Walks `workspace → git_namespace` and delegates to
/// [`github_token_for_namespace`]. Returns `Ok(None)` when no namespace is
/// configured — callers should fall back to the machine's git credentials
/// (SSH, credential helper). `Err` is reserved for the case where a
/// namespace IS configured but the token can't be fetched.
pub async fn github_token_for_workspace(
    workspace: &entity::workspaces::Model,
) -> Result<Option<String>, OxyError> {
    use sea_orm::EntityTrait;

    let Some(namespace_id) = workspace.git_namespace_id else {
        return Ok(None);
    };
    let db = crate::database::client::establish_connection().await?;
    let ns = entity::git_namespaces::Entity::find_by_id(namespace_id)
        .one(&db)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to find git namespace: {e}")))?
        .ok_or_else(|| OxyError::RuntimeError("Git namespace not found".to_string()))?;
    github_token_for_namespace(&ns).await.map(Some)
}
