use std::{
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use crate::config::constants::DATABASE_SEMANTIC_PATH;
use oxy_shared::errors::OxyError;

use super::{
    model::{
        AppConfig, BuilderAgentConfig, Config, Database, Model, Workflow, WorkflowWithRawVariables,
    },
    storage::{ConfigSource, ConfigStorage},
    test_config::TestFileConfig,
};

#[derive(Debug, Clone)]
pub struct ConfigManager {
    storage: Arc<ConfigSource>,
    config: Arc<Config>,
    /// Runtime-registered databases (e.g. modeling outputs). Checked before static config.
    runtime_databases: Arc<RwLock<Vec<Database>>>,
}

impl ConfigManager {
    pub(super) fn new(storage: ConfigSource, config: Config) -> Self {
        Self {
            storage: Arc::new(storage),
            config: Arc::new(config),
            runtime_databases: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn models(&self) -> &[Model] {
        &self.config.models
    }

    pub fn resolve_model(&self, model_name: &str) -> Result<&Model, OxyError> {
        let model = self
            .config
            .models
            .iter()
            .find(|m| m.name() == model_name)
            .ok_or_else(|| {
                OxyError::ConfigurationError(format!("Model '{model_name}' not found in config"))
            })?;
        Ok(model)
    }

    pub fn default_model(&self) -> Option<&str> {
        self.config.models.first().map(|m| m.name())
    }

    /// Look up a database by name. Checks runtime-registered databases first,
    /// then falls back to static config. Returns an owned clone.
    pub fn resolve_database(&self, database_name: &str) -> Result<Database, OxyError> {
        // Check runtime overlay first (e.g. modeling outputs registered after a run)
        if let Ok(rt) = self.runtime_databases.read()
            && let Some(db) = rt.iter().find(|d| d.name == database_name)
        {
            return Ok(db.clone());
        }
        // Fall back to static config
        self.config
            .databases
            .iter()
            .find(|w| w.name == database_name)
            .cloned()
            .ok_or_else(|| {
                OxyError::ConfigurationError(format!(
                    "Database '{database_name}' not found in config"
                ))
            })
    }

    /// Register a database at runtime (e.g. a modeling project output directory).
    /// Replaces any existing runtime entry with the same name; static config entries are untouched.
    pub fn add_runtime_database(&self, db: Database) {
        if let Ok(mut rt) = self.runtime_databases.write() {
            rt.retain(|d| d.name != db.name);
            rt.push(db);
        }
    }

    pub fn default_database_ref(&self) -> Option<&String> {
        self.config.defaults.as_ref().map(|d| d.database.as_ref())?
    }

    /// Returns the configured protected branches, if any.
    pub fn protected_branches(&self) -> Option<&[String]> {
        self.config.protected_branches.as_deref()
    }

    /// Returns the configured fork-point branch for auto-created worktrees, if any.
    pub fn base_branch(&self) -> Option<&str> {
        self.config.base_branch.as_deref()
    }

    /// Returns the configured project timezone (IANA name), if any.
    pub fn timezone(&self) -> Option<&str> {
        self.config.timezone.as_deref()
    }

    pub async fn resolve_file<P: AsRef<Path>>(&self, file_ref: P) -> Result<String, OxyError> {
        self.storage.fs_link(file_ref).await
    }

    pub async fn resolve_glob(&self, paths: &Vec<String>) -> Result<Vec<String>, OxyError> {
        let mut expanded_paths = Vec::new();
        for path in paths {
            expanded_paths.extend(self.storage.glob(path).await?);
        }
        Ok(expanded_paths)
    }

    pub async fn resolve_workflow<P: AsRef<Path>>(
        &self,
        workflow_name: P,
    ) -> Result<Workflow, OxyError> {
        self.storage.load_workflow_config(workflow_name).await
    }

    pub async fn resolve_workflow_with_raw_variables<P: AsRef<Path>>(
        &self,
        workflow_name: P,
    ) -> Result<WorkflowWithRawVariables, OxyError> {
        self.storage
            .load_workflow_config_with_raw_variables(workflow_name)
            .await
    }

    /// Returns all databases: static config entries plus any runtime-registered ones.
    /// Runtime entries with the same name as a static entry take precedence (appear first).
    pub fn list_databases(&self) -> Vec<Database> {
        let runtime: Vec<Database> = self
            .runtime_databases
            .read()
            .map(|rt| rt.clone())
            .unwrap_or_default();
        let runtime_names: std::collections::HashSet<String> =
            runtime.iter().map(|d| d.name.clone()).collect();
        let mut result = runtime;
        result.extend(
            self.config
                .databases
                .iter()
                .filter(|d| !runtime_names.contains(&d.name))
                .cloned(),
        );
        result
    }

    pub fn list_looker_integrations(&self) -> Vec<&super::model::Integration> {
        self.config
            .integrations
            .iter()
            .filter(|i| matches!(i.integration_type, super::model::IntegrationType::Looker(_)))
            .collect()
    }

    pub async fn list_apps(&self) -> Result<Vec<PathBuf>, OxyError> {
        self.storage.list_apps().await
    }
    pub async fn list_analytics_agents(&self) -> Result<Vec<PathBuf>, OxyError> {
        self.storage.list_analytics_agents().await
    }
    pub async fn list_workflows(&self) -> Result<Vec<PathBuf>, OxyError> {
        self.storage.list_workflows().await
    }
    pub async fn list_pipelines(&self) -> Result<Vec<PathBuf>, OxyError> {
        self.storage.list_pipelines().await
    }

    pub async fn resolve_app<P: AsRef<Path>>(&self, app_path: P) -> Result<AppConfig, OxyError> {
        self.storage.load_app_config(app_path).await
    }

    pub async fn resolve_test<P: AsRef<Path>>(
        &self,
        test_ref: P,
    ) -> Result<TestFileConfig, OxyError> {
        self.storage.load_test_config(test_ref).await
    }

    pub async fn list_tests(&self) -> Result<Vec<PathBuf>, OxyError> {
        self.storage.list_tests().await
    }

    pub async fn get_builder_agent_path(&self) -> Result<PathBuf, OxyError> {
        // Builder is always built-in now; no on-disk path exists.
        Err(OxyError::ConfigurationError(
            "Built-in builder agent does not have a file path".to_string(),
        ))
    }

    /// Returns the full builder agent config, if any.
    pub fn get_builder_config(&self) -> Option<&BuilderAgentConfig> {
        self.config.builder_agent.as_ref()
    }

    /// Returns true when the builder is configured as a built-in copilot
    /// (i.e. `builder_agent: { model: "..." }`).
    pub fn is_builder_builtin(&self) -> bool {
        matches!(
            self.config.builder_agent,
            Some(BuilderAgentConfig::Builtin { .. })
        )
    }

    pub fn get_config(&self) -> &Config {
        &self.config
    }

    pub fn get_model_key_var(&self, model: &Model) -> Option<String> {
        model.key_var().map(|s| s.to_string())
    }

    pub fn get_database_password_var(&self, database: &Database) -> Option<String> {
        match &database.database_type {
            crate::config::model::DatabaseType::Postgres(postgres) => postgres.password_var.clone(),
            crate::config::model::DatabaseType::Airhouse(airhouse) => airhouse.password_var.clone(),
            crate::config::model::DatabaseType::Mysql(mysql) => mysql.password_var.clone(),
            crate::config::model::DatabaseType::Snowflake(snowflake) => {
                snowflake.auth_type.get_password_var().cloned()
            }
            crate::config::model::DatabaseType::ClickHouse(clickhouse) => {
                clickhouse.password_var.clone()
            }
            crate::config::model::DatabaseType::Redshift(redshift) => redshift.password_var.clone(),
            _ => None, // Other database types might not have password_var
        }
    }

    pub async fn resolve_state_dir(&self) -> Result<PathBuf, OxyError> {
        self.storage.resolve_state_dir().await
    }

    pub async fn get_charts_dir(&self) -> Result<PathBuf, OxyError> {
        self.storage.get_charts_dir().await
    }

    pub async fn get_exported_chart_dir(&self) -> Result<PathBuf, OxyError> {
        self.storage.get_exported_chart_dir().await
    }

    pub async fn get_results_dir(&self) -> Result<PathBuf, OxyError> {
        self.storage.get_results_dir().await
    }

    pub async fn get_app_results_dir(&self) -> Result<PathBuf, OxyError> {
        self.storage.get_app_results_dir().await
    }

    /// Gets the workspace path from the configuration
    pub fn workspace_path(&self) -> &std::path::Path {
        &self.config.workspace_path
    }

    /// Gets the semantics directory path (workspace_path/semantics).
    /// Used for writing semantic files.
    pub fn semantics_path(&self) -> PathBuf {
        self.config.workspace_path.join("semantics")
    }

    /// Gets the base path for scanning semantic layer files.
    /// Scans the entire project so .view.yml/.topic.yml files can live anywhere.
    pub fn semantics_scan_path(&self) -> PathBuf {
        self.config.workspace_path.clone()
    }

    pub fn database_semantic_path(&self) -> PathBuf {
        self.config.workspace_path.join(DATABASE_SEMANTIC_PATH)
    }

    pub fn get_integration_by_name(
        &self,
        integration_name: &str,
    ) -> Option<&crate::config::model::Integration> {
        self.config
            .integrations
            .iter()
            .find(|i| i.name == integration_name)
    }

    /// Updates the databases in the config and writes to config.yml
    pub async fn update_databases(&self, new_databases: Vec<Database>) -> Result<(), OxyError> {
        // Create a new config with updated databases
        let mut updated_config = (*self.config).clone();
        updated_config.databases = new_databases;

        // Write the updated config
        self.storage.write_config(&updated_config).await?;
        Ok(())
    }

    /// Adds a database to the existing configuration
    pub async fn add_database(&self, database: Database) -> Result<(), OxyError> {
        let mut updated_config = (*self.config).clone();

        // Check if database with same name exists
        if updated_config
            .databases
            .iter()
            .any(|db| db.name == database.name)
        {
            return Err(OxyError::ConfigurationError(format!(
                "Database with name '{}' already exists",
                database.name
            )));
        }

        updated_config.databases.push(database);
        self.storage.write_config(&updated_config).await?;
        Ok(())
    }

    /// Adds multiple databases to the existing configuration
    pub async fn add_databases(&self, databases: Vec<Database>) -> Result<(), OxyError> {
        let mut updated_config = (*self.config).clone();

        // Check for duplicates
        for database in &databases {
            if updated_config
                .databases
                .iter()
                .any(|db| db.name == database.name)
            {
                return Err(OxyError::ConfigurationError(format!(
                    "Database with name '{}' already exists",
                    database.name
                )));
            }
        }

        updated_config.databases.extend(databases);
        self.storage.write_config(&updated_config).await?;
        Ok(())
    }

    /// Removes a database from the configuration by name
    pub async fn remove_database(&self, database_name: &str) -> Result<(), OxyError> {
        let mut updated_config = (*self.config).clone();

        // Find and remove the database
        let initial_len = updated_config.databases.len();
        updated_config
            .databases
            .retain(|db| db.name != database_name);

        if updated_config.databases.len() == initial_len {
            return Err(OxyError::ConfigurationError(format!(
                "Database with name '{}' not found",
                database_name
            )));
        }

        self.storage.write_config(&updated_config).await?;
        Ok(())
    }

    /// Removes a model entry from the configuration by name.
    pub async fn remove_model(&self, model_name: &str) -> Result<(), OxyError> {
        let mut updated_config = (*self.config).clone();

        let initial_len = updated_config.models.len();
        updated_config.models.retain(|m| m.name() != model_name);

        if updated_config.models.len() == initial_len {
            return Err(OxyError::ConfigurationError(format!(
                "Model with name '{}' not found",
                model_name
            )));
        }

        self.storage.write_config(&updated_config).await?;
        Ok(())
    }

    /// Returns the current data repos
    pub fn list_repositories(&self) -> &[crate::config::model::Repository] {
        &self.config.repositories
    }

    /// Adds a repository to the configuration
    pub async fn add_repository(
        &self,
        repo: crate::config::model::Repository,
    ) -> Result<(), OxyError> {
        let mut updated_config = (*self.config).clone();

        if updated_config
            .repositories
            .iter()
            .any(|r| r.name == repo.name)
        {
            return Err(OxyError::ConfigurationError(format!(
                "Repository with name '{}' already exists",
                repo.name
            )));
        }

        updated_config.repositories.push(repo);
        self.storage.write_config(&updated_config).await?;
        Ok(())
    }

    /// Removes a repository from the configuration by name
    pub async fn remove_repository(&self, name: &str) -> Result<(), OxyError> {
        let mut updated_config = (*self.config).clone();

        let initial_len = updated_config.repositories.len();
        updated_config.repositories.retain(|r| r.name != name);

        if updated_config.repositories.len() == initial_len {
            return Err(OxyError::ConfigurationError(format!(
                "Repository with name '{}' not found",
                name
            )));
        }

        self.storage.write_config(&updated_config).await?;
        Ok(())
    }

    /// Upserts an integration entry, matching on the variant kind (Toast,
    /// OpenWeatherMap, BestTime, Omni, Looker). If an integration of the
    /// same kind already exists, it is replaced — name is preserved from
    /// the incoming entry. The world-model "Apps" UI treats each kind as
    /// singleton-per-workspace; the entry-name distinction is reserved
    /// for cases (Omni/Looker) where multiple instances of the same kind
    /// can coexist.
    pub async fn upsert_integration(
        &self,
        integration: crate::config::model::Integration,
    ) -> Result<(), OxyError> {
        let mut updated_config = (*self.config).clone();
        let kind = integration_kind(&integration);

        if let Some(slot) = updated_config
            .integrations
            .iter_mut()
            .find(|i| integration_kind(i) == kind)
        {
            *slot = integration;
        } else {
            updated_config.integrations.push(integration);
        }
        self.storage.write_config(&updated_config).await?;
        Ok(())
    }

    /// Removes the integration entry matching a kind ("toast",
    /// "openweathermap", "besttime", "omni", "looker"). Returns
    /// `Ok(())` when nothing matched — idempotent.
    pub async fn remove_integration_by_kind(&self, kind: &str) -> Result<(), OxyError> {
        let mut updated_config = (*self.config).clone();
        let initial_len = updated_config.integrations.len();
        updated_config
            .integrations
            .retain(|i| integration_kind(i) != kind);
        if updated_config.integrations.len() == initial_len {
            return Ok(());
        }
        self.storage.write_config(&updated_config).await?;
        Ok(())
    }
}

fn integration_kind(integration: &crate::config::model::Integration) -> &'static str {
    use crate::config::model::IntegrationType;
    match &integration.integration_type {
        IntegrationType::Omni(_) => "omni",
        IntegrationType::Looker(_) => "looker",
        IntegrationType::Toast(_) => "toast",
        IntegrationType::OpenWeatherMap(_) => "openweathermap",
        IntegrationType::BestTime(_) => "besttime",
        IntegrationType::Unifi(_) => "unifi",
    }
}
