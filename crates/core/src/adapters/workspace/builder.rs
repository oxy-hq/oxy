use std::path::Path;
use std::sync::Arc;

use crate::{
    adapters::{runs::RunsManager, secrets::SecretsManager, workspace::manager::WorkspaceManager},
    config::{ConfigBuilder, ConfigManager},
    intent::{IntentClassifier, IntentConfig},
    storage::SharedChartImageRenderer,
};
use oxy_shared::errors::OxyError;

#[derive(Default)]
pub struct WorkspaceBuilder {
    workspace_id: Option<uuid::Uuid>,
    config_manager: Option<ConfigManager>,
    secrets_manager: Option<SecretsManager>,
    runs_manager: Option<RunsManager>,
    intent_classifier: Option<Arc<IntentClassifier>>,
    chart_image_renderer: Option<SharedChartImageRenderer>,
}

impl WorkspaceBuilder {
    pub fn new(workspace_id: uuid::Uuid) -> Self {
        Self {
            workspace_id: Some(workspace_id),
            config_manager: None,
            secrets_manager: None,
            runs_manager: None,
            intent_classifier: None,
            chart_image_renderer: None,
        }
    }

    pub async fn with_workspace_path<P: AsRef<Path>>(
        mut self,
        workspace_path: P,
    ) -> Result<Self, OxyError> {
        self.config_manager = Some(
            ConfigBuilder::new()
                .with_workspace_path(workspace_path)?
                .build()
                .await?,
        );
        Ok(self)
    }

    pub async fn with_workspace_path_and_fallback_config<P: AsRef<Path>>(
        mut self,
        workspace_path: P,
    ) -> Result<Self, OxyError> {
        self.config_manager = Some(
            ConfigBuilder::new()
                .with_workspace_path(workspace_path)?
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

    /// Inject a chart image renderer (typically a headless-browser backed
    /// implementation from the CLI crate). When unset, S3 chart image
    /// publishing is disabled even if `storage` is configured.
    pub fn with_chart_image_renderer(mut self, renderer: SharedChartImageRenderer) -> Self {
        self.chart_image_renderer = Some(renderer);
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

    pub async fn build(self) -> Result<WorkspaceManager, OxyError> {
        let config_manager = self.config_manager.ok_or(OxyError::RuntimeError(
            "Config source is required".to_string(),
        ))?;

        let secret_manager = self
            .secrets_manager
            .unwrap_or(SecretsManager::from_environment().unwrap());

        let workspace_id = self.workspace_id.ok_or(OxyError::RuntimeError(
            "Workspace ID is required".to_string(),
        ))?;

        // Compute the chart image publisher eagerly so S3 client construction
        // and AWS credential resolution happen exactly once per workspace.
        let chart_image_publisher = WorkspaceManager::build_chart_image_publisher(
            self.chart_image_renderer,
            &config_manager,
        )
        .await?;

        Ok(WorkspaceManager::new(
            workspace_id,
            config_manager,
            secret_manager,
            self.runs_manager,
            self.intent_classifier,
            chart_image_publisher,
        ))
    }
}
