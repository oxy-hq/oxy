use std::collections::HashSet;
use std::sync::Arc;

use uuid::Uuid;

use crate::{
    adapters::{runs::RunsManager, secrets::SecretsManager},
    config::ConfigManager,
    intent::IntentClassifier,
    storage::{
        BlobStorageChartImagePublisher, SharedChartImagePublisher, SharedChartImageRenderer,
    },
};
use oxy_shared::errors::OxyError;

#[derive(Debug, Clone)]
pub struct WorkspaceManager {
    pub workspace_id: Uuid,
    pub config_manager: ConfigManager,
    pub secrets_manager: SecretsManager,
    pub runs_manager: Option<RunsManager>,
    pub intent_classifier: Option<Arc<IntentClassifier>>,
    /// Chart image publisher, pre-assembled at workspace build time when both
    /// a renderer is injected AND `storage` is configured. Resolving
    /// AWS credentials and constructing the S3 client is not free, so we do it
    /// once per workspace instead of on every call.
    chart_image_publisher: Option<SharedChartImagePublisher>,
}

impl WorkspaceManager {
    pub(super) fn new(
        workspace_id: Uuid,
        config_manager: ConfigManager,
        secrets_manager: SecretsManager,
        runs_manager: Option<RunsManager>,
        intent_classifier: Option<Arc<IntentClassifier>>,
        chart_image_publisher: Option<SharedChartImagePublisher>,
    ) -> Self {
        Self {
            workspace_id,
            config_manager,
            secrets_manager,
            runs_manager,
            intent_classifier,
            chart_image_publisher,
        }
    }

    /// Pre-assembled chart image publisher (see struct field doc). Returns
    /// `None` when either no renderer was injected at build time or no
    /// `storage` backend is configured — that is the default path in
    /// which charts live on disk as JSON only.
    pub fn chart_image_publisher(&self) -> Option<SharedChartImagePublisher> {
        self.chart_image_publisher.clone()
    }

    /// Helper for [`super::WorkspaceBuilder`] — assemble a publisher from a
    /// renderer and the storage config. Kept here so the assembly logic lives
    /// alongside the field it populates.
    pub(super) async fn build_chart_image_publisher(
        renderer: Option<SharedChartImageRenderer>,
        config_manager: &ConfigManager,
    ) -> Result<Option<SharedChartImagePublisher>, OxyError> {
        let Some(renderer) = renderer else {
            return Ok(None);
        };
        let Some(storage) = config_manager.chart_image_blob_storage().await? else {
            return Ok(None);
        };
        let publisher = BlobStorageChartImagePublisher::new(renderer, storage);
        Ok(Some(Arc::new(publisher) as SharedChartImagePublisher))
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
