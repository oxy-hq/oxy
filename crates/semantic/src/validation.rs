use crate::SemanticLayerError;
use crate::cube::entity_graph::EntityGraph;
use crate::models::*;
use std::collections::HashSet;

/// Validation result for semantic layer components
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl ValidationResult {
    pub fn new() -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn add_error(&mut self, error: String) {
        self.is_valid = false;
        self.errors.push(error);
    }

    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    pub fn merge(&mut self, other: ValidationResult) {
        if !other.is_valid {
            self.is_valid = false;
        }
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
    }
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Validator trait for semantic layer components
pub trait SemanticValidator {
    fn validate(&self) -> ValidationResult;
}

impl SemanticValidator for Entity {
    fn validate(&self) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Validate name
        if self.name.is_empty() {
            result.add_error("Entity name cannot be empty".to_string());
        }

        if !self.name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            result.add_error(format!(
                "Entity name '{}' contains invalid characters. Only alphanumeric characters and underscores are allowed",
                self.name
            ));
        }

        // Validate description
        if self.description.is_empty() {
            result.add_error("Entity description cannot be empty".to_string());
        }

        // Validate key or keys
        if self.key.is_none() && self.keys.is_none() {
            result.add_error("Entity must have either 'key' or 'keys' specified".to_string());
        }

        if self.key.is_some() && self.keys.is_some() {
            result.add_warning(
                "Entity has both 'key' and 'keys' specified. Using 'keys' and ignoring 'key'"
                    .to_string(),
            );
        }

        // Validate single key
        if let Some(ref key) = self.key
            && key.is_empty()
        {
            result.add_error("Entity key cannot be empty".to_string());
        }

        // Validate multiple keys
        if let Some(ref keys) = self.keys {
            if keys.is_empty() {
                result.add_error("Entity keys cannot be an empty array".to_string());
            } else {
                for (idx, key) in keys.iter().enumerate() {
                    if key.is_empty() {
                        result.add_error(format!("Entity key at index {} cannot be empty", idx));
                    }
                }
            }
        }

        result
    }
}

impl SemanticValidator for Dimension {
    fn validate(&self) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Validate name
        if self.name.is_empty() {
            result.add_error("Dimension name cannot be empty".to_string());
        }

        if !self.name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            result.add_error(format!(
                "Dimension name '{}' contains invalid characters. Only alphanumeric characters and underscores are allowed",
                self.name
            ));
        }

        // Validate description (optional)
        if let Some(ref description) = self.description
            && description.is_empty()
        {
            result.add_error("Dimension description cannot be empty when provided".to_string());
        }

        // Validate expr
        if self.expr.is_empty() {
            result.add_error("Dimension expr cannot be empty".to_string());
        }

        // Validate synonyms
        if let Some(synonyms) = &self.synonyms {
            if synonyms.is_empty() {
                result.add_warning("Synonyms list should not be empty when specified".to_string());
            }

            let mut unique_synonyms = HashSet::new();
            for synonym in synonyms {
                if !unique_synonyms.insert(synonym) {
                    result.add_warning(format!("Duplicate synonym '{}' found", synonym));
                }
            }
        }

        result
    }
}

impl SemanticValidator for Measure {
    fn validate(&self) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Validate name
        if self.name.is_empty() {
            result.add_error("Measure name cannot be empty".to_string());
        }

        if !self.name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            result.add_error(format!(
                "Measure name '{}' contains invalid characters. Only alphanumeric characters and underscores are allowed",
                self.name
            ));
        }

        // Validate description (optional)
        if let Some(ref description) = self.description
            && description.is_empty()
        {
            result.add_error("Measure description cannot be empty when provided".to_string());
        }

        // Validate expr based on measure type
        match self.measure_type {
            MeasureType::Count => {
                // Count measures don't require expr
                if self.expr.is_some() {
                    result.add_warning(
                        "Count measures typically don't need an expression".to_string(),
                    );
                }
            }
            MeasureType::Custom => {
                // Custom measures require expr
                if self.expr.is_none() {
                    result.add_error("Custom measures require an 'expr' field".to_string());
                }
            }
            _ => {
                // Other measure types require expr
                if self.expr.is_none() {
                    result.add_error(format!(
                        "{:?} measures require an 'expr' field",
                        self.measure_type
                    ));
                }
            }
        }

        // Validate synonyms
        if let Some(synonyms) = &self.synonyms {
            if synonyms.is_empty() {
                result.add_warning("Synonyms list should not be empty when specified".to_string());
            }

            let mut unique_synonyms = HashSet::new();
            for synonym in synonyms {
                if !unique_synonyms.insert(synonym) {
                    result.add_warning(format!("Duplicate synonym '{}' found", synonym));
                }
            }
        }

        result
    }
}

impl SemanticValidator for View {
    fn validate(&self) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Validate name
        if self.name.is_empty() {
            result.add_error("View name cannot be empty".to_string());
        }

        if !self.name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            result.add_error(format!(
                "View name '{}' contains invalid characters. Only alphanumeric characters and underscores are allowed",
                self.name
            ));
        }

        // Validate description
        if self.description.is_empty() {
            result.add_error("View description cannot be empty".to_string());
        }

        // Validate data source configuration
        if self.table.is_none() && self.sql.is_none() {
            result.add_error("View must specify either 'table' or 'sql' field".to_string());
        }

        if self.table.is_some() && self.sql.is_some() {
            result.add_warning("View should specify either 'table' or 'sql', not both".to_string());
        }

        // Validate entities
        if self.entities.is_empty() {
            result.add_error("View must have at least one entity".to_string());
        } else {
            let mut entity_names = HashSet::new();
            let mut primary_entities = 0;

            for entity in &self.entities {
                // Check for duplicate entity names
                if !entity_names.insert(&entity.name) {
                    result.add_error(format!("Duplicate entity name '{}' found", entity.name));
                }

                // Count primary entities
                if entity.entity_type == EntityType::Primary {
                    primary_entities += 1;
                }

                // Validate individual entity
                result.merge(entity.validate());
            }

            // Validate primary entity count
            if primary_entities == 0 {
                result.add_error("View must have exactly one primary entity".to_string());
            } else if primary_entities > 1 {
                result.add_error("View can only have one primary entity".to_string());
            }
        }

        // Validate dimensions
        if self.dimensions.is_empty() {
            result.add_error("View must have at least one dimension".to_string());
        } else {
            let mut dimension_names = HashSet::new();

            for dimension in &self.dimensions {
                // Check for duplicate dimension names
                if !dimension_names.insert(&dimension.name) {
                    result.add_error(format!(
                        "Duplicate dimension name '{}' found",
                        dimension.name
                    ));
                }

                // Validate individual dimension
                result.merge(dimension.validate());
            }
        }

        // Validate measures
        if let Some(measures) = &self.measures {
            let mut measure_names = HashSet::new();

            for measure in measures {
                // Check for duplicate measure names
                if !measure_names.insert(&measure.name) {
                    result.add_error(format!("Duplicate measure name '{}' found", measure.name));
                }

                // Validate individual measure
                result.merge(measure.validate());
            }
        }

        // Check for name conflicts between entities, dimensions, and measures
        let mut all_names = HashSet::new();

        for entity in &self.entities {
            if !all_names.insert(&entity.name) {
                result.add_error(format!(
                    "Name conflict: '{}' is used by multiple components",
                    entity.name
                ));
            }
        }

        for dimension in &self.dimensions {
            if !all_names.insert(&dimension.name) {
                result.add_error(format!(
                    "Name conflict: '{}' is used by multiple components",
                    dimension.name
                ));
            }
        }

        if let Some(measures) = &self.measures {
            for measure in measures {
                if !all_names.insert(&measure.name) {
                    result.add_error(format!(
                        "Name conflict: '{}' is used by multiple components",
                        measure.name
                    ));
                }
            }
        }

        result
    }
}

impl SemanticValidator for Topic {
    fn validate(&self) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Validate name
        if self.name.is_empty() {
            result.add_error("Topic name cannot be empty".to_string());
        }

        if !self.name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            result.add_error(format!(
                "Topic name '{}' contains invalid characters. Only alphanumeric characters and underscores are allowed",
                self.name
            ));
        }

        // Validate description
        if self.description.is_empty() {
            result.add_error("Topic description cannot be empty".to_string());
        }

        // Validate views
        if self.views.is_empty() {
            result.add_error("Topic must include at least one view".to_string());
        } else {
            let mut unique_views = HashSet::new();
            for view_name in &self.views {
                if view_name.is_empty() {
                    result.add_error("View name in topic cannot be empty".to_string());
                }
                if !unique_views.insert(view_name) {
                    result.add_warning(format!("Duplicate view '{}' found in topic", view_name));
                }
            }
        }

        // Validate base_view if specified
        if let Some(ref base_view) = self.base_view {
            if base_view.is_empty() {
                result.add_error("Topic base_view cannot be empty".to_string());
            } else if !self.views.contains(base_view) {
                result.add_error(format!(
                    "Topic base_view '{}' must be included in the views list",
                    base_view
                ));
            }
        }

        // Validate default_filters if specified
        if let Some(ref filters) = self.default_filters {
            for (i, filter) in filters.iter().enumerate() {
                if filter.field.trim().is_empty() {
                    result.add_error(format!(
                        "Default filter at index {} has empty field name",
                        i
                    ));
                }
            }
        }

        result
    }
}

impl SemanticValidator for SemanticLayer {
    fn validate(&self) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Validate views
        if self.views.is_empty() {
            result.add_error("Semantic layer must have at least one view".to_string());
        } else {
            let mut view_names = HashSet::new();

            for view in &self.views {
                // Check for duplicate view names
                if !view_names.insert(&view.name) {
                    result.add_error(format!("Duplicate view name '{}' found", view.name));
                }

                // Validate individual view
                result.merge(view.validate());
            }
        }

        // Validate topics
        if let Some(topics) = &self.topics {
            let mut topic_names = HashSet::new();
            let view_names: HashSet<_> = self.views.iter().map(|v| &v.name).collect();

            for topic in topics {
                // Check for duplicate topic names
                if !topic_names.insert(&topic.name) {
                    result.add_error(format!("Duplicate topic name '{}' found", topic.name));
                }

                // Validate individual topic
                result.merge(topic.validate());

                // Check if all referenced views exist
                for view_name in &topic.views {
                    if !view_names.contains(view_name) {
                        result.add_error(format!(
                            "Topic '{}' references non-existent view '{}'",
                            topic.name, view_name
                        ));
                    }
                }

                // Validate that all views in the topic use the same datasource
                let topic_views: Vec<_> = self
                    .views
                    .iter()
                    .filter(|v| topic.views.contains(&v.name))
                    .collect();

                if topic_views.len() > 1 {
                    let mut datasources = HashSet::new();
                    for view in &topic_views {
                        // Use "default" as the datasource if none is specified
                        let datasource = view.datasource.as_deref().unwrap_or("default");
                        datasources.insert(datasource);
                    }

                    if datasources.len() > 1 {
                        result.add_error(format!(
                            "Topic '{}' contains views from different datasources: {}. All views in a topic must use the same datasource",
                            topic.name,
                            datasources.into_iter().collect::<Vec<_>>().join(", ")
                        ));
                    }
                }

                // Validate base_view reachability if specified
                if let Some(ref base_view) = topic.base_view {
                    // Build entity graph to check reachability
                    match EntityGraph::from_semantic_layer(self) {
                        Ok(entity_graph) => {
                            let unreachable_views = entity_graph
                                .validate_base_view_reachability(base_view, &topic.views);

                            if !unreachable_views.is_empty() {
                                result.add_error(format!(
                                    "Topic '{}' has base_view '{}' but the following views are not reachable via joins: {}. Ensure all views have proper entity relationships defined.",
                                    topic.name,
                                    base_view,
                                    unreachable_views.join(", ")
                                ));
                            }
                        }
                        Err(e) => {
                            result.add_warning(format!(
                                "Could not validate base_view reachability for topic '{}': {}",
                                topic.name, e
                            ));
                        }
                    }
                }
            }
        }

        result
    }
}

/// Validates a complete semantic layer configuration
pub fn validate_semantic_layer(
    semantic_layer: &SemanticLayer,
) -> Result<ValidationResult, SemanticLayerError> {
    Ok(semantic_layer.validate())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Entity, EntityType};

    #[test]
    fn test_entity_validation_with_single_key() {
        let entity = Entity {
            name: "customer".to_string(),
            entity_type: EntityType::Primary,
            description: "Customer entity".to_string(),
            key: Some("customer_id".to_string()),
            keys: None,
        };

        let result = entity.validate();
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_entity_validation_with_composite_keys() {
        let entity = Entity {
            name: "order_item".to_string(),
            entity_type: EntityType::Primary,
            description: "Order item entity".to_string(),
            key: None,
            keys: Some(vec!["order_id".to_string(), "line_item_id".to_string()]),
        };

        let result = entity.validate();
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_entity_validation_with_both_key_and_keys() {
        let entity = Entity {
            name: "test_entity".to_string(),
            entity_type: EntityType::Primary,
            description: "Test entity".to_string(),
            key: Some("id".to_string()),
            keys: Some(vec!["id".to_string(), "tenant_id".to_string()]),
        };

        let result = entity.validate();
        assert!(result.is_valid);
        // Should have a warning about both being specified
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("both 'key' and 'keys'"));
    }

    #[test]
    fn test_entity_validation_missing_keys() {
        let entity = Entity {
            name: "test_entity".to_string(),
            entity_type: EntityType::Primary,
            description: "Test entity".to_string(),
            key: None,
            keys: None,
        };

        let result = entity.validate();
        assert!(!result.is_valid);
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("must have either 'key' or 'keys'"))
        );
    }

    #[test]
    fn test_entity_validation_empty_key() {
        let entity = Entity {
            name: "test_entity".to_string(),
            entity_type: EntityType::Primary,
            description: "Test entity".to_string(),
            key: Some("".to_string()),
            keys: None,
        };

        let result = entity.validate();
        assert!(!result.is_valid);
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("key cannot be empty"))
        );
    }

    #[test]
    fn test_entity_validation_empty_keys_array() {
        let entity = Entity {
            name: "test_entity".to_string(),
            entity_type: EntityType::Primary,
            description: "Test entity".to_string(),
            key: None,
            keys: Some(vec![]),
        };

        let result = entity.validate();
        assert!(!result.is_valid);
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("keys cannot be an empty array"))
        );
    }

    #[test]
    fn test_entity_validation_empty_string_in_keys() {
        let entity = Entity {
            name: "test_entity".to_string(),
            entity_type: EntityType::Primary,
            description: "Test entity".to_string(),
            key: None,
            keys: Some(vec!["key1".to_string(), "".to_string(), "key3".to_string()]),
        };

        let result = entity.validate();
        assert!(!result.is_valid);
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("key at index 1 cannot be empty"))
        );
    }

    #[test]
    fn test_entity_get_keys_with_single_key() {
        let entity = Entity {
            name: "test".to_string(),
            entity_type: EntityType::Primary,
            description: "Test".to_string(),
            key: Some("id".to_string()),
            keys: None,
        };

        let keys = entity.get_keys();
        assert_eq!(keys, vec!["id".to_string()]);
    }

    #[test]
    fn test_entity_get_keys_with_composite_keys() {
        let entity = Entity {
            name: "test".to_string(),
            entity_type: EntityType::Primary,
            description: "Test".to_string(),
            key: None,
            keys: Some(vec!["key1".to_string(), "key2".to_string()]),
        };

        let keys = entity.get_keys();
        assert_eq!(keys, vec!["key1".to_string(), "key2".to_string()]);
    }

    #[test]
    fn test_entity_get_keys_prefers_keys_over_key() {
        let entity = Entity {
            name: "test".to_string(),
            entity_type: EntityType::Primary,
            description: "Test".to_string(),
            key: Some("single_key".to_string()),
            keys: Some(vec!["key1".to_string(), "key2".to_string()]),
        };

        let keys = entity.get_keys();
        // Should return keys, not key
        assert_eq!(keys, vec!["key1".to_string(), "key2".to_string()]);
    }

    #[test]
    fn test_entity_is_composite() {
        let single_key = Entity {
            name: "test".to_string(),
            entity_type: EntityType::Primary,
            description: "Test".to_string(),
            key: Some("id".to_string()),
            keys: None,
        };
        assert!(!single_key.is_composite());

        let composite_key = Entity {
            name: "test".to_string(),
            entity_type: EntityType::Primary,
            description: "Test".to_string(),
            key: None,
            keys: Some(vec!["key1".to_string(), "key2".to_string()]),
        };
        assert!(composite_key.is_composite());

        let single_element_keys = Entity {
            name: "test".to_string(),
            entity_type: EntityType::Primary,
            description: "Test".to_string(),
            key: None,
            keys: Some(vec!["id".to_string()]),
        };
        assert!(!single_element_keys.is_composite());
    }

    #[test]
    fn test_topic_base_view_validation() {
        use crate::Topic;

        // Test valid topic with base_view
        let valid_topic = Topic {
            name: "sales".to_string(),
            description: "Sales data".to_string(),
            views: vec!["orders".to_string(), "customers".to_string()],
            base_view: Some("orders".to_string()),
            retrieval: None,
            default_filters: None,
        };
        let result = valid_topic.validate();
        assert!(result.is_valid, "Valid topic should pass validation");

        // Test topic with base_view not in views list
        let invalid_topic = Topic {
            name: "sales".to_string(),
            description: "Sales data".to_string(),
            views: vec!["orders".to_string(), "customers".to_string()],
            base_view: Some("products".to_string()),
            retrieval: None,
            default_filters: None,
        };
        let result = invalid_topic.validate();
        assert!(
            !result.is_valid,
            "Topic with base_view not in views should fail"
        );
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("must be included in the views list")),
            "Should have error about base_view not in views"
        );

        // Test topic with empty base_view
        let empty_base_view_topic = Topic {
            name: "sales".to_string(),
            description: "Sales data".to_string(),
            views: vec!["orders".to_string(), "customers".to_string()],
            base_view: Some("".to_string()),
            retrieval: None,
            default_filters: None,
        };
        let result = empty_base_view_topic.validate();
        assert!(!result.is_valid, "Topic with empty base_view should fail");
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("base_view cannot be empty")),
            "Should have error about empty base_view"
        );
    }

    #[test]
    fn test_topic_default_filters_validation() {
        use crate::{Topic, TopicArrayFilter, TopicFilter, TopicFilterType, TopicScalarFilter};

        // Test valid topic with default_filters
        let valid_topic = Topic {
            name: "sales".to_string(),
            description: "Sales data".to_string(),
            views: vec!["orders".to_string()],
            base_view: None,
            retrieval: None,
            default_filters: Some(vec![
                TopicFilter {
                    field: "status".to_string(),
                    filter_type: TopicFilterType::Neq(TopicScalarFilter {
                        value: serde_json::json!("cancelled"),
                    }),
                },
                TopicFilter {
                    field: "amount".to_string(),
                    filter_type: TopicFilterType::Gt(TopicScalarFilter {
                        value: serde_json::json!(0),
                    }),
                },
            ]),
        };
        let result = valid_topic.validate();
        assert!(
            result.is_valid,
            "Valid topic with default_filters should pass validation"
        );

        // Test topic with empty filter field
        let invalid_topic = Topic {
            name: "sales".to_string(),
            description: "Sales data".to_string(),
            views: vec!["orders".to_string()],
            base_view: None,
            retrieval: None,
            default_filters: Some(vec![TopicFilter {
                field: "".to_string(),
                filter_type: TopicFilterType::Eq(TopicScalarFilter {
                    value: serde_json::json!("test"),
                }),
            }]),
        };
        let result = invalid_topic.validate();
        assert!(
            !result.is_valid,
            "Topic with empty filter field should fail"
        );
        assert!(
            result.errors.iter().any(|e| e.contains("empty field name")),
            "Should have error about empty filter field"
        );

        // Test topic with whitespace-only filter field
        let whitespace_topic = Topic {
            name: "sales".to_string(),
            description: "Sales data".to_string(),
            views: vec!["orders".to_string()],
            base_view: None,
            retrieval: None,
            default_filters: Some(vec![TopicFilter {
                field: "   ".to_string(),
                filter_type: TopicFilterType::In(TopicArrayFilter {
                    values: vec![serde_json::json!("a"), serde_json::json!("b")],
                }),
            }]),
        };
        let result = whitespace_topic.validate();
        assert!(
            !result.is_valid,
            "Topic with whitespace-only filter field should fail"
        );
    }

    #[test]
    fn test_semantic_layer_base_view_reachability() {
        use crate::{Dimension, DimensionType, SemanticLayer, Topic, View};

        // Create views with proper entity relationships
        let orders_view = View {
            name: "orders".to_string(),
            description: "Orders".to_string(),
            table: Some("orders".to_string()),
            sql: None,
            datasource: Some("test_db".to_string()),
            label: None,
            entities: vec![
                Entity {
                    name: "order".to_string(),
                    entity_type: EntityType::Primary,
                    description: "Order entity".to_string(),
                    key: Some("order_id".to_string()),
                    keys: None,
                },
                Entity {
                    name: "customer".to_string(),
                    entity_type: EntityType::Foreign,
                    description: "Customer who placed order".to_string(),
                    key: Some("customer_id".to_string()),
                    keys: None,
                },
            ],
            dimensions: vec![Dimension {
                name: "order_id".to_string(),
                dimension_type: DimensionType::String,
                description: Some("Order ID".to_string()),
                expr: "order_id".to_string(),
                samples: None,
                synonyms: None,
            }],
            measures: None,
        };

        let customers_view = View {
            name: "customers".to_string(),
            description: "Customers".to_string(),
            table: Some("customers".to_string()),
            sql: None,
            datasource: Some("test_db".to_string()),
            label: None,
            entities: vec![Entity {
                name: "customer".to_string(),
                entity_type: EntityType::Primary,
                description: "Customer entity".to_string(),
                key: Some("customer_id".to_string()),
                keys: None,
            }],
            dimensions: vec![Dimension {
                name: "customer_id".to_string(),
                dimension_type: DimensionType::String,
                description: Some("Customer ID".to_string()),
                expr: "customer_id".to_string(),
                samples: None,
                synonyms: None,
            }],
            measures: None,
        };

        // Create an unreachable view (no entity connection)
        let products_view = View {
            name: "products".to_string(),
            description: "Products".to_string(),
            table: Some("products".to_string()),
            sql: None,
            datasource: Some("test_db".to_string()),
            label: None,
            entities: vec![Entity {
                name: "product".to_string(),
                entity_type: EntityType::Primary,
                description: "Product entity".to_string(),
                key: Some("product_id".to_string()),
                keys: None,
            }],
            dimensions: vec![Dimension {
                name: "product_id".to_string(),
                dimension_type: DimensionType::String,
                description: Some("Product ID".to_string()),
                expr: "product_id".to_string(),
                samples: None,
                synonyms: None,
            }],
            measures: None,
        };

        // Topic with reachable views
        let valid_topic = Topic {
            name: "sales".to_string(),
            description: "Sales data".to_string(),
            views: vec!["orders".to_string(), "customers".to_string()],
            base_view: Some("orders".to_string()),
            retrieval: None,
            default_filters: None,
        };

        let valid_layer = SemanticLayer {
            views: vec![orders_view.clone(), customers_view.clone()],
            topics: Some(vec![valid_topic]),
            metadata: None,
        };

        let result = valid_layer.validate();
        assert!(
            result.is_valid,
            "Semantic layer with reachable views should pass: {:?}",
            result.errors
        );

        // Topic with unreachable view
        let invalid_topic = Topic {
            name: "sales_with_products".to_string(),
            description: "Sales with products".to_string(),
            views: vec![
                "orders".to_string(),
                "customers".to_string(),
                "products".to_string(),
            ],
            base_view: Some("orders".to_string()),
            retrieval: None,
            default_filters: None,
        };

        let invalid_layer = SemanticLayer {
            views: vec![orders_view, customers_view, products_view],
            topics: Some(vec![invalid_topic]),
            metadata: None,
        };

        let result = invalid_layer.validate();
        assert!(
            !result.is_valid,
            "Semantic layer with unreachable views should fail"
        );
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("not reachable via joins")),
            "Should have error about unreachable views: {:?}",
            result.errors
        );
    }
}
