pub mod builder;
pub mod manager;

use std::path::PathBuf;

use sea_orm::EntityTrait;
use uuid::Uuid;

use crate::config::resolve_local_project_path;
use crate::db::client::establish_connection;
use crate::errors::OxyError;
use crate::github::GitOperations;

/// Resolve the project path for a given project ID.
///
/// - Nil UUID (local dev): uses `resolve_local_project_path()` to find config.yml
/// - Non-nil UUID (cloud): fetches project from DB and returns git repo path for active branch
pub async fn resolve_project_path(project_id: Uuid) -> Result<PathBuf, OxyError> {
    if project_id.is_nil() {
        // Local development - find project by config.yml location
        resolve_local_project_path().map_err(|e| {
            OxyError::ConfigurationError(format!("Failed to resolve local project path: {}", e))
        })
    } else {
        // Cloud - fetch project from DB and use its active branch
        let conn = establish_connection().await?;
        let project = entity::prelude::Projects::find_by_id(project_id)
            .one(&conn)
            .await
            .map_err(|e| OxyError::DBError(e.to_string()))?
            .ok_or_else(|| OxyError::DBError(format!("Project {} not found", project_id)))?;

        GitOperations::get_repository_path(project_id, project.active_branch_id)
    }
}
