use crate::{errors::OxyError, state_dir::get_state_dir};
use sea_orm::{Database, DatabaseConnection};

pub async fn establish_connection() -> Result<DatabaseConnection, OxyError> {
    // If OXY_DATABASE_URL is set, use PostgreSQL (external or Docker-managed)
    if let Ok(url) = std::env::var("OXY_DATABASE_URL") {
        tracing::info!("Using PostgreSQL from OXY_DATABASE_URL");
        // Validate that the URL is a PostgreSQL connection string
        if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
            tracing::error!(
                "OXY_DATABASE_URL must be a PostgreSQL connection string (starting with 'postgres://' or 'postgresql://'). Got: {}",
                url
            );
            return Err(OxyError::Database(
                "OXY_DATABASE_URL must be a PostgreSQL connection string (starting with 'postgres://' or 'postgresql://')".to_string()
            ));
        }
        return Database::connect(url).await.map_err(|e| {
            tracing::error!("Failed to connect to PostgreSQL database: {}", e);
            OxyError::Database(e.to_string())
        });
    }

    // Otherwise, default to SQLite for backward compatibility
    tracing::info!("Using SQLite database (default for backward compatibility)");
    let state_dir = get_state_dir();
    let db_path = state_dir.join("db.sqlite");
    let connection_string = format!("sqlite://{}?mode=rwc", db_path.display());

    tracing::info!("Connecting to SQLite at {}", db_path.display());
    Database::connect(connection_string).await.map_err(|e| {
        tracing::error!("Failed to connect to SQLite database: {}", e);
        OxyError::Database(e.to_string())
    })
}
