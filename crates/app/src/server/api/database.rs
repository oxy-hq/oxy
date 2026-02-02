use crate::server::api::middlewares::project::{ProjectManagerExtractor, ProjectPath};
use crate::{
    cli::commands::clean::{clean_all, clean_cache, clean_database_folder, clean_vectors},
    server::service::{
        project::{
            database_config::DatabaseConfigBuilder,
            models::{WarehouseConfig, WarehousesFormData},
        },
        sync::sync_databases,
    },
};
use axum::{
    extract::{Json, Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response, sse::Sse},
};
use oxy::config::model::{DatabaseType, SnowflakeAuthType};
use oxy::connector::Connector;
use oxy::semantic::SemanticManager;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use scopeguard::guard;
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use tokio::sync::mpsc;
use utoipa::ToSchema;
#[derive(Serialize, ToSchema)]
pub struct DatabaseInfo {
    pub name: String,
    pub dialect: String,
    pub datasets: HashMap<String, Vec<String>>,
    pub synced: bool,
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

            // Collect error messages
            let error_messages: Vec<String> = results
                .iter()
                .filter_map(|result| match result {
                    Err(e) => Some(e.to_string()),
                    Ok(_) => None,
                })
                .collect();

            let message = if error_count == 0 {
                if success_count == 1 {
                    "Database synced successfully".to_string()
                } else {
                    format!("{success_count} databases synced successfully")
                }
            } else if success_count == 0 {
                format!("Failed to sync: {}", error_messages.join("; "))
            } else {
                format!(
                    "{success_count} databases synced, {error_count} failed: {}",
                    error_messages.join("; ")
                )
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
    let config_manager = &project_manager.config_manager;
    let secrets_manager = &project_manager.secrets_manager;

    let semantic_manager =
        SemanticManager::from_config(config_manager.clone(), secrets_manager.clone(), false)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut databases = Vec::new();

    for db in config_manager.list_databases() {
        // Try to load cached database info (without triggering sync)
        let (datasets, synced) = match semantic_manager
            .try_load_cached_database_info(&db.name)
            .await
        {
            Ok(Some(db_info)) => {
                // Extract table names from semantic_info keys for each dataset
                let datasets = db_info
                    .datasets
                    .into_iter()
                    .map(|(dataset_name, dataset_info)| {
                        let tables: Vec<String> =
                            dataset_info.semantic_info.keys().cloned().collect();
                        (dataset_name, tables)
                    })
                    .collect();
                (datasets, true)
            }
            Ok(None) => {
                // Not synced yet - return empty datasets
                (HashMap::new(), false)
            }
            Err(_) => {
                // Error loading - return empty datasets
                (HashMap::new(), false)
            }
        };

        databases.push(DatabaseInfo {
            name: db.name.clone(),
            dialect: db.dialect(),
            datasets,
            synced,
        });
    }

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

#[derive(Serialize, ToSchema)]
pub struct CreateDatabaseConfigResponse {
    pub success: bool,
    pub message: String,
    pub databases_added: Vec<String>,
}

/// Creates database configurations and updates the config.yml file
#[utoipa::path(
    post,
    path = "/projects/{project_id}/databases",
    request_body = WarehousesFormData,
    params(
        ("project_id" = Uuid, Path, description = "Project ID")
    ),
    responses(
        (status = 201, description = "Database configurations created successfully", body = CreateDatabaseConfigResponse),
        (status = 400, description = "Bad request - validation failed"),
        (status = 409, description = "Conflict - database with same name already exists"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Databases"
)]
pub async fn create_database_config(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path(ProjectPath { project_id: _ }): Path<ProjectPath>,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Json(warehouses_form): Json<WarehousesFormData>,
) -> Result<Response, StatusCode> {
    // Get the project path from the config manager
    let repo_path = project_manager.config_manager.project_path();

    tracing::info!(
        "Creating database configurations {:?}",
        warehouses_form.warehouses
    );
    // Build database configurations
    let databases = DatabaseConfigBuilder::build_configs(
        &warehouses_form,
        repo_path,
        user.id,
        &project_manager.secrets_manager,
    )
    .await?;

    // Collect database names for response
    let database_names: Vec<String> = databases.iter().map(|db| db.name.clone()).collect();

    // Add databases to the config and write to config.yml
    match project_manager
        .config_manager
        .add_databases(databases)
        .await
    {
        Ok(_) => {
            let response = CreateDatabaseConfigResponse {
                success: true,
                message: format!(
                    "{} database configuration(s) created successfully",
                    database_names.len()
                ),
                databases_added: database_names,
            };
            Ok((StatusCode::CREATED, Json(response)).into_response())
        }
        Err(e) => {
            tracing::error!("Failed to add databases to config: {}", e);

            // Check if it's a duplicate database error
            if e.to_string().contains("already exists") {
                Ok((
                    StatusCode::CONFLICT,
                    Json(json!({
                        "success": false,
                        "error": e.to_string()
                    })),
                )
                    .into_response())
            } else {
                Ok((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "success": false,
                        "error": format!("Failed to update configuration: {}", e)
                    })),
                )
                    .into_response())
            }
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct TestDatabaseConnectionRequest {
    pub warehouse: WarehouseConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TestDatabaseConnectionResponse {
    pub success: bool,
    pub message: String,
    pub connection_time_ms: Option<u64>,
    pub error_details: Option<String>,
}

/// Connection test event types for SSE streaming
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConnectionTestEvent {
    Progress {
        message: String,
    },
    BrowserAuthRequired {
        sso_url: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout_secs: Option<u64>,
    },
    Complete {
        result: TestDatabaseConnectionResponse,
    },
}

/// Test a database connection with real-time progress via SSE
#[utoipa::path(
    post,
    path = "/projects/{project_id}/databases/test-connection",
    request_body = TestDatabaseConnectionRequest,
    params(
        ("project_id" = uuid::Uuid, Path, description = "Project ID")
    ),
    responses(
        (status = 200, description = "Connection test stream", content_type = "text/event-stream"),
    ),
    security(
        ("ApiKey" = [])
    ),
    tag = "Databases"
)]
pub async fn test_database_connection(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path(ProjectPath { project_id: _ }): Path<ProjectPath>,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Json(request): Json<TestDatabaseConnectionRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let (tx, rx) = mpsc::channel::<ConnectionTestEvent>(100);
    // Build temp database config
    let temp_db_name = format!("test_conn_{}", uuid::Uuid::new_v4());
    let repo_path = project_manager.config_manager.project_path();

    let database_config = match DatabaseConfigBuilder::build_configs(
        &WarehousesFormData {
            warehouses: vec![WarehouseConfig {
                r#type: request.warehouse.r#type.clone(),
                name: Some(temp_db_name.clone()),
                config: request.warehouse.config,
            }],
        },
        repo_path,
        user.id,
        &project_manager.secrets_manager,
    )
    .await
    {
        Ok(config) => config,
        Err(_) => {
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    if database_config.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    tokio::spawn(async move {
        let start_time = std::time::Instant::now();

        // Progress: Start
        let _ = tx
            .send(ConnectionTestEvent::Progress {
                message: "Initiating connection test...".to_string(),
            })
            .await;

        // Set up scope guard to clean up secrets after testing
        let secret_name = format!("{}_PASSWORD", temp_db_name.to_uppercase());
        let secrets_manager = project_manager.secrets_manager.clone();
        let _cleanup_guard = guard((), move |_| {
            let secret_name = secret_name.clone();
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async move {
                    tracing::info!("Cleaning up temporary secret: {}", secret_name);
                    // Delete the temporary secret
                    secrets_manager
                        .remove_secret(&secret_name)
                        .await
                        .unwrap_or_else(|e| {
                            tracing::error!(
                                "Failed to delete temporary secret {}: {}",
                                secret_name,
                                e
                            );
                        });
                })
            });
        });

        let db_config = &database_config[0];

        let _ = tx
            .send(ConnectionTestEvent::Progress {
                message: "Creating connector...".to_string(),
            })
            .await;

        // Create SSO URL channel
        let (sso_tx, mut sso_rx) = mpsc::channel::<String>(1);

        // Check if Snowflake browser auth
        let is_snowflake_browser = matches!(
            &db_config.database_type,
            DatabaseType::Snowflake(sf) if matches!(sf.auth_type, SnowflakeAuthType::BrowserAuth { .. })
        );

        // Spawn task to listen for SSO URL
        if is_snowflake_browser {
            let tx_clone = tx.clone();
            let db_config_clone = db_config.clone();
            tokio::spawn(async move {
                if let Some(sso_url) = sso_rx.recv().await {
                    let timeout =
                        if let DatabaseType::Snowflake(sf) = &db_config_clone.database_type {
                            if let SnowflakeAuthType::BrowserAuth {
                                browser_timeout_secs,
                                ..
                            } = &sf.auth_type
                            {
                                Some(*browser_timeout_secs)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                    let _ = tx_clone
                        .send(ConnectionTestEvent::BrowserAuthRequired {
                            sso_url,
                            message: "Please complete authentication in your browser".to_string(),
                            timeout_secs: timeout,
                        })
                        .await;
                }
            });
        }

        // Create connector with SSO sender
        let connector = match Connector::from_db(
            db_config,
            &project_manager.config_manager,
            &project_manager.secrets_manager,
            None,
            None,
            None,
            if is_snowflake_browser {
                Some(sso_tx)
            } else {
                None
            },
        )
        .await
        {
            Ok(conn) => conn,
            Err(e) => {
                tracing::error!("Failed to create connector: {}", e);
                let _ = tx
                    .send(ConnectionTestEvent::Complete {
                        result: TestDatabaseConnectionResponse {
                            success: false,
                            message: "Failed to create connector".to_string(),
                            connection_time_ms: None,
                            error_details: Some(e.to_string()),
                        },
                    })
                    .await;
                return;
            }
        };

        if is_snowflake_browser {
            let _ = tx
                .send(ConnectionTestEvent::Progress {
                    message: "Waiting for authentication...".to_string(),
                })
                .await;
        }

        let _ = tx
            .send(ConnectionTestEvent::Progress {
                message: "Testing connection...".to_string(),
            })
            .await;

        // Run test query
        match connector.run_query("SELECT 1").await {
            Ok(_) => {
                let elapsed = start_time.elapsed().as_millis() as u64;
                let _ = tx
                    .send(ConnectionTestEvent::Complete {
                        result: TestDatabaseConnectionResponse {
                            success: true,
                            message: "Connection successful".to_string(),
                            connection_time_ms: Some(elapsed),
                            error_details: None,
                        },
                    })
                    .await;
            }
            Err(e) => {
                let elapsed = start_time.elapsed().as_millis() as u64;
                let _ = tx
                    .send(ConnectionTestEvent::Complete {
                        result: TestDatabaseConnectionResponse {
                            success: false,
                            message: "Connection failed".to_string(),
                            connection_time_ms: Some(elapsed),
                            error_details: Some(e.to_string()),
                        },
                    })
                    .await;
            }
        }
    });

    Ok(Sse::new(oxy::utils::create_sse_stream(rx)))
}
