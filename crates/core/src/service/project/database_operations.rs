use crate::db::client::establish_connection;
use crate::errors::OxyError;
use chrono::Utc;
use sea_orm::DatabaseConnection;
use uuid::Uuid;

pub struct DatabaseOperations;

impl DatabaseOperations {
    #[inline]
    pub fn now() -> chrono::DateTime<chrono::Utc> {
        Utc::now()
    }

    pub async fn get_connection() -> Result<DatabaseConnection, OxyError> {
        establish_connection().await
    }

    pub fn wrap_db_error<E: std::fmt::Display>(msg: &str, e: E) -> OxyError {
        OxyError::DBError(format!("{}: {}", msg, e))
    }

    pub async fn with_connection<F, Fut, T>(f: F) -> Result<T, OxyError>
    where
        F: FnOnce(DatabaseConnection) -> Fut,
        Fut: std::future::Future<Output = Result<T, OxyError>>,
    {
        let db = Self::get_connection().await?;
        f(db).await
    }
}

pub struct ValidationUtils;

impl ValidationUtils {
    pub fn parse_repo_id(repo_id_str: &str) -> Result<i64, OxyError> {
        repo_id_str
            .parse::<i64>()
            .map_err(|_| OxyError::ConfigurationError("Invalid repository ID".to_string()))
    }

    pub fn validate_project_branch_relationship(
        branch_project_id: Uuid,
        expected_project_id: Uuid,
    ) -> Result<(), OxyError> {
        if branch_project_id != expected_project_id {
            return Err(OxyError::RuntimeError(
                "Branch does not belong to the specified project".to_string(),
            ));
        }
        Ok(())
    }
}
