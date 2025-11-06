use crate::{
    config::model::{Database, DatabaseType, Snowflake},
    errors::OxyError,
    service::{
        project::models::{WarehouseConfig, WarehousesFormData},
        secret_manager::{CreateSecretParams, SecretManagerService},
    },
};
use axum::http::StatusCode;
use sea_orm::DatabaseConnection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{error, warn};
use uuid::Uuid;

pub struct DatabaseConfigBuilder;

impl DatabaseConfigBuilder {
    pub async fn build_database_configs(
        project_id: Uuid,
        user_id: Uuid,
        warehouses_form: &WarehousesFormData,
        db: &DatabaseConnection,
        repo_path: &Path,
    ) -> std::result::Result<Vec<Database>, StatusCode> {
        let mut config_databases = Vec::new();

        for warehouse in &warehouses_form.warehouses {
            let db_name = warehouse
                .name
                .clone()
                .unwrap_or_else(|| "default-db".to_string());

            let database = match warehouse.r#type.as_str() {
                "postgres" => {
                    Self::build_postgres_config(project_id, user_id, db_name, warehouse, db).await?
                }
                "redshift" => {
                    Self::build_redshift_config(project_id, user_id, db_name, warehouse, db).await?
                }
                "mysql" => {
                    Self::build_mysql_config(project_id, user_id, db_name, warehouse, db).await?
                }
                "clickhouse" => {
                    Self::build_clickhouse_config(project_id, user_id, db_name, warehouse, db)
                        .await?
                }
                "bigquery" => Self::build_bigquery_config(db_name, warehouse, repo_path).await?,
                "duckdb" => Self::build_duckdb_config(db_name, warehouse),
                "snowflake" => {
                    Self::build_snowflake_config(project_id, user_id, db_name, warehouse, db)
                        .await?
                }
                _ => {
                    warn!("Unsupported database type: {}", warehouse.r#type);
                    continue;
                }
            };

            config_databases.push(database);
        }
        Ok(config_databases)
    }

    async fn build_postgres_config(
        project_id: Uuid,
        user_id: Uuid,
        db_name: String,
        warehouse: &WarehouseConfig,
        db: &DatabaseConnection,
    ) -> std::result::Result<Database, StatusCode> {
        let postgres_config = warehouse.get_postgres_config();

        let db_var_name = db_name.to_uppercase() + "_PASSWORD";

        if let Some(password) = &postgres_config.password {
            Self::create_secret(
                project_id,
                user_id,
                db_var_name.clone(),
                password.clone(),
                db,
            )
            .await
            .map_err(|e| {
                error!("Failed to create PostgreSQL password secret: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }

        Ok(Database {
            name: db_name,
            database_type: DatabaseType::Postgres(crate::config::model::Postgres {
                host: postgres_config.host,
                host_var: None,
                port: postgres_config.port,
                port_var: None,
                user: postgres_config.user,
                user_var: None,
                password: None,
                password_var: Some(db_var_name),
                database: postgres_config.database,
                database_var: None,
            }),
        })
    }

    async fn build_redshift_config(
        project_id: Uuid,
        user_id: Uuid,
        db_name: String,
        warehouse: &WarehouseConfig,
        db: &DatabaseConnection,
    ) -> std::result::Result<Database, StatusCode> {
        let redshift_config = warehouse.get_redshift_config();

        let db_var_name = db_name.to_uppercase() + "_PASSWORD";

        if let Some(password) = &redshift_config.password {
            Self::create_secret(
                project_id,
                user_id,
                db_var_name.clone(),
                password.clone(),
                db,
            )
            .await
            .map_err(|e| {
                error!("Failed to create Redshift password secret: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }

        Ok(Database {
            name: db_name,
            database_type: DatabaseType::Redshift(crate::config::model::Redshift {
                host: redshift_config.host,
                host_var: None,
                port: redshift_config.port,
                port_var: None,
                user: redshift_config.user,
                user_var: None,
                password: None,
                password_var: Some(db_var_name),
                database: redshift_config.database,
                database_var: None,
            }),
        })
    }

    async fn build_mysql_config(
        project_id: Uuid,
        user_id: Uuid,
        db_name: String,
        warehouse: &WarehouseConfig,
        db: &DatabaseConnection,
    ) -> std::result::Result<Database, StatusCode> {
        let mysql_config = warehouse.get_mysql_config();

        let db_var_name = db_name.to_uppercase() + "_PASSWORD";

        if let Some(password) = &mysql_config.password {
            Self::create_secret(
                project_id,
                user_id,
                db_var_name.clone(),
                password.clone(),
                db,
            )
            .await
            .map_err(|e| {
                error!("Failed to create MySQL password secret: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }

        Ok(Database {
            name: db_name,
            database_type: DatabaseType::Mysql(crate::config::model::Mysql {
                host: mysql_config.host,
                host_var: None,
                port: mysql_config.port,
                port_var: None,
                user: mysql_config.user,
                user_var: None,
                password: None,
                password_var: Some(db_var_name),
                database: mysql_config.database,
                database_var: None,
            }),
        })
    }

    async fn build_clickhouse_config(
        project_id: Uuid,
        user_id: Uuid,
        db_name: String,
        warehouse: &WarehouseConfig,
        db: &DatabaseConnection,
    ) -> std::result::Result<Database, StatusCode> {
        let clickhouse_config = warehouse.get_clickhouse_config();

        let db_var_name = db_name.to_uppercase() + "_PASSWORD";

        if let Some(password) = &clickhouse_config.password {
            Self::create_secret(
                project_id,
                user_id,
                db_var_name.clone(),
                password.clone(),
                db,
            )
            .await
            .map_err(|e| {
                error!("Failed to create ClickHouse password secret: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }

        Ok(Database {
            name: db_name,
            database_type: DatabaseType::ClickHouse(crate::config::model::ClickHouse {
                host: Some(
                    clickhouse_config
                        .host
                        .unwrap_or_else(|| "localhost".to_string()),
                ),
                host_var: None,
                user: Some(
                    clickhouse_config
                        .user
                        .unwrap_or_else(|| "default".to_string()),
                ),
                user_var: None,
                password: None,
                password_var: Some(db_var_name),
                database: Some(
                    clickhouse_config
                        .database
                        .unwrap_or_else(|| "default".to_string()),
                ),
                database_var: None,
                schemas: HashMap::new(),
                role: None,
                settings_prefix: None,
                filters: HashMap::new(),
            }),
        })
    }

    async fn build_bigquery_config(
        db_name: String,
        warehouse: &WarehouseConfig,
        repo_path: &Path,
    ) -> std::result::Result<Database, StatusCode> {
        let bigquery_config = warehouse.get_bigquery_config();

        let key_content = match &bigquery_config.key {
            Some(key) => key,
            None => {
                error!("BigQuery key not provided for database '{}'", db_name);
                return Err(StatusCode::BAD_REQUEST);
            }
        };

        let key_filename = format!("{}.key", db_name);
        let key_path = PathBuf::from(&key_filename);

        if let Err(e) = std::fs::write(repo_path.join(&key_path), key_content) {
            error!(
                "Failed to write BigQuery key file '{}': {}",
                key_filename, e
            );
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }

        Ok(Database {
            name: db_name,
            database_type: DatabaseType::Bigquery(crate::config::model::BigQuery {
                key_path: Some(key_path),
                key_path_var: None,
                dataset: bigquery_config.dataset,
                datasets: HashMap::new(),
                dry_run_limit: bigquery_config.dry_run_limit,
            }),
        })
    }

    fn build_duckdb_config(db_name: String, warehouse: &WarehouseConfig) -> Database {
        let duckdb_config = warehouse.get_duckdb_config();

        Database {
            name: db_name,
            database_type: DatabaseType::DuckDB(crate::config::model::DuckDB {
                file_search_path: duckdb_config
                    .file_search_path
                    .unwrap_or_else(|| "data".to_string()),
            }),
        }
    }

    async fn build_snowflake_config(
        project_id: Uuid,
        user_id: Uuid,
        db_name: String,
        warehouse: &WarehouseConfig,
        db: &DatabaseConnection,
    ) -> std::result::Result<Database, StatusCode> {
        let snowflake_config = warehouse.get_snowflake_config();

        let db_var_name = db_name.to_uppercase() + "_PASSWORD";

        if let Some(password) = &snowflake_config.password {
            Self::create_secret(
                project_id,
                user_id,
                db_var_name.clone(),
                password.clone(),
                db,
            )
            .await
            .map_err(|e| {
                error!("Failed to create Snowflake password secret: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }

        let private_key_path = snowflake_config
            .private_key_path
            .as_ref()
            .map(PathBuf::from);

        Ok(Database {
            name: db_name,
            database_type: DatabaseType::Snowflake(Snowflake {
                account: snowflake_config.account.unwrap_or_default(),
                username: snowflake_config.username.unwrap_or_default(),
                password: None,
                password_var: Some(db_var_name),
                warehouse: snowflake_config.warehouse.unwrap_or_default(),
                database: snowflake_config.database.unwrap_or_default(),
                schema: snowflake_config.schema,
                role: snowflake_config.role,
                private_key_path,
                datasets: HashMap::new(),
                filters: HashMap::new(),
            }),
        })
    }

    async fn create_secret(
        project_id: Uuid,
        user_id: Uuid,
        key: String,
        value: String,
        db: &DatabaseConnection,
    ) -> Result<(), OxyError> {
        let secret_manager = SecretManagerService::new(project_id);
        let create_params = CreateSecretParams {
            name: key,
            value,
            description: None,
            created_by: user_id,
        };

        secret_manager.create_secret(db, create_params).await?;
        Ok(())
    }
}
