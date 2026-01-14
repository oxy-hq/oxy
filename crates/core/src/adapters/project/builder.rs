use std::path::Path;
use std::sync::Arc;

use crate::{
    adapters::{project::manager::ProjectManager, runs::RunsManager, secrets::SecretsManager},
    config::{ConfigBuilder, ConfigManager},
    errors::OxyError,
    intent::{IntentClassifier, IntentConfig},
};

pub struct ProjectBuilder {
    project_id: Option<uuid::Uuid>,
    config_manager: Option<ConfigManager>,
    secrets_manager: Option<SecretsManager>,
    runs_manager: Option<RunsManager>,
    intent_classifier: Option<Arc<IntentClassifier>>,
}

impl Default for ProjectBuilder {
    fn default() -> Self {
        Self {
            project_id: None,
            config_manager: None,
            secrets_manager: None,
            runs_manager: None,
            intent_classifier: None,
        }
    }
}

impl ProjectBuilder {
    pub fn new(project_id: uuid::Uuid) -> Self {
        Self {
            project_id: Some(project_id),
            config_manager: None,
            secrets_manager: None,
            runs_manager: None,
            intent_classifier: None,
        }
    }

    pub async fn with_project_path<P: AsRef<Path>>(
        mut self,
        project_path: P,
    ) -> Result<Self, OxyError> {
        self.config_manager = Some(
            ConfigBuilder::new()
                .with_project_path(project_path)?
                .build()
                .await?,
        );
        Ok(self)
    }

    pub async fn with_project_path_and_fallback_config<P: AsRef<Path>>(
        mut self,
        project_path: P,
    ) -> Result<Self, OxyError> {
        self.config_manager = Some(
            ConfigBuilder::new()
                .with_project_path(project_path)?
                .build_with_fallback_config()
                .await?,
        );
        Ok(self)
    }

    pub fn with_secrets_manager(mut self, secret_manager: SecretsManager) -> Self {
        self.secrets_manager = Some(secret_manager);
        self
    }

    pub fn with_runs_manager(mut self, runs_manager: RunsManager) -> Self {
        self.runs_manager = Some(runs_manager);
        self
    }

    /// Try to create an intent classifier from environment variables.
    /// If the required environment variables (like OPENAI_API_KEY) are not set,
    /// this will silently skip and return self without a classifier.
    pub async fn try_with_intent_classifier(mut self) -> Self {
        let config = IntentConfig::from_env();
        // Only try to create classifier if OpenAI API key is set
        if !config.openai_api_key.is_empty() {
            match IntentClassifier::new(config).await {
                Ok(classifier) => {
                    self.intent_classifier = Some(Arc::new(classifier));
                }
                Err(e) => {
                    tracing::warn!("Failed to create intent classifier: {}", e);
                }
            }
        }
        self
    }

    pub async fn build(self) -> Result<ProjectManager, OxyError> {
        let config_manager = self.config_manager.ok_or(OxyError::RuntimeError(
            "Config source is required".to_string(),
        ))?;

        let secret_manager = self
            .secrets_manager
            .unwrap_or(SecretsManager::from_environment().unwrap());

        let project_id = self
            .project_id
            .ok_or(OxyError::RuntimeError("Project ID is required".to_string()))?;

        Ok(ProjectManager::new(
            project_id,
            config_manager,
            secret_manager,
            self.runs_manager,
            self.intent_classifier,
        ))
    }
}
