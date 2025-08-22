use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{config::auth::Authentication, errors::OxyError};

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

    pub fn list_databases(&self) -> Result<&[Database], OxyError> {
        Ok(self.config.databases.as_slice())
    }

    pub fn get_authentication(&self) -> Option<Authentication> {
        self.config.authentication.clone()
    }

    pub async fn list_agents(&self) -> Result<Vec<PathBuf>, OxyError> {
        let agents = self.storage.list_agents().await?;
        tracing::debug!("Agents: {:?}", agents);
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

    pub async fn get_required_secrets(&self) -> Result<Option<Vec<String>>, OxyError> {
        let secret_resolver = crate::service::secret_resolver::SecretResolverService::new();
        let mut secrets_to_check: HashSet<String> = HashSet::new();

        // Check model configurations for key_var requirements
        for model in &self.config.models {
            if let Some(key_var) = self.get_model_key_var(model) {
                let secret = secret_resolver.resolve_secret(&key_var).await?;
                tracing::info!(
                    "Checking model key variable: {}, value: {:?}",
                    key_var,
                    secret.clone()
                );
                // Only add to secrets_to_check if it's not already resolvable
                if secret.is_none() {
                    secrets_to_check.insert(key_var);
                }
            }
        }

        // Check database configurations for password_var requirements
        for database in &self.config.databases {
            if let Some(password_var) = self.get_database_password_var(database) {
                tracing::info!("Checking database password variable: {}", password_var);
                // Only add to secrets_to_check if it's not already resolvable
                if secret_resolver
                    .resolve_secret(&password_var)
                    .await?
                    .is_none()
                {
                    secrets_to_check.insert(password_var);
                }
            }
        }

        // Check authentication configuration
        if let Some(auth) = &self.config.authentication {
            // Check basic auth SMTP password
            if let Some(basic_auth) = &auth.basic {
                // Only add to secrets_to_check if it's not already resolvable
                if secret_resolver
                    .resolve_secret(&basic_auth.smtp_password_var)
                    .await?
                    .is_none()
                {
                    secrets_to_check.insert(basic_auth.smtp_password_var.clone());
                }
            }

            // Check Google OAuth client secret
            if let Some(google_auth) = &auth.google {
                // Only add to secrets_to_check if it's not already resolvable
                if secret_resolver
                    .resolve_secret(&google_auth.client_secret_var)
                    .await?
                    .is_none()
                {
                    secrets_to_check.insert(google_auth.client_secret_var.clone());
                }
            }
        }

        if secrets_to_check.is_empty() {
            Ok(None)
        } else {
            Ok(Some(secrets_to_check.into_iter().collect()))
        }
    }

    fn get_model_key_var(&self, model: &Model) -> Option<String> {
        match model {
            Model::OpenAI { key_var, .. } => Some(key_var.clone()),
            Model::Google { key_var, .. } => Some(key_var.clone()),
            Model::Anthropic { key_var, .. } => Some(key_var.clone()),
            Model::Ollama { .. } => None, // Ollama doesn't use key_var
        }
    }

    fn get_database_password_var(&self, database: &Database) -> Option<String> {
        match &database.database_type {
            crate::config::model::DatabaseType::Postgres(postgres) => postgres.password_var.clone(),
            crate::config::model::DatabaseType::Mysql(mysql) => mysql.password_var.clone(),
            crate::config::model::DatabaseType::Snowflake(snowflake) => {
                snowflake.password_var.clone()
            }
            crate::config::model::DatabaseType::ClickHouse(clickhouse) => {
                clickhouse.password_var.clone()
            }
            crate::config::model::DatabaseType::Redshift(redshift) => redshift.password_var.clone(),
            _ => None, // Other database types might not have password_var
        }
    }
}
