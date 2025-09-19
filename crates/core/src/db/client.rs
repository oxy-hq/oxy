use crate::{errors::OxyError, state_dir::get_state_dir};
use sea_orm::{Database, DatabaseConnection};

pub async fn establish_connection() -> Result<DatabaseConnection, OxyError> {
    let db_url: Option<String> = std::env::var("OXY_DATABASE_URL").ok();
    if let Some(url) = db_url {
        tracing::info!("Using database URL from environment: {}", url);
        Database::connect(url).await.map_err(|e| {
            tracing::error!("Failed to connect to database: {}", e);
            OxyError::Database(e.to_string())
        })
    } else {
        let state_dir = get_state_dir();
        let db_path = format!(
            "sqlite://{}/db.sqlite?mode=rwc",
            state_dir.to_string_lossy()
        );
        tracing::info!("Using default database path: {}", db_path);
        Database::connect(db_path).await.map_err(|e| {
            tracing::error!("Failed to connect to database: {}", e);
            OxyError::Database(e.to_string())
        })
    }
}
