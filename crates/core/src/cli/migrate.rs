use crate::db::client::establish_connection;
use crate::errors::OxyError;
use migration::Migrator;
use migration::MigratorTrait;

pub async fn migrate() -> Result<(), OxyError> {
    // Ensure the database is migrated to the latest version
    let db = establish_connection()
        .await
        .map_err(|e| OxyError::DBError(format!("Failed to establish database connection: {e}")))?;
    Migrator::up(&db, None)
        .await
        .map_err(|e| OxyError::DBError(format!("Failed to run migrations: {e}")))?;
    Ok(())
}
