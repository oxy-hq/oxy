use std::collections::HashSet;
use std::sync::Arc;

use uuid::Uuid;

use crate::{
    adapters::{runs::RunsManager, secrets::SecretsManager},
    config::ConfigManager,
    intent::IntentClassifier,
};
use oxy_shared::errors::OxyError;

#[derive(Debug, Clone)]
pub struct ProjectManager {
    pub project_id: Uuid,
    pub config_manager: ConfigManager,
    pub secrets_manager: SecretsManager,
    pub runs_manager: Option<RunsManager>,
    pub intent_classifier: Option<Arc<IntentClassifier>>,
}

impl ProjectManager {
    pub(super) fn new(
        project_id: Uuid,
        config_manager: ConfigManager,
        secrets_manager: SecretsManager,
        runs_manager: Option<RunsManager>,
        intent_classifier: Option<Arc<IntentClassifier>>,
    ) -> Self {
        Self {
            project_id,
            config_manager,
            secrets_manager,
            runs_manager,
            intent_classifier,
        }
    }

    pub async fn get_required_secrets(&self) -> Result<Option<Vec<String>>, OxyError> {
        let mut secrets_to_check: HashSet<String> = HashSet::new();

        let config_manager = &self.config_manager;

        let config = config_manager.get_config();

        for model in &config.models {
            if let Some(key_var) = config_manager.get_model_key_var(model) {
                let secret = self.secrets_manager.resolve_secret(&key_var).await?;
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
        for database in &config.databases {
            if let Some(password_var) = config_manager.get_database_password_var(database) {
                tracing::info!("Checking database password variable: {}", password_var);
                // Only add to secrets_to_check if it's not already resolvable
                if self
                    .secrets_manager
                    .resolve_secret(&password_var)
                    .await?
                    .is_none()
                {
                    secrets_to_check.insert(password_var);
                }
            }
        }

        if secrets_to_check.is_empty() {
            Ok(None)
        } else {
            Ok(Some(secrets_to_check.into_iter().collect()))
        }
    }
}
