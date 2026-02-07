use oxy_shared::errors::OxyError;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use std::time::Duration;

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

    // Configure connection pool for resilience against intermittent connection issues
    let mut opt = ConnectOptions::new(url);
    opt.max_connections(10)
        .min_connections(1)
        .connect_timeout(Duration::from_secs(10))
        .acquire_timeout(Duration::from_secs(10))
        // Close idle connections after 5 minutes to avoid stale connections
        .idle_timeout(Duration::from_secs(300))
        // Max lifetime of 30 minutes to force connection refresh
        .max_lifetime(Duration::from_secs(1800))
        // Test connections before use to detect "Connection reset by peer" errors
        .test_before_acquire(true)
        .sqlx_logging(false);

    Database::connect(opt).await.map_err(|e| {
        tracing::error!("Failed to connect to PostgreSQL database: {}", e);
        OxyError::Database(e.to_string())
    })
}
