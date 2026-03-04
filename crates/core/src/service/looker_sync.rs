use std::path::PathBuf;

use oxy_looker::models::ExploreMetadata;
use oxy_looker::{LookerApiClient, LookerError, MetadataMerger, MetadataStorage};

/// Result of a synchronization operation
///
/// Contains statistics about the sync operation including successful,
/// failed, and skipped explores with detailed error information.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncResult {
    /// The integration name that was synchronized
    pub integration: String,
    /// The model ID that was synchronized
    pub model: String,
    /// Total number of explores processed
    pub total_explores: usize,
    /// List of explores that were successfully synchronized
    pub successful: Vec<String>,
    /// List of explores that failed to synchronize with error details
    pub failed: Vec<(String, String)>,
    /// List of explores that were skipped
    pub skipped: Vec<String>,
}

impl SyncResult {
    /// Check if the sync operation was completely successful
    pub fn is_success(&self) -> bool {
        self.failed.is_empty()
    }

    /// Check if the sync operation was partially successful
    pub fn is_partial_success(&self) -> bool {
        !self.successful.is_empty() && !self.failed.is_empty()
    }

    /// Check if the sync operation completely failed
    pub fn is_failure(&self) -> bool {
        self.successful.is_empty() && !self.failed.is_empty()
    }

    /// Get the success rate as a percentage
    pub fn success_rate(&self) -> f64 {
        if self.total_explores == 0 {
            return 100.0;
        }
        (self.successful.len() as f64 / self.total_explores as f64) * 100.0
    }

    /// Get a summary string of the sync operation
    pub fn summary(&self) -> String {
        format!(
            "Synchronized {}/{} explores ({:.1}% success rate)",
            self.successful.len(),
            self.total_explores,
            self.success_rate()
        )
    }

    /// Get detailed error information for failed explores
    pub fn error_summary(&self) -> Option<String> {
        if self.failed.is_empty() {
            return None;
        }

        let errors: Vec<String> = self
            .failed
            .iter()
            .map(|(name, error)| format!("- {}: {}", name, error))
            .collect();

        Some(format!(
            "Failed to sync {} explores:\n{}",
            self.failed.len(),
            errors.join("\n")
        ))
    }
}

/// Service for synchronizing Looker explore metadata
///
/// The LookerSyncService handles fetching metadata from the Looker API and storing it locally
/// in the project's metadata directories. It supports both individual explore synchronization
/// and full model synchronization workflows.
#[derive(Debug)]
pub struct LookerSyncService {
    client: LookerApiClient,
    storage: MetadataStorage,
    merger: MetadataMerger,
    integration_name: String,
}

impl LookerSyncService {
    /// Create a new LookerSyncService instance
    ///
    /// # Arguments
    /// * `client` - Configured Looker API client for making requests
    /// * `state_dir` - Path to the state directory for base metadata storage
    /// * `project_dir` - Path to the project directory for overlay metadata
    /// * `integration_name` - Name of the integration for metadata storage organization
    pub fn new<P1, P2>(
        client: LookerApiClient,
        state_dir: P1,
        project_dir: P2,
        integration_name: String,
    ) -> Self
    where
        P1: Into<PathBuf>,
        P2: Into<PathBuf>,
    {
        let state_dir = state_dir.into();
        let project_dir = project_dir.into();

        let storage = MetadataStorage::new(&state_dir, &project_dir, integration_name.clone());

        let merger = MetadataMerger::new(&state_dir, &project_dir, integration_name.clone());

        Self {
            client,
            storage,
            merger,
            integration_name,
        }
    }

    /// Synchronize metadata for a specific explore
    ///
    /// This method fetches explore metadata from the Looker API and saves it to the
    /// local .looker directory. It ensures the directory structure exists before saving.
    ///
    /// # Arguments
    /// * `model` - The Looker model name to sync from
    /// * `explore` - The specific explore to synchronize
    ///
    /// # Returns
    /// * `Ok(())` if the explore was successfully synchronized
    /// * `Err(LookerError)` if the sync failed for any reason
    pub async fn sync_explore(&mut self, model: &str, explore: &str) -> Result<(), LookerError> {
        // Validate inputs
        if model.trim().is_empty() {
            return Err(LookerError::ConfigError {
                message: "Model name cannot be empty".to_string(),
            });
        }

        if explore.trim().is_empty() {
            return Err(LookerError::ConfigError {
                message: "Explore name cannot be empty".to_string(),
            });
        }

        // Ensure directory structure exists
        self.storage.ensure_directory_structure(model)?;

        // Fetch explore metadata from Looker API
        let explore_response =
            self.client
                .get_explore(model, explore)
                .await
                .map_err(|e| LookerError::SyncError {
                    message: format!("Failed to fetch explore {}/{}: {}", model, explore, e),
                })?;

        // Transform API response to ExploreMetadata
        let explore_metadata = transform_to_metadata(&explore_response, model);

        // Save the metadata to storage
        self.storage
            .save_base_metadata(model, explore, &explore_metadata)?;

        Ok(())
    }

    /// Synchronize all explores in a model
    ///
    /// This method fetches the list of explores for a model and synchronizes each one.
    /// It continues processing even if individual explores fail, collecting all results.
    ///
    /// # Arguments
    /// * `model` - The Looker model name to sync
    ///
    /// # Returns
    /// * `Ok(SyncResult)` containing sync statistics and any errors encountered
    /// * `Err(LookerError)` if the model listing fails
    pub async fn sync_model(&mut self, model: &str) -> Result<SyncResult, LookerError> {
        // Validate input
        if model.trim().is_empty() {
            return Err(LookerError::ConfigError {
                message: "Model name cannot be empty".to_string(),
            });
        }

        // Fetch all explores for the model
        let models = self.client.list_models().await?;

        let model_data =
            models
                .iter()
                .find(|m| m.name == model)
                .ok_or_else(|| LookerError::NotFoundError {
                    resource: format!("model '{}'", model),
                })?;

        let explore_names: Vec<String> = model_data
            .explores
            .iter()
            .filter(|e| !e.hidden.unwrap_or(false))
            .map(|e| e.name.clone())
            .collect();

        // Initialize sync result
        let mut sync_result = SyncResult {
            integration: self.integration_name.clone(),
            model: model.to_string(),
            total_explores: explore_names.len(),
            successful: Vec::new(),
            failed: Vec::new(),
            skipped: Vec::new(),
        };

        // Sync each explore
        for explore_name in explore_names {
            match self.sync_explore(model, &explore_name).await {
                Ok(()) => {
                    sync_result.successful.push(explore_name);
                }
                Err(error) => {
                    sync_result.failed.push((explore_name, error.to_string()));
                }
            }
        }

        Ok(sync_result)
    }

    /// Check if an explore has been synchronized (exists in base metadata)
    ///
    /// # Arguments
    /// * `model` - The model name to check
    /// * `explore` - The explore name to check
    ///
    /// # Returns
    /// * `true` if the explore has been synchronized and exists in base metadata
    /// * `false` if the explore has not been synchronized
    pub fn is_explore_synchronized(&self, model: &str, explore: &str) -> bool {
        self.storage.base_metadata_exists(model, explore)
    }

    /// List all synchronized explores for a model
    ///
    /// # Arguments
    /// * `model` - The model name to list explores for
    ///
    /// # Returns
    /// * `Ok(Vec<String>)` containing the list of synchronized explore names
    /// * `Err(LookerError)` if the listing operation failed
    pub fn list_synchronized_explores(&self, model: &str) -> Result<Vec<String>, LookerError> {
        self.storage.list_base_explores(model)
    }

    /// Get a reference to the underlying API client
    pub fn client(&self) -> &LookerApiClient {
        &self.client
    }

    /// Get a mutable reference to the underlying API client
    pub fn client_mut(&mut self) -> &mut LookerApiClient {
        &mut self.client
    }

    /// Get a reference to the metadata storage
    pub fn storage(&self) -> &MetadataStorage {
        &self.storage
    }

    /// Get a reference to the metadata merger
    pub fn merger(&self) -> &MetadataMerger {
        &self.merger
    }

    /// Get the integration name
    pub fn integration_name(&self) -> &str {
        &self.integration_name
    }
}

/// Transform LookML explore response to ExploreMetadata
fn transform_to_metadata(
    explore: &oxy_looker::models::LookmlModelExplore,
    model: &str,
) -> ExploreMetadata {
    use oxy_looker::models::{FieldMetadata, ViewMetadata};
    use std::collections::HashMap;

    // Group fields by view
    let mut views_map: HashMap<String, (Vec<FieldMetadata>, Vec<FieldMetadata>)> = HashMap::new();

    // Get fields from the explore (may be None)
    if let Some(ref fields) = explore.fields {
        // Process dimensions
        for field in &fields.dimensions {
            let view_name = field.view.clone().unwrap_or_else(|| explore.name.clone());
            let view_entry = views_map.entry(view_name).or_default();
            view_entry.0.push(FieldMetadata {
                name: field.name.clone(),
                label: field.label.clone(),
                description: field.description.clone(),
                field_type: "dimension".to_string(),
                data_type: field.type_.clone(),
                sql: field.sql.clone(),
                agent_hint: None,
                examples: None,
            });
        }

        // Process measures
        for field in &fields.measures {
            let view_name = field.view.clone().unwrap_or_else(|| explore.name.clone());
            let view_entry = views_map.entry(view_name).or_default();
            view_entry.1.push(FieldMetadata {
                name: field.name.clone(),
                label: field.label.clone(),
                description: field.description.clone(),
                field_type: "measure".to_string(),
                data_type: field.type_.clone(),
                sql: field.sql.clone(),
                agent_hint: None,
                examples: None,
            });
        }
    }

    // Convert to ViewMetadata
    let mut views: Vec<ViewMetadata> = views_map
        .into_iter()
        .map(|(view_name, (dimensions, measures))| ViewMetadata {
            name: view_name,
            dimensions,
            measures,
        })
        .collect();

    // Sort views by name for consistency
    views.sort_by(|a, b| a.name.cmp(&b.name));

    ExploreMetadata {
        model: model.to_string(),
        name: explore.name.clone(),
        base_view_name: Some(
            explore
                .view_name
                .clone()
                .unwrap_or_else(|| explore.name.clone()),
        ),
        label: explore.label.clone(),
        description: explore.description.clone(),
        views,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxy_looker::models::{ExploreFields, LookmlModelExplore, LookmlModelExploreField};

    #[test]
    fn test_sync_result_success() {
        let result = SyncResult {
            integration: "test".to_string(),
            model: "ecommerce".to_string(),
            total_explores: 3,
            successful: vec![
                "orders".to_string(),
                "users".to_string(),
                "products".to_string(),
            ],
            failed: vec![],
            skipped: vec![],
        };

        assert!(result.is_success());
        assert!(!result.is_partial_success());
        assert!(!result.is_failure());
        assert_eq!(result.success_rate(), 100.0);
    }

    #[test]
    fn test_sync_result_partial_success() {
        let result = SyncResult {
            integration: "test".to_string(),
            model: "ecommerce".to_string(),
            total_explores: 3,
            successful: vec!["orders".to_string(), "users".to_string()],
            failed: vec![("products".to_string(), "Failed to fetch".to_string())],
            skipped: vec![],
        };

        assert!(!result.is_success());
        assert!(result.is_partial_success());
        assert!(!result.is_failure());
        assert!((result.success_rate() - 66.67).abs() < 0.1);
    }

    #[test]
    fn test_sync_result_failure() {
        let result = SyncResult {
            integration: "test".to_string(),
            model: "ecommerce".to_string(),
            total_explores: 2,
            successful: vec![],
            failed: vec![
                ("orders".to_string(), "Failed to fetch".to_string()),
                ("users".to_string(), "Connection error".to_string()),
            ],
            skipped: vec![],
        };

        assert!(!result.is_success());
        assert!(!result.is_partial_success());
        assert!(result.is_failure());
        assert_eq!(result.success_rate(), 0.0);
    }

    #[test]
    fn test_transform_to_metadata() {
        let explore = LookmlModelExplore {
            name: "orders".to_string(),
            label: Some("Orders".to_string()),
            description: Some("Order analytics".to_string()),
            view_name: Some("orders_base".to_string()),
            fields: Some(ExploreFields {
                dimensions: vec![LookmlModelExploreField {
                    name: "orders.id".to_string(),
                    label: Some("Order ID".to_string()),
                    description: Some("Unique order identifier".to_string()),
                    field_type: Some("dimension".to_string()),
                    view: Some("orders".to_string()),
                    sql: Some("${TABLE}.id".to_string()),
                    type_: Some("number".to_string()),
                    hidden: Some(false),
                    suggest_dimension: None,
                    suggest_explore: None,
                }],
                measures: vec![LookmlModelExploreField {
                    name: "orders.count".to_string(),
                    label: Some("Count".to_string()),
                    description: Some("Number of orders".to_string()),
                    field_type: Some("measure".to_string()),
                    view: Some("orders".to_string()),
                    sql: Some("COUNT(*)".to_string()),
                    type_: Some("number".to_string()),
                    hidden: Some(false),
                    suggest_dimension: None,
                    suggest_explore: None,
                }],
                filters: vec![],
                parameters: vec![],
            }),
            source_file: None,
            sql_table_name: None,
        };

        let metadata = transform_to_metadata(&explore, "ecommerce");

        assert_eq!(metadata.model, "ecommerce");
        assert_eq!(metadata.name, "orders");
        assert_eq!(metadata.base_view_name, Some("orders_base".to_string()));
        assert_eq!(metadata.label, Some("Orders".to_string()));
        assert_eq!(metadata.description, Some("Order analytics".to_string()));
        assert_eq!(metadata.views.len(), 1);
        assert_eq!(metadata.views[0].name, "orders");
        assert_eq!(metadata.views[0].dimensions.len(), 1);
        assert_eq!(metadata.views[0].measures.len(), 1);
    }
}
