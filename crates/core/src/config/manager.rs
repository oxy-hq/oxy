use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    agent::builders::fsm::config::AgenticConfig,
    config::constants::{DATABASE_SEMANTIC_PATH, GLOBAL_SEMANTIC_PATH},
    errors::OxyError,
};

use super::{
    model::{AgentConfig, AppConfig, Config, Database, Model, Workflow, WorkflowWithRawVariables},
    storage::{ConfigSource, ConfigStorage},
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

    pub fn resolve_model(&self, model_name: &str) -> Result<&Model, OxyError> {
        let model = self
            .config
            .models
            .iter()
            .find(|m| match m {
                Model::OpenAI { name, .. } => name,
                Model::Ollama { name, .. } => name,
                Model::Google { name, .. } => name,
                Model::Anthropic { name, .. } => name,
            } == model_name)
            .ok_or_else(|| {
                OxyError::ConfigurationError(format!("Model '{model_name}' not found in config"))
            })?;
        Ok(model)
    }

    pub fn default_model(&self) -> Option<&String> {
        self.config.models.first().map(|m| match m {
            Model::OpenAI { name, .. } => name,
            Model::Ollama { name, .. } => name,
            Model::Google { name, .. } => name,
            Model::Anthropic { name, .. } => name,
        })
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

    pub async fn resolve_agent<P: AsRef<Path>>(
        &self,
        agent_name: P,
    ) -> Result<AgentConfig, OxyError> {
        self.storage.load_agent_config(agent_name).await
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

    pub async fn list_agents(&self) -> Result<Vec<PathBuf>, OxyError> {
        let agents = self.storage.list_agents().await?;
        tracing::info!("Agents: {:?}", agents);
        tracing::debug!("Builder: {:?}", self.config.builder_agent);
        if let Some(ref builder_agent) = self.config.builder_agent {
            // hide the builder agent from the list
            let builder_agent_full_path =
                self.storage.fs_link(builder_agent).await.map_err(|_| {
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
    pub async fn list_workflows(&self) -> Result<Vec<PathBuf>, OxyError> {
        self.storage.list_workflows().await
    }

    pub async fn resolve_app<P: AsRef<Path>>(&self, app_path: P) -> Result<AppConfig, OxyError> {
        self.storage.load_app_config(app_path).await
    }

    pub async fn get_build_agent(&self) -> Result<AgentConfig, OxyError> {
        if let Some(ref agent) = self.config.builder_agent {
            return self.resolve_agent(agent).await;
        } else {
            Err(OxyError::ConfigurationError(
                "No builder agent specified in config".to_string(),
            ))
        }
    }

    pub async fn get_builder_agent_path(&self) -> Result<PathBuf, OxyError> {
        if let Some(ref agent) = self.config.builder_agent {
            Ok(agent.to_owned())
        } else {
            Err(OxyError::ConfigurationError(
                "No builder agent specified in config".to_string(),
            ))
        }
    }

    pub fn get_config(&self) -> &Config {
        &self.config
    }

    pub fn get_model_key_var(&self, model: &Model) -> Option<String> {
        match model {
            Model::OpenAI { key_var, .. } => Some(key_var.clone()),
            Model::Google { key_var, .. } => Some(key_var.clone()),
            Model::Anthropic { key_var, .. } => Some(key_var.clone()),
            Model::Ollama { .. } => None, // Ollama doesn't use key_var
        }
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

    /// Gets the project path from the configuration
    pub fn project_path(&self) -> &std::path::Path {
        &self.config.project_path
    }

    /// Gets the semantics directory path (project_path/semantics)
    pub fn semantics_path(&self) -> PathBuf {
        self.config.project_path.join("semantics")
    }

    pub fn global_semantic_path(&self) -> PathBuf {
        self.config.project_path.join(GLOBAL_SEMANTIC_PATH)
    }

    pub fn database_semantic_path(&self) -> PathBuf {
        self.config.project_path.join(DATABASE_SEMANTIC_PATH)
    }

    pub fn globals_path(&self) -> PathBuf {
        self.config.project_path.join("globals")
    }

    pub fn get_globals_registry(&self) -> oxy_globals::GlobalRegistry {
        let globals_path = self.globals_path();
        if !globals_path.exists() {
            std::fs::create_dir_all(&globals_path).unwrap();
        }
        oxy_globals::GlobalRegistry::new(globals_path)
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
}
