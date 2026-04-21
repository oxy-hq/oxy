use std::collections::HashMap;
use std::path::Path;

use crate::error::OmniError;
use crate::models::{DimensionMetadata, MeasureMetadata, TopicMetadata, ViewMetadata};
use crate::storage::MetadataStorage;

/// Handles merging of base Omni metadata with custom overlay metadata
///
/// The MetadataMerger implements precedence rules where custom overlay metadata
/// takes precedence over base Omni metadata. This allows users to add additional
/// context, descriptions, and examples on top of the base semantic layer.
#[derive(Debug, Clone)]
pub struct MetadataMerger {
    storage: MetadataStorage,
}

impl MetadataMerger {
    /// Create a new MetadataMerger instance
    pub fn new<P: AsRef<Path>>(project_path: P, integration_name: String) -> Self {
        Self {
            storage: MetadataStorage::new(project_path, integration_name),
        }
    }

    /// Load and merge metadata for a specific topic
    ///
    /// This function loads both base metadata from .omni directory and overlay
    /// metadata from omni directory, then merges them with overlay taking precedence.
    pub fn load_merged_metadata(
        &self,
        model_id: &str,
        topic_name: &str,
    ) -> Result<Option<TopicMetadata>, OmniError> {
        // Delegate to the storage layer for the merged metadata loading
        self.storage.load_merged_metadata(model_id, topic_name)
    }

    /// Merge base topic metadata with optional overlay metadata
    ///
    /// Implements precedence rules where overlay metadata takes precedence over base metadata.
    /// The merge strategy is:
    /// - Simple fields: overlay value if present, otherwise base value
    /// - Collections (views, relationships): merge by name/identifier with overlay precedence
    /// - Custom fields: overlay values are used directly
    pub fn merge_topic_metadata(
        base: TopicMetadata,
        overlay: Option<TopicMetadata>,
    ) -> TopicMetadata {
        let overlay = match overlay {
            Some(overlay) => overlay,
            None => return base,
        };

        TopicMetadata {
            name: overlay.name.clone(), // Name should match, but overlay takes precedence
            label: overlay.label.or(base.label),
            views: Self::merge_views(base.views, overlay.views),
            // Custom overlay fields - overlay takes full precedence
            custom_description: overlay.custom_description.or(base.custom_description),
            agent_hints: overlay.agent_hints.or(base.agent_hints),
            examples: overlay.examples.or(base.examples),
        }
    }

    /// Merge view collections with overlay precedence
    ///
    /// Views are merged by name. If a view exists in both base and overlay,
    /// the overlay view takes precedence but dimensions and measures are merged.
    fn merge_views(
        base_views: Vec<ViewMetadata>,
        overlay_views: Vec<ViewMetadata>,
    ) -> Vec<ViewMetadata> {
        let mut merged_views = Vec::new();
        let mut overlay_map: HashMap<String, ViewMetadata> = overlay_views
            .into_iter()
            .map(|view| (view.name.clone(), view))
            .collect();

        // Process base views, merging with overlay if present
        for base_view in base_views {
            if let Some(overlay_view) = overlay_map.remove(&base_view.name) {
                // Merge base and overlay view
                merged_views.push(Self::merge_view_metadata(base_view, overlay_view));
            } else {
                // Only base view exists
                merged_views.push(base_view);
            }
        }

        // Add any remaining overlay views that weren't in base
        for (_, overlay_view) in overlay_map {
            merged_views.push(overlay_view);
        }

        // Sort by name for consistent ordering
        merged_views.sort_by(|a, b| a.name.cmp(&b.name));
        merged_views
    }

    /// Merge individual view metadata with overlay precedence
    fn merge_view_metadata(base: ViewMetadata, overlay: ViewMetadata) -> ViewMetadata {
        ViewMetadata {
            name: overlay.name, // Should match, but overlay takes precedence
            dimensions: Self::merge_dimensions(base.dimensions, overlay.dimensions),
            measures: Self::merge_measures(base.measures, overlay.measures),
            filter_only_fields: if overlay.filter_only_fields.is_empty() {
                base.filter_only_fields
            } else {
                overlay.filter_only_fields
            },
        }
    }

    /// Merge dimension collections with overlay precedence
    ///
    /// Dimensions are merged by field_name and view_name combination.
    /// Overlay dimensions take precedence, but only for fields that are specified (not None).
    /// This allows users to override only specific dimension properties.
    fn merge_dimensions(
        base_dimensions: Vec<DimensionMetadata>,
        overlay_dimensions: Vec<DimensionMetadata>,
    ) -> Vec<DimensionMetadata> {
        let mut merged_dimensions = Vec::new();
        let mut overlay_map: HashMap<(String, String), DimensionMetadata> = overlay_dimensions
            .into_iter()
            .map(|dim| ((dim.field_name.clone(), dim.view_name.clone()), dim))
            .collect();

        // Process base dimensions, merging with overlay if present
        for base_dim in base_dimensions {
            let key = (base_dim.field_name.clone(), base_dim.view_name.clone());
            if let Some(overlay_dim) = overlay_map.remove(&key) {
                // Merge base and overlay dimension, with overlay taking precedence
                merged_dimensions.push(Self::merge_dimension_metadata(base_dim, overlay_dim));
            } else {
                // Only base dimension exists
                merged_dimensions.push(base_dim);
            }
        }

        // Add any remaining overlay dimensions that weren't in base
        for (_, overlay_dim) in overlay_map {
            merged_dimensions.push(overlay_dim);
        }

        // Sort by field_name for consistent ordering
        merged_dimensions.sort_by(|a, b| a.field_name.cmp(&b.field_name));
        merged_dimensions
    }

    /// Merge individual dimension metadata with overlay precedence
    /// Base values are used when overlay values are not specified
    fn merge_dimension_metadata(
        base: DimensionMetadata,
        overlay: DimensionMetadata,
    ) -> DimensionMetadata {
        DimensionMetadata {
            field_name: overlay.field_name, // Should match
            view_name: overlay.view_name,   // Should match
            data_type: overlay.data_type,
            fully_qualified_name: overlay.fully_qualified_name,
            description: overlay.description.or(base.description),
            ai_context: overlay.ai_context.or(base.ai_context),
            label: overlay.label.or(base.label),
        }
    }

    /// Merge measure collections with overlay precedence
    ///
    /// Measures are merged by field_name and view_name combination.
    /// Overlay measures take precedence, but only for fields that are specified.
    /// This allows users to override only specific measure properties.
    fn merge_measures(
        base_measures: Vec<MeasureMetadata>,
        overlay_measures: Vec<MeasureMetadata>,
    ) -> Vec<MeasureMetadata> {
        let mut merged_measures = Vec::new();
        let mut overlay_map: HashMap<(String, String), MeasureMetadata> = overlay_measures
            .into_iter()
            .map(|measure| {
                (
                    (measure.field_name.clone(), measure.view_name.clone()),
                    measure,
                )
            })
            .collect();

        // Process base measures, merging with overlay if present
        for base_measure in base_measures {
            let key = (
                base_measure.field_name.clone(),
                base_measure.view_name.clone(),
            );
            if let Some(overlay_measure) = overlay_map.remove(&key) {
                // Merge base and overlay measure, with overlay taking precedence
                merged_measures.push(Self::merge_measure_metadata(base_measure, overlay_measure));
            } else {
                // Only base measure exists
                merged_measures.push(base_measure);
            }
        }

        // Add any remaining overlay measures that weren't in base
        for (_, overlay_measure) in overlay_map {
            merged_measures.push(overlay_measure);
        }

        // Sort by field_name for consistent ordering
        merged_measures.sort_by(|a, b| a.field_name.cmp(&b.field_name));
        merged_measures
    }

    /// Merge individual measure metadata with overlay precedence
    /// Base values are used when overlay values are not available
    fn merge_measure_metadata(base: MeasureMetadata, overlay: MeasureMetadata) -> MeasureMetadata {
        // Merge individual fields with overlay precedence
        MeasureMetadata {
            field_name: overlay.field_name, // Should match
            view_name: overlay.view_name,   // Should match
            data_type: overlay.data_type,
            fully_qualified_name: overlay.fully_qualified_name,
            description: overlay.description.or(base.description),
            ai_context: overlay.ai_context.or(base.ai_context),
            label: overlay.label.or(base.label),
        }
    }

    /// Check if merged metadata exists for a topic
    ///
    /// Returns true if either base or overlay metadata exists
    pub fn merged_metadata_exists(&self, model_id: &str, topic_name: &str) -> bool {
        self.storage.base_metadata_exists(model_id, topic_name)
            || self.storage.overlay_metadata_exists(model_id, topic_name)
    }

    /// List all topics that have either base or overlay metadata
    pub fn list_all_topics(&self, model_id: &str) -> Result<Vec<String>, OmniError> {
        let base_topics = self.storage.list_base_topics(model_id)?;
        let overlay_topics = self.storage.list_overlay_topics(model_id)?;

        let mut all_topics: Vec<String> = base_topics
            .into_iter()
            .chain(overlay_topics)
            .collect::<std::collections::HashSet<_>>() // Remove duplicates
            .into_iter()
            .collect();

        all_topics.sort();
        Ok(all_topics)
    }

    /// Get metadata storage reference for direct access
    pub fn storage(&self) -> &MetadataStorage {
        &self.storage
    }
}
