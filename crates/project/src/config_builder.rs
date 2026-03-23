use crate::database_config::DatabaseConfigBuilder;
use crate::model_config::ModelConfigBuilder;
use crate::models::{ModelsFormData, WarehousesFormData};
use axum::http::StatusCode;
use oxy::config::model::Config;
use sea_orm::DatabaseTransaction;
use std::fs;
use std::path::Path;
use tracing::error;
use uuid::Uuid;

pub struct ConfigBuilder;

impl ConfigBuilder {
    pub async fn create_project_config(
        project_id: Uuid,
        user_id: Uuid,
        warehouses: &WarehousesFormData,
        models: &ModelsFormData,
        repo_path: &Path,
        txn: &DatabaseTransaction,
    ) -> std::result::Result<(), StatusCode> {
        let config_models =
            ModelConfigBuilder::build_model_configs(project_id, user_id, models, txn).await?;

        let config_databases = DatabaseConfigBuilder::build_database_configs(
            project_id, user_id, warehouses, txn, repo_path,
        )
        .await?;

        let config = Config {
            defaults: None,
            models: config_models,
            databases: config_databases,
            builder_agent: None,
            project_path: repo_path.to_path_buf(),
            integrations: Vec::new(),
            slack: None,
            mcp: None,
            a2a: None,
        };

        Self::write_config_file(&config, repo_path)
    }

    fn write_config_file(config: &Config, repo_path: &Path) -> std::result::Result<(), StatusCode> {
        let config_path = repo_path.join("config.yml");
        let config_yaml = serde_yaml::to_string(config).map_err(|e| {
            error!("Failed to serialize config YAML: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        fs::write(&config_path, config_yaml).map_err(|e| {
            error!("Failed to write config.yml: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        Ok(())
    }
}
