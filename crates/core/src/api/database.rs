use crate::api::middlewares::project::{ProjectManagerExtractor, ProjectPath};
use crate::{
    auth::extractor::AuthenticatedUserExtractor,
    cli::clean::{clean_all, clean_cache, clean_database_folder, clean_vectors},
    service::sync::sync_databases,
};
use axum::{
    extract::{Json, Path, Query},
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
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path(ProjectPath {
        project_id: _project_id,
    }): Path<ProjectPath>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Query(params): Query<SyncDatabaseQuery>,
) -> Result<Json<DatabaseSyncResponse>, StatusCode> {
    let filter = params.database.map(|db| {
        let datasets = params.datasets.unwrap_or_default();
        (db, datasets)
    });

    let overwrite = true; // Always overwrite

    let config = project_manager.config_manager;
    let secrets_manager = project_manager.secrets_manager;

    match sync_databases(config.clone(), secrets_manager.clone(), filter, overwrite).await {
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
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
) -> Result<Json<Vec<DatabaseInfo>>, StatusCode> {
    let databases = project_manager
        .config_manager
        .list_databases()
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
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Query(params): Query<CleanRequest>,
) -> Result<Json<CleanResponse>, StatusCode> {
    let target = params.target.unwrap_or(CleanTarget::All);

    let mut cleaned_items = Vec::new();
    let mut success = true;
    let mut error_message = String::new();

    match target {
        CleanTarget::All => match clean_all(false, &project_manager.config_manager).await {
            Ok(_) => {
                cleaned_items.extend(vec![
                    "Databases folder".to_string(),
                    "Vector store".to_string(),
                    "Cache".to_string(),
                ]);
            }
            Err(e) => {
                success = false;
                error_message = format!("Failed to clean all: {e}");
            }
        },
        CleanTarget::DatabasesFolder => {
            match clean_database_folder(false, &project_manager.config_manager).await {
                Ok(_) => cleaned_items.push("Databases folder".to_string()),
                Err(e) => {
                    success = false;
                    error_message = format!("Failed to clean databases folder: {e}");
                }
            }
        }
        CleanTarget::Vectors => match clean_vectors(false, &project_manager.config_manager).await {
            Ok(_) => cleaned_items.push("Vector store".to_string()),
            Err(e) => {
                success = false;
                error_message = format!("Failed to clean vectors: {e}");
            }
        },
        CleanTarget::Cache => match clean_cache(false, &project_manager.config_manager).await {
            Ok(_) => cleaned_items.push("Cache".to_string()),
            Err(e) => {
                success = false;
                error_message = format!("Failed to clean cache: {e}");
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
