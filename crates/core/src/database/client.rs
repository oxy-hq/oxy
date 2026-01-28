use oxy_shared::errors::OxyError;
use sea_orm::{Database, DatabaseConnection};

pub async fn establish_connection() -> Result<DatabaseConnection, OxyError> {
    // OXY_DATABASE_URL is required - PostgreSQL only
    let url = std::env::var("OXY_DATABASE_URL").map_err(|_| {
        OxyError::Database(
            "OXY_DATABASE_URL environment variable is required. \
            Use 'oxy start' to automatically start PostgreSQL with Docker, \
            or set OXY_DATABASE_URL to your PostgreSQL connection string."
                .to_string(),
        )
    })?;

    tracing::debug!("Connecting to PostgreSQL from OXY_DATABASE_URL");

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

    Database::connect(url).await.map_err(|e| {
        tracing::error!("Failed to connect to PostgreSQL database: {}", e);
        OxyError::Database(e.to_string())
    })
}
