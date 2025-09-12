use crate::SemanticLayerError;
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

        // Validate key
        if self.key.is_empty() {
            result.add_error("Entity key cannot be empty".to_string());
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
        if let Some(ref description) = self.description {
            if description.is_empty() {
                result.add_error("Dimension description cannot be empty when provided".to_string());
            }
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
        if let Some(ref description) = self.description {
            if description.is_empty() {
                result.add_error("Measure description cannot be empty when provided".to_string());
            }
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
                // Custom measures require either expr or sql
                if self.expr.is_none() && self.sql.is_none() {
                    result.add_error(
                        "Custom measures require either 'expr' or 'sql' field".to_string(),
                    );
                }
                if self.expr.is_some() && self.sql.is_some() {
                    result.add_warning(
                        "Custom measures should specify either 'expr' or 'sql', not both"
                            .to_string(),
                    );
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
            let mut primary_key_count = 0;

            for dimension in &self.dimensions {
                // Check for duplicate dimension names
                if !dimension_names.insert(&dimension.name) {
                    result.add_error(format!(
                        "Duplicate dimension name '{}' found",
                        dimension.name
                    ));
                }

                // Count primary keys
                if dimension.primary_key.unwrap_or(false) {
                    primary_key_count += 1;
                }

                // Validate individual dimension
                result.merge(dimension.validate());
            }

            // Validate primary key count
            if primary_key_count == 0 {
                result
                    .add_warning("View should have at least one primary key dimension".to_string());
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
