use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    config::{agent_config::AgenticConfig, constants::DATABASE_SEMANTIC_PATH},
    observability::events,
};
use oxy_shared::errors::OxyError;

use super::{
    model::{
        AgentConfig, AppConfig, BuilderAgentConfig, Config, Database, Model, Workflow,
        WorkflowWithRawVariables,
    },
    storage::{ConfigSource, ConfigStorage},
    test_config::TestFileConfig,
};

#[derive(Debug, Clone)]
pub struct ConfigManager {
    storage: Arc<ConfigSource>,
    config: Arc<Config>,
}

impl ConfigManager {
    pub(super) fn new(storage: ConfigSource, config: Config) -> Self {
        Self {
            storage: Arc::new(storage),
            config: Arc::new(config),
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

    pub fn resolve_database(&self, database_name: &str) -> Result<&Database, OxyError> {
        let database = self
            .config
            .databases
            .iter()
            .find(|w| w.name == database_name)
            .ok_or_else(|| {
                OxyError::ConfigurationError(format!(
                    "Database '{database_name}' not found in config"
                ))
            })?;
        Ok(database)
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

    #[tracing::instrument(skip_all, err, fields(
        oxy.name = events::agent::load_agent_config::NAME,
        oxy.span_type = events::agent::load_agent_config::TYPE,
    ))]
    pub async fn resolve_agent<P: AsRef<Path>>(
        &self,
        agent_name: P,
    ) -> Result<AgentConfig, OxyError> {
        let agent_name_str = agent_name.as_ref().display().to_string();
        events::agent::load_agent_config::input(&agent_name_str);
        let output = self.storage.load_agent_config(agent_name).await?;
        events::agent::load_agent_config::output(&output);
        Ok(output)
    }

    pub async fn resolve_agentic_workflow<P: AsRef<Path>>(
        &self,
        agent_name: P,
    ) -> Result<AgenticConfig, OxyError> {
        self.storage.load_agentic_workflow_config(agent_name).await
    }

    pub fn list_databases(&self) -> &[Database] {
        self.config.databases.as_slice()
    }

    pub fn list_looker_integrations(&self) -> Vec<&super::model::Integration> {
        self.config
            .integrations
            .iter()
            .filter(|i| matches!(i.integration_type, super::model::IntegrationType::Looker(_)))
            .collect()
    }

    pub async fn list_agents(&self) -> Result<Vec<PathBuf>, OxyError> {
        let agents = self.storage.list_agents().await?;
        tracing::info!("Agents: {:?}", agents);
        tracing::debug!("Builder: {:?}", self.config.builder_agent);
        if let Some(BuilderAgentConfig::Path(ref path)) = self.config.builder_agent {
            // hide the legacy path-based builder agent from the list
            let builder_agent_full_path = self.storage.fs_link(path).await.map_err(|_| {
                OxyError::ConfigurationError("Failed to resolve agent path".to_string())
            })?;
            Ok(agents
                .iter()
                .filter(|agent| agent.display().to_string() != builder_agent_full_path)
                .cloned()
                .collect())
        } else {
            Ok(agents)
        }
    }

    pub async fn list_apps(&self) -> Result<Vec<PathBuf>, OxyError> {
        self.storage.list_apps().await
    }
    pub async fn list_agentic_workflows(&self) -> Result<Vec<PathBuf>, OxyError> {
        self.storage.list_agentic_workflows().await
    }
    pub async fn list_analytics_agents(&self) -> Result<Vec<PathBuf>, OxyError> {
        self.storage.list_analytics_agents().await
    }
    pub async fn list_workflows(&self) -> Result<Vec<PathBuf>, OxyError> {
        self.storage.list_workflows().await
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

    pub async fn get_build_agent(&self) -> Result<AgentConfig, OxyError> {
        match &self.config.builder_agent {
            Some(BuilderAgentConfig::Path(path)) => self.resolve_agent(path).await,
            Some(BuilderAgentConfig::Builtin { .. }) => Err(OxyError::ConfigurationError(
                "Built-in builder agent does not use an agent file. Use get_builder_config() instead.".to_string(),
            )),
            None => Err(OxyError::ConfigurationError(
                "No builder agent specified in config".to_string(),
            )),
        }
    }

    pub async fn get_builder_agent_path(&self) -> Result<PathBuf, OxyError> {
        match &self.config.builder_agent {
            Some(BuilderAgentConfig::Path(path)) => Ok(path.to_owned()),
            Some(BuilderAgentConfig::Builtin { .. }) => Err(OxyError::ConfigurationError(
                "Built-in builder agent does not have a file path".to_string(),
            )),
            None => Err(OxyError::ConfigurationError(
                "No builder agent specified in config".to_string(),
            )),
        }
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
}
