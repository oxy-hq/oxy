//! Metadata merging logic for Looker
//!
//! This module implements the overlay-precedence merge strategy where user
//! customizations override base metadata while preserving unmapped base entries.

use std::collections::HashMap;
use std::path::Path;

use tracing::{debug, warn};

use crate::error::LookerError;
use crate::models::{
    ExploreMetadata, FieldMetadata, OverlayExploreMetadata, OverlayFieldMetadata,
    OverlayViewMetadata, ViewMetadata,
};
use crate::storage::MetadataStorage;

/// Merges base and overlay metadata for Looker explores.
///
/// The merger implements an overlay-precedence strategy where user customizations
/// (in project/looker/) override base metadata (in state_dir/.looker/) while
/// preserving unmapped base entries.
#[derive(Debug, Clone)]
pub struct MetadataMerger {
    storage: MetadataStorage,
}

impl MetadataMerger {
    /// Creates a new MetadataMerger with the specified storage configuration.
    pub fn new<P1: AsRef<Path>, P2: AsRef<Path>>(
        state_dir: P1,
        project_dir: P2,
        integration_name: String,
    ) -> Self {
        Self {
            storage: MetadataStorage::new(state_dir, project_dir, integration_name),
        }
    }

    /// Creates a MetadataMerger from an existing storage instance.
    pub fn from_storage(storage: MetadataStorage) -> Self {
        Self { storage }
    }

    /// Loads and merges metadata for an explore.
    ///
    /// Returns the base metadata merged with any overlay customizations.
    /// If no overlay exists, returns the base metadata unchanged.
    pub fn load_merged_metadata(
        &self,
        model: &str,
        explore: &str,
    ) -> Result<ExploreMetadata, LookerError> {
        let base = self.storage.load_base_metadata(model, explore)?;
        let overlay = self.storage.load_overlay_metadata(model, explore)?;

        Ok(Self::merge(base, overlay))
    }

    /// Merges base metadata with optional overlay metadata.
    ///
    /// # Merge Strategy
    ///
    /// - Explore-level: overlay description overrides base if present
    /// - View-level: views are merged by name
    /// - Field-level: fields are merged by name, with overlay values taking precedence
    pub fn merge(
        base: ExploreMetadata,
        overlay: Option<OverlayExploreMetadata>,
    ) -> ExploreMetadata {
        let Some(overlay) = overlay else {
            return base;
        };

        debug!(
            model = base.model,
            explore = base.name,
            "Merging explore metadata with overlay"
        );

        let model = base.model.clone();
        let explore = base.name.clone();

        ExploreMetadata {
            model: base.model,
            name: base.name,
            base_view_name: base.base_view_name,
            label: base.label,
            description: overlay.description.or(base.description),
            views: Self::merge_views(base.views, overlay.views, &model, &explore),
        }
    }

    /// Merges view lists from base and overlay metadata.
    fn merge_views(
        base_views: Vec<ViewMetadata>,
        overlay_views: Option<Vec<OverlayViewMetadata>>,
        model: &str,
        explore: &str,
    ) -> Vec<ViewMetadata> {
        let Some(overlay_views) = overlay_views else {
            return base_views;
        };

        // Create a set of base view names for checking missing views
        let base_view_names: std::collections::HashSet<&str> =
            base_views.iter().map(|v| v.name.as_str()).collect();

        // Create a HashMap of overlay views keyed by name
        let overlay_map: HashMap<String, &OverlayViewMetadata> =
            overlay_views.iter().map(|v| (v.name.clone(), v)).collect();

        // Log warnings for overlay views not found in base
        for overlay_view in &overlay_views {
            if !base_view_names.contains(overlay_view.name.as_str()) {
                warn!(
                    model = model,
                    explore = explore,
                    view = overlay_view.name,
                    "Overlay references view that does not exist in base metadata"
                );
            }
        }

        let mut merged_views: Vec<ViewMetadata> = base_views
            .into_iter()
            .map(|base_view| {
                if let Some(overlay_view) = overlay_map.get(&base_view.name) {
                    Self::merge_view(base_view, overlay_view, model, explore)
                } else {
                    base_view
                }
            })
            .collect();

        // Sort by name for consistent output
        merged_views.sort_by(|a, b| a.name.cmp(&b.name));
        merged_views
    }

    /// Merges a single view with its overlay.
    fn merge_view(
        base: ViewMetadata,
        overlay: &OverlayViewMetadata,
        model: &str,
        explore: &str,
    ) -> ViewMetadata {
        let view_name = base.name.clone();
        ViewMetadata {
            name: base.name,
            dimensions: Self::merge_fields(
                base.dimensions,
                overlay.dimensions.as_ref(),
                model,
                explore,
                &view_name,
                "dimension",
            ),
            measures: Self::merge_fields(
                base.measures,
                overlay.measures.as_ref(),
                model,
                explore,
                &view_name,
                "measure",
            ),
        }
    }

    /// Merges field lists from base and overlay metadata.
    fn merge_fields(
        base_fields: Vec<FieldMetadata>,
        overlay_fields: Option<&Vec<OverlayFieldMetadata>>,
        model: &str,
        explore: &str,
        view: &str,
        field_type: &str,
    ) -> Vec<FieldMetadata> {
        let Some(overlay_fields) = overlay_fields else {
            return base_fields;
        };

        // Create a set of base field names for checking missing fields
        let base_field_names: std::collections::HashSet<&str> =
            base_fields.iter().map(|f| f.name.as_str()).collect();

        // Create a HashMap of overlay fields keyed by name
        let overlay_map: HashMap<&str, &OverlayFieldMetadata> = overlay_fields
            .iter()
            .map(|f| (f.name.as_str(), f))
            .collect();

        // Log warnings for overlay fields not found in base
        for overlay_field in overlay_fields {
            if !base_field_names.contains(overlay_field.name.as_str()) {
                warn!(
                    model = model,
                    explore = explore,
                    view = view,
                    field = overlay_field.name,
                    field_type = field_type,
                    "Overlay references field that does not exist in base metadata"
                );
            }
        }

        let mut merged_fields: Vec<FieldMetadata> = base_fields
            .into_iter()
            .map(|base_field| {
                if let Some(overlay_field) = overlay_map.get(base_field.name.as_str()) {
                    Self::merge_field(base_field, overlay_field)
                } else {
                    base_field
                }
            })
            .collect();

        // Sort by name for consistent output
        merged_fields.sort_by(|a, b| a.name.cmp(&b.name));
        merged_fields
    }

    /// Merges a single field with its overlay.
    ///
    /// Overlay values take precedence over base values when present.
    fn merge_field(base: FieldMetadata, overlay: &OverlayFieldMetadata) -> FieldMetadata {
        FieldMetadata {
            name: base.name,
            label: base.label,
            description: overlay.description.clone().or(base.description),
            field_type: base.field_type,
            data_type: base.data_type,
            sql: base.sql,
            agent_hint: overlay.agent_hint.clone().or(base.agent_hint),
            examples: overlay.examples.clone().or(base.examples),
        }
    }

    /// Checks if merged metadata exists (either base or overlay).
    pub fn merged_metadata_exists(&self, model: &str, explore: &str) -> bool {
        self.storage.base_metadata_exists(model, explore)
            || self.storage.overlay_metadata_exists(model, explore)
    }

    /// Lists all explores that have either base or overlay metadata.
    pub fn list_all_explores(&self, model: &str) -> Result<Vec<String>, LookerError> {
        let mut explores = std::collections::HashSet::new();

        for explore in self.storage.list_base_explores(model)? {
            explores.insert(explore);
        }

        for explore in self.storage.list_overlay_explores(model)? {
            explores.insert(explore);
        }

        let mut result: Vec<String> = explores.into_iter().collect();
        result.sort();
        Ok(result)
    }

    /// Returns a reference to the underlying storage.
    pub fn storage(&self) -> &MetadataStorage {
        &self.storage
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::QueryExample;
    use tempfile::TempDir;

    fn create_test_merger() -> (MetadataMerger, TempDir, TempDir) {
        let state_dir = TempDir::new().unwrap();
        let project_dir = TempDir::new().unwrap();
        let merger = MetadataMerger::new(
            state_dir.path(),
            project_dir.path(),
            "test_integration".to_string(),
        );
        (merger, state_dir, project_dir)
    }

    fn create_base_metadata() -> ExploreMetadata {
        ExploreMetadata {
            model: "ecommerce".to_string(),
            name: "orders".to_string(),
            base_view_name: Some("orders".to_string()),
            label: Some("Orders".to_string()),
            description: Some("Base description".to_string()),
            views: vec![ViewMetadata {
                name: "orders".to_string(),
                dimensions: vec![
                    FieldMetadata {
                        name: "id".to_string(),
                        label: Some("ID".to_string()),
                        description: Some("Order ID".to_string()),
                        field_type: "dimension".to_string(),
                        data_type: Some("number".to_string()),
                        sql: None,
                        agent_hint: None,
                        examples: None,
                    },
                    FieldMetadata {
                        name: "created_date".to_string(),
                        label: Some("Created Date".to_string()),
                        description: None,
                        field_type: "dimension".to_string(),
                        data_type: Some("date".to_string()),
                        sql: None,
                        agent_hint: None,
                        examples: None,
                    },
                ],
                measures: vec![FieldMetadata {
                    name: "count".to_string(),
                    label: Some("Count".to_string()),
                    description: None,
                    field_type: "measure".to_string(),
                    data_type: Some("number".to_string()),
                    sql: None,
                    agent_hint: None,
                    examples: None,
                }],
            }],
        }
    }

    #[test]
    fn test_merge_no_overlay() {
        let base = create_base_metadata();
        let merged = MetadataMerger::merge(base.clone(), None);

        assert_eq!(merged.name, base.name);
        assert_eq!(merged.description, base.description);
        assert_eq!(merged.views.len(), base.views.len());
    }

    #[test]
    fn test_merge_overlay_description() {
        let base = create_base_metadata();
        let overlay = OverlayExploreMetadata {
            description: Some("Custom description".to_string()),
            views: None,
        };

        let merged = MetadataMerger::merge(base, Some(overlay));

        assert_eq!(merged.description, Some("Custom description".to_string()));
    }

    #[test]
    fn test_merge_overlay_field_agent_hint() {
        let base = create_base_metadata();
        let overlay = OverlayExploreMetadata {
            description: None,
            views: Some(vec![OverlayViewMetadata {
                name: "orders".to_string(),
                dimensions: Some(vec![OverlayFieldMetadata {
                    name: "created_date".to_string(),
                    description: None,
                    agent_hint: Some("Supports relative dates like 'last 7 days'".to_string()),
                    examples: None,
                }]),
                measures: None,
            }]),
        };

        let merged = MetadataMerger::merge(base, Some(overlay));

        let orders_view = merged.views.iter().find(|v| v.name == "orders").unwrap();
        let created_date = orders_view
            .dimensions
            .iter()
            .find(|d| d.name == "created_date")
            .unwrap();

        assert_eq!(
            created_date.agent_hint,
            Some("Supports relative dates like 'last 7 days'".to_string())
        );
    }

    #[test]
    fn test_merge_overlay_field_examples() {
        let base = create_base_metadata();
        let example = QueryExample {
            query: "Orders from Q4".to_string(),
            filters: None,
            fields: None,
        };

        let overlay = OverlayExploreMetadata {
            description: None,
            views: Some(vec![OverlayViewMetadata {
                name: "orders".to_string(),
                dimensions: Some(vec![OverlayFieldMetadata {
                    name: "created_date".to_string(),
                    description: None,
                    agent_hint: None,
                    examples: Some(vec![example.clone()]),
                }]),
                measures: None,
            }]),
        };

        let merged = MetadataMerger::merge(base, Some(overlay));

        let orders_view = merged.views.iter().find(|v| v.name == "orders").unwrap();
        let created_date = orders_view
            .dimensions
            .iter()
            .find(|d| d.name == "created_date")
            .unwrap();

        assert!(created_date.examples.is_some());
        assert_eq!(created_date.examples.as_ref().unwrap().len(), 1);
        assert_eq!(
            created_date.examples.as_ref().unwrap()[0].query,
            "Orders from Q4"
        );
    }

    #[test]
    fn test_merge_preserves_unmapped_fields() {
        let base = create_base_metadata();
        let overlay = OverlayExploreMetadata {
            description: None,
            views: Some(vec![OverlayViewMetadata {
                name: "orders".to_string(),
                dimensions: Some(vec![OverlayFieldMetadata {
                    name: "created_date".to_string(),
                    description: Some("Custom description".to_string()),
                    agent_hint: None,
                    examples: None,
                }]),
                measures: None,
            }]),
        };

        let merged = MetadataMerger::merge(base, Some(overlay));

        let orders_view = merged.views.iter().find(|v| v.name == "orders").unwrap();

        // id field should be preserved unchanged
        let id_field = orders_view
            .dimensions
            .iter()
            .find(|d| d.name == "id")
            .unwrap();
        assert_eq!(id_field.description, Some("Order ID".to_string()));

        // created_date field should have overlay description
        let created_date = orders_view
            .dimensions
            .iter()
            .find(|d| d.name == "created_date")
            .unwrap();
        assert_eq!(
            created_date.description,
            Some("Custom description".to_string())
        );
    }

    #[test]
    fn test_load_merged_metadata() {
        let (merger, _state_dir, _project_dir) = create_test_merger();

        // Save base metadata
        let base = create_base_metadata();
        merger
            .storage()
            .save_base_metadata("ecommerce", "orders", &base)
            .unwrap();

        // Save overlay metadata
        let overlay = OverlayExploreMetadata {
            description: Some("Merged description".to_string()),
            views: None,
        };
        merger
            .storage()
            .save_overlay_metadata("ecommerce", "orders", &overlay)
            .unwrap();

        // Load merged
        let merged = merger.load_merged_metadata("ecommerce", "orders").unwrap();

        assert_eq!(merged.description, Some("Merged description".to_string()));
        assert_eq!(merged.views.len(), 1);
    }

    #[test]
    fn test_list_all_explores() {
        let (merger, _state_dir, _project_dir) = create_test_merger();

        // Save base metadata for "orders"
        let base = create_base_metadata();
        merger
            .storage()
            .save_base_metadata("ecommerce", "orders", &base)
            .unwrap();

        // Save overlay metadata for "users" (without base)
        let overlay = OverlayExploreMetadata {
            description: Some("Users overlay".to_string()),
            views: None,
        };
        merger
            .storage()
            .save_overlay_metadata("ecommerce", "users", &overlay)
            .unwrap();

        // List all explores
        let explores = merger.list_all_explores("ecommerce").unwrap();

        assert_eq!(explores.len(), 2);
        assert!(explores.contains(&"orders".to_string()));
        assert!(explores.contains(&"users".to_string()));
    }

    #[test]
    fn test_merged_metadata_exists() {
        let (merger, _state_dir, _project_dir) = create_test_merger();

        // Initially should not exist
        assert!(!merger.merged_metadata_exists("ecommerce", "orders"));

        // Save base metadata
        let base = create_base_metadata();
        merger
            .storage()
            .save_base_metadata("ecommerce", "orders", &base)
            .unwrap();

        // Now should exist
        assert!(merger.merged_metadata_exists("ecommerce", "orders"));
    }

    #[test]
    fn test_merge_logs_warning_for_missing_view() {
        // This test verifies that merging with a non-existent view in overlay
        // doesn't panic and correctly ignores the missing view
        let base = create_base_metadata();
        let overlay = OverlayExploreMetadata {
            description: None,
            views: Some(vec![OverlayViewMetadata {
                name: "nonexistent_view".to_string(),
                dimensions: Some(vec![OverlayFieldMetadata {
                    name: "some_field".to_string(),
                    description: Some("Custom description".to_string()),
                    agent_hint: None,
                    examples: None,
                }]),
                measures: None,
            }]),
        };

        // Should not panic, warning is logged (but we can't easily assert on logs)
        let merged = MetadataMerger::merge(base.clone(), Some(overlay));

        // Base metadata should be unchanged since overlay view doesn't exist
        assert_eq!(merged.views.len(), base.views.len());
        assert_eq!(merged.views[0].name, "orders");
    }

    #[test]
    fn test_merge_logs_warning_for_missing_field() {
        // This test verifies that merging with a non-existent field in overlay
        // doesn't panic and correctly ignores the missing field
        let base = create_base_metadata();
        let overlay = OverlayExploreMetadata {
            description: None,
            views: Some(vec![OverlayViewMetadata {
                name: "orders".to_string(),
                dimensions: Some(vec![OverlayFieldMetadata {
                    name: "nonexistent_field".to_string(),
                    description: Some("Custom description".to_string()),
                    agent_hint: Some("Some hint".to_string()),
                    examples: None,
                }]),
                measures: Some(vec![OverlayFieldMetadata {
                    name: "nonexistent_measure".to_string(),
                    description: None,
                    agent_hint: Some("Measure hint".to_string()),
                    examples: None,
                }]),
            }]),
        };

        // Should not panic, warnings are logged (but we can't easily assert on logs)
        let merged = MetadataMerger::merge(base.clone(), Some(overlay));

        // Base metadata should be unchanged since overlay fields don't exist
        let orders_view = merged.views.iter().find(|v| v.name == "orders").unwrap();
        assert_eq!(orders_view.dimensions.len(), 2); // id and created_date
        assert_eq!(orders_view.measures.len(), 1); // count

        // Verify existing fields are not modified
        let id_field = orders_view
            .dimensions
            .iter()
            .find(|d| d.name == "id")
            .unwrap();
        assert_eq!(id_field.description, Some("Order ID".to_string()));
        assert!(id_field.agent_hint.is_none());
    }

    #[test]
    fn test_merge_with_both_valid_and_missing_overlay_entries() {
        // Test that valid overlay entries are applied while missing ones are logged
        let base = create_base_metadata();
        let overlay = OverlayExploreMetadata {
            description: Some("Updated description".to_string()),
            views: Some(vec![
                OverlayViewMetadata {
                    name: "orders".to_string(), // Valid view
                    dimensions: Some(vec![
                        OverlayFieldMetadata {
                            name: "id".to_string(), // Valid field
                            description: Some("Updated ID description".to_string()),
                            agent_hint: Some("Primary key".to_string()),
                            examples: None,
                        },
                        OverlayFieldMetadata {
                            name: "missing_dimension".to_string(), // Invalid field
                            description: Some("Won't be applied".to_string()),
                            agent_hint: None,
                            examples: None,
                        },
                    ]),
                    measures: None,
                },
                OverlayViewMetadata {
                    name: "missing_view".to_string(), // Invalid view
                    dimensions: None,
                    measures: None,
                },
            ]),
        };

        let merged = MetadataMerger::merge(base, Some(overlay));

        // Description should be updated
        assert_eq!(merged.description, Some("Updated description".to_string()));

        // Valid field should be updated
        let orders_view = merged.views.iter().find(|v| v.name == "orders").unwrap();
        let id_field = orders_view
            .dimensions
            .iter()
            .find(|d| d.name == "id")
            .unwrap();
        assert_eq!(
            id_field.description,
            Some("Updated ID description".to_string())
        );
        assert_eq!(id_field.agent_hint, Some("Primary key".to_string()));

        // Missing view should not create a new view
        assert!(
            merged
                .views
                .iter()
                .find(|v| v.name == "missing_view")
                .is_none()
        );

        // Missing field should not create a new field
        assert!(
            orders_view
                .dimensions
                .iter()
                .find(|d| d.name == "missing_dimension")
                .is_none()
        );
    }
}
