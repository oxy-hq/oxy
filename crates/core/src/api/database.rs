use crate::{
    auth::extractor::AuthenticatedUserExtractor,
    cli::clean::{clean_all, clean_cache, clean_database_folder, clean_vectors},
    config::ConfigBuilder,
    project::resolve_project_path,
    service::sync::sync_databases,
};
use axum::{
    extract::{Json, Query},
    http::StatusCode,
};
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;
#[derive(Serialize, ToSchema)]
pub struct DatabaseInfo {
    pub name: String,
    pub dialect: String,
    pub datasets: HashMap<String, Vec<String>>,
}

#[derive(Serialize, ToSchema)]
pub struct DatabaseSyncResponse {
    pub success: bool,
    pub message: String,
    pub sync_time_secs: Option<f64>,
}

// support deserializing datasets as either a single string or a list of strings
fn deserialize_datasets<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<serde_json::Value>::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(vec![s])),
        Some(serde_json::Value::Array(arr)) => {
            let mut result = Vec::with_capacity(arr.len());
            for v in arr {
                match v {
                    serde_json::Value::String(s) => result.push(s),
                    _ => return Err(de::Error::custom("Expected string in datasets array")),
                }
            }
            Ok(Some(result))
        }
        _ => Err(de::Error::custom("Invalid type for datasets")),
    }
}

#[derive(Deserialize, ToSchema)]
pub struct SyncDatabaseQuery {
    pub database: Option<String>,
    #[serde(default, deserialize_with = "deserialize_datasets")]
    pub datasets: Option<Vec<String>>,
}

pub async fn sync_database(
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Query(params): Query<SyncDatabaseQuery>,
) -> Result<Json<DatabaseSyncResponse>, StatusCode> {
    let project_path = resolve_project_path().map_err(|e| {
        tracing::error!("Failed to find project path: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let config = ConfigBuilder::new()
        .with_project_path(&project_path)
        .map_err(|e| {
            tracing::error!("Failed to create config builder: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .build()
        .await
        .map_err(|e| {
            tracing::error!("Failed to build config: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let filter = params.database.map(|db| {
        let datasets = params.datasets.unwrap_or_default();
        (db, datasets)
    });

    let overwrite = true; // Always overwrite

    match sync_databases(config, filter, overwrite).await {
        Ok(results) => {
            let success_count = results.iter().filter(|r| r.is_ok()).count();
            let error_count = results.iter().filter(|r| r.is_err()).count();

            // Calculate average sync time from successful results
            let total_sync_time: f64 = results
                .iter()
                .filter_map(|result| match result {
                    Ok(sync_metrics) => Some(sync_metrics.sync_time_secs),
                    Err(_) => None,
                })
                .sum();

            let avg_sync_time = if success_count > 0 {
                Some(total_sync_time / success_count as f64)
            } else {
                None
            };

            let message = if error_count == 0 {
                if success_count == 1 {
                    "Database synced successfully".to_string()
                } else {
                    format!("{success_count} databases synced successfully")
                }
            } else if success_count == 0 {
                "Failed to sync databases".to_string()
            } else {
                format!("{success_count} databases synced, {error_count} failed")
            };

            Ok(Json(DatabaseSyncResponse {
                success: error_count == 0,
                message,
                sync_time_secs: avg_sync_time,
            }))
        }
        Err(e) => {
            tracing::error!("Database sync failed: {}", e);
            Ok(Json(DatabaseSyncResponse {
                success: false,
                message: format!("Database sync failed: {e}"),
                sync_time_secs: None,
            }))
        }
    }
}

pub async fn list_databases(
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
) -> Result<Json<Vec<DatabaseInfo>>, StatusCode> {
    let project_path = resolve_project_path().map_err(|e| {
        tracing::error!("Failed to find project path: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let config_builder = ConfigBuilder::new()
        .with_project_path(&project_path)
        .map_err(|e| {
            tracing::error!("Failed to create config builder: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let config = config_builder.build().await.map_err(|e| {
        tracing::error!("Failed to build config: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let databases = config
        .list_databases()
        .map_err(|e| {
            tracing::error!("Failed to list databases: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .iter()
        .map(|db| DatabaseInfo {
            name: db.name.clone(),
            dialect: db.dialect(),
            datasets: db.datasets(),
        })
        .collect::<Vec<DatabaseInfo>>();

    Ok(Json(databases))
}

#[derive(Debug, Deserialize)]
pub struct CleanRequest {
    target: Option<CleanTarget>, // "all", "DatabasesFolder", "Vectors", "Cache"
                                 // DatabasesFolder: semantic models and build artifacts
                                 // Vectors: LanceDB embeddings and search indexes
                                 // Cache: temporary files, logs, and chart cache
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanTarget {
    All,
    DatabasesFolder,
    Vectors,
    Cache,
}
#[derive(Debug, Serialize)]
pub struct CleanResponse {
    success: bool,
    message: String,
    cleaned_items: Vec<String>,
}

pub async fn clean_data(
    _user: AuthenticatedUserExtractor,
    Query(params): Query<CleanRequest>,
) -> Result<Json<CleanResponse>, StatusCode> {
    let target = params.target.unwrap_or(CleanTarget::All);

    let mut cleaned_items = Vec::new();
    let mut success = true;
    let mut error_message = String::new();

    match target {
        CleanTarget::All => match clean_all(false).await {
            Ok(_) => {
                cleaned_items.extend(vec![
                    "Databases folder".to_string(),
                    "Vector store".to_string(),
                    "Cache".to_string(),
                ]);
            }
            Err(e) => {
                success = false;
                error_message = format!("Failed to clean all: {}", e);
            }
        },
        CleanTarget::DatabasesFolder => match clean_database_folder(false).await {
            Ok(_) => cleaned_items.push("Databases folder".to_string()),
            Err(e) => {
                success = false;
                error_message = format!("Failed to clean databases folder: {}", e);
            }
        },
        CleanTarget::Vectors => match clean_vectors(false).await {
            Ok(_) => cleaned_items.push("Vector store".to_string()),
            Err(e) => {
                success = false;
                error_message = format!("Failed to clean vectors: {}", e);
            }
        },
        CleanTarget::Cache => match clean_cache(false).await {
            Ok(_) => cleaned_items.push("Cache".to_string()),
            Err(e) => {
                success = false;
                error_message = format!("Failed to clean cache: {}", e);
            }
        },
    }

    if success {
        Ok(Json(CleanResponse {
            success: true,
            message: format!("Successfully cleaned: {}", cleaned_items.join(", ")),
            cleaned_items,
        }))
    } else {
        tracing::error!("{}", error_message);
        Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
}
