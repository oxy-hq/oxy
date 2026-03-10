use oxy_shared::errors::OxyError;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::OnceCell;

/// Global shared database connection pool.
///
/// This ensures all components share the same connection pool, preventing
/// pool exhaustion issues when running concurrent procedures.
///
/// Previously, each call to `establish_connection()` created a new pool with
/// max 10 connections. With multiple components (RunsManager, CheckpointStorage,
/// SecretsManager, etc.) each creating their own pool, the total connections
/// could exceed PostgreSQL's `max_connections` limit.
static GLOBAL_DB_POOL: OnceCell<Arc<DatabaseConnection>> = OnceCell::const_new();

/// Get or create the shared database connection pool.
///
/// This is the preferred method for obtaining a database connection.
/// It returns a clone of the Arc-wrapped connection, which is cheap
/// since DatabaseConnection is already internally Arc-based.
pub async fn establish_connection() -> Result<DatabaseConnection, OxyError> {
    let conn = GLOBAL_DB_POOL
        .get_or_try_init(|| async {
            let conn = create_connection_pool().await?;
            Ok::<Arc<DatabaseConnection>, OxyError>(Arc::new(conn))
        })
        .await?;

    // Clone the Arc's inner DatabaseConnection
    // Sea-ORM's DatabaseConnection is Clone and internally uses Arc,
    // so this is a cheap operation that shares the underlying pool
    Ok((*conn).clone())
}

/// Create a new database connection pool.
///
/// This is called once by `establish_connection()` to initialize the global pool.
/// Configuration is read from environment variables:
///
/// - `OXY_DATABASE_URL`: PostgreSQL connection string (required)
/// - `OXY_DB_MAX_CONNECTIONS`: Maximum connections in pool (default: 20)
/// - `OXY_DB_MIN_CONNECTIONS`: Minimum connections in pool (default: 2)
/// - `OXY_DB_CONNECT_TIMEOUT_SECS`: Connection timeout in seconds (default: 30)
/// - `OXY_DB_ACQUIRE_TIMEOUT_SECS`: Pool acquire timeout in seconds (default: 30)
///
/// The default timeouts are increased from the original 10 seconds to 30 seconds
/// to better handle slower Docker networking environments like Rancher Desktop
/// on Windows (WSL2 backend).
async fn create_connection_pool() -> Result<DatabaseConnection, OxyError> {
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

    // Read pool configuration from environment with sensible defaults
    // Increased defaults for better Windows/Rancher Desktop compatibility
    let max_connections: u32 = std::env::var("OXY_DB_MAX_CONNECTIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20); // Increased from 10 to handle concurrent procedures

    let min_connections: u32 = std::env::var("OXY_DB_MIN_CONNECTIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2); // Increased from 1 for better concurrency

    let connect_timeout_secs: u64 = std::env::var("OXY_DB_CONNECT_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30); // Increased from 10 for Windows/Rancher

    let acquire_timeout_secs: u64 = std::env::var("OXY_DB_ACQUIRE_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30); // Increased from 10 for Windows/Rancher

    tracing::info!(
        "Creating database connection pool: max_connections={}, min_connections={}, connect_timeout={}s, acquire_timeout={}s",
        max_connections,
        min_connections,
        connect_timeout_secs,
        acquire_timeout_secs
    );

    // Configure connection pool for resilience against intermittent connection issues
    let mut opt = ConnectOptions::new(url);
    opt.max_connections(max_connections)
        .min_connections(min_connections)
        .connect_timeout(Duration::from_secs(connect_timeout_secs))
        .acquire_timeout(Duration::from_secs(acquire_timeout_secs))
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

#[cfg(test)]
mod tests {
    // Note: Tests that need a fresh pool can use create_connection_pool() directly
    // The global pool is initialized once and reused across tests
}
