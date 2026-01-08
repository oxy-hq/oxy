use crate::{config::ConfigManager, config::model::SemanticQueryTask, errors::OxyError};
use oxy_semantic::{SemanticLayer, Topic, View, parse_semantic_layer_from_dir};
use serde_json::Value;
use std::collections::HashSet;

/// Error types specific to semantic query validation
#[derive(Debug, Clone)]
pub enum SemanticQueryError {
    MissingTopic {
        topic: String,
        available: Vec<String>,
    },
    UnknownMeasure {
        field: String,
        topic: String,
        suggestions: Vec<String>,
    },
    UnknownDimension {
        field: String,
        topic: String,
        suggestions: Vec<String>,
    },
    EmptySelection,
    InvalidValueType {
        field: String,
        expected: String,
        actual: String,
    },
    MetadataMissing {
        path: String,
    },
    /// CubeJS returned an error payload or non-success status
    CubeJSError {
        details: String,
    },
    UnsupportedFilters {
        details: String,
    },
    ExecutionFailed {
        details: String,
    },
}

impl std::fmt::Display for SemanticQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SemanticQueryError::MissingTopic { topic, available } => {
                write!(
                    f,
                    "Topic '{}' not found. Available topics: {}",
                    topic,
                    available.join(", ")
                )
            }
            SemanticQueryError::UnknownMeasure {
                field,
                topic,
                suggestions,
            } => {
                let suggestion_text = if suggestions.is_empty() {
                    String::new()
                } else {
                    format!(" Did you mean: {}?", suggestions.join(", "))
                };
                write!(
                    f,
                    "Measure '{}' not found in topic '{}'.{}",
                    field, topic, suggestion_text
                )
            }
            SemanticQueryError::UnknownDimension {
                field,
                topic,
                suggestions,
            } => {
                let suggestion_text = if suggestions.is_empty() {
                    String::new()
                } else {
                    format!(" Did you mean: {}?", suggestions.join(", "))
                };
                write!(
                    f,
                    "Dimension '{}' not found in topic '{}'.{}",
                    field, topic, suggestion_text
                )
            }
            SemanticQueryError::EmptySelection => {
                write!(f, "At least one dimension or measure must be selected")
            }
            SemanticQueryError::InvalidValueType {
                field,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "Field '{}' expects {} but got {}",
                    field, expected, actual
                )
            }
            SemanticQueryError::MetadataMissing { path } => {
                write!(f, "Semantic metadata not found at path: {}", path)
            }
            SemanticQueryError::CubeJSError { details } => {
                write!(f, "CubeJS API error: {}", details)
            }
            SemanticQueryError::UnsupportedFilters { details } => {
                write!(f, "Unsupported filter configuration: {}", details)
            }
            SemanticQueryError::ExecutionFailed { details } => {
                write!(f, "Semantic query execution failed: {}", details)
            }
        }
    }
}

impl From<SemanticQueryError> for OxyError {
    fn from(err: SemanticQueryError) -> Self {
        use SemanticQueryError as SQE;
        match &err {
            // Validation-related errors (user can fix the query)
            SQE::MissingTopic { .. }
            | SQE::UnknownMeasure { .. }
            | SQE::UnknownDimension { .. }
            | SQE::EmptySelection
            | SQE::InvalidValueType { .. }
            | SQE::UnsupportedFilters { .. } => OxyError::ValidationError(err.to_string()),
            // Configuration / environment issues
            SQE::MetadataMissing { .. } => OxyError::ConfigurationError(err.to_string()),
            // Execution / runtime failures
            SQE::CubeJSError { .. } | SQE::ExecutionFailed { .. } => {
                OxyError::RuntimeError(err.to_string())
            }
        }
    }
}

/// Validates a semantic query task against the semantic layer metadata
pub async fn validate_semantic_query_task(
    config: &ConfigManager,
    task: &SemanticQueryTask,
) -> Result<ValidatedSemanticQuery, OxyError> {
    // Load semantic layer metadata from the project's semantics directory
    let semantic_dir = config.semantics_path();

    if !semantic_dir.exists() {
        return Err(SemanticQueryError::MetadataMissing {
            path: semantic_dir.display().to_string(),
        }
        .into());
    }

    let parse_result = parse_semantic_layer_from_dir(&semantic_dir, config.get_globals_registry())
        .map_err(|e| SemanticQueryError::ExecutionFailed {
            details: format!("Failed to parse semantic layer: {}", e),
        })?;

    let semantic_layer = parse_result.semantic_layer;

    // Validate the task against the semantic layer
    validate_task_against_metadata(task, &semantic_layer)
}

/// Holds validated semantic query information
#[derive(Debug, Clone)]
pub struct ValidatedSemanticQuery {
    pub task: SemanticQueryTask,
    pub topic: Topic,
    pub valid_dimensions: HashSet<String>,
    pub valid_measures: HashSet<String>,
    pub views: Vec<View>,
}

/// Result type that can contain either a validated query or a validation error
#[derive(Debug, Clone)]
pub enum SemanticQueryValidation {
    Valid(ValidatedSemanticQuery),
    Invalid {
        task: SemanticQueryTask,
        error: String,
    },
}

/// Validates a semantic query task against loaded semantic layer metadata
fn validate_task_against_metadata(
    task: &SemanticQueryTask,
    semantic_layer: &SemanticLayer,
) -> Result<ValidatedSemanticQuery, OxyError> {
    // Get all available topics
    let empty_topics = Vec::new();
    let topics = semantic_layer.topics.as_ref().unwrap_or(&empty_topics);
    let available_topics: Vec<String> = topics.iter().map(|t| t.name.clone()).collect();

    // Validate topic exists or infer from views
    let topic = if let Some(topic_name) = &task.query.topic {
        topics
            .iter()
            .find(|t| t.name == *topic_name)
            .ok_or_else(|| SemanticQueryError::MissingTopic {
                topic: topic_name.clone(),
                available: available_topics,
            })?
            .clone()
    } else {
        // Infer topic from views referenced in dimensions and measures
        let mut view_names = HashSet::new();

        for dim in &task.query.dimensions {
            if let Some((view, _)) = dim.split_once('.') {
                view_names.insert(view.to_string());
            }
        }

        for measure in &task.query.measures {
            if let Some((view, _)) = measure.split_once('.') {
                view_names.insert(view.to_string());
            }
        }

        if view_names.is_empty() {
            return Err(SemanticQueryError::EmptySelection.into());
        }

        // Verify all referenced views exist in the semantic layer
        for view_name in &view_names {
            if !semantic_layer.views.iter().any(|v| v.name == *view_name) {
                return Err(OxyError::ValidationError(format!(
                    "View '{}' not found in semantic layer",
                    view_name
                )));
            }
        }

        Topic {
            name: "adhoc_query".to_string(),
            description: "Ad-hoc query topic inferred from views".to_string(),
            views: view_names.into_iter().collect(),
            base_view: None,
            retrieval: None,
            default_filters: None,
        }
    };

    // Get all views referenced by this topic
    let topic_views: Vec<View> = semantic_layer
        .views
        .iter()
        .filter(|view| topic.views.contains(&view.name))
        .cloned()
        .collect();

    if topic_views.is_empty() {
        let topic_name = task.query.topic.as_deref().unwrap_or("adhoc_query");
        return Err(SemanticQueryError::ExecutionFailed {
            details: format!("Topic '{}' references no valid views", topic_name),
        }
        .into());
    }

    tracing::debug!(
        "Validating semantic query task for topic '{}', found {} views",
        topic.name,
        topic_views.len()
    );

    // Build valid dimension and measure field sets from metadata (fully-qualified with topic prefix)
    let (valid_dimensions, valid_measures) = build_field_sets(&topic.name, &topic_views);

    tracing::info!(
        "Valid dimensions: {:?}, valid measures: {:?}",
        valid_dimensions,
        valid_measures
    );

    // Validate minimum selection requirement
    if task.query.dimensions.is_empty() && task.query.measures.is_empty() {
        return Err(SemanticQueryError::EmptySelection.into());
    }

    // Validate field references
    validate_field_references(task, &valid_dimensions, &valid_measures, &topic.name)?;

    // Check for duplicate fields and emit warnings
    check_duplicate_fields(task);

    // Validate filters
    validate_filters(task, &valid_dimensions, &valid_measures, &topic.name)?;

    // Validate orders
    validate_orders(task, &valid_dimensions, &valid_measures, &topic.name)?;

    Ok(ValidatedSemanticQuery {
        task: task.clone(),
        topic,
        valid_dimensions,
        valid_measures,
        views: topic_views,
    })
}

/// Builds sets of valid fully-qualified dimension and measure field names from views.
/// Enforces the `topic.field` form so only fully-qualified references are accepted.
fn build_field_sets(_topic_name: &str, views: &[View]) -> (HashSet<String>, HashSet<String>) {
    let mut dimensions = HashSet::new();
    let mut measures = HashSet::new();

    for view in views {
        for dimension in &view.dimensions {
            dimensions.insert(format!("{}.{}", view.name, dimension.name));
        }
        if let Some(view_measures) = &view.measures {
            for measure in view_measures {
                measures.insert(format!("{}.{}", view.name, measure.name));
            }
        }
    }

    (dimensions, measures)
}

/// Validates that all referenced fields exist in the metadata
fn validate_field_references(
    task: &SemanticQueryTask,
    valid_dimensions: &HashSet<String>,
    valid_measures: &HashSet<String>,
    topic: &str,
) -> Result<(), OxyError> {
    // Check dimensions
    for dimension in &task.query.dimensions {
        if !valid_dimensions.contains(dimension) {
            let suggestions = find_suggestions(dimension, valid_dimensions, 5);
            return Err(SemanticQueryError::UnknownDimension {
                field: dimension.to_string(),
                topic: topic.to_string(),
                suggestions,
            }
            .into());
        }
    }

    // Check measures
    for measure in &task.query.measures {
        if !valid_measures.contains(measure) {
            let suggestions = find_suggestions(measure, valid_measures, 5);
            return Err(SemanticQueryError::UnknownMeasure {
                field: measure.to_string(),
                topic: topic.to_string(),
                suggestions,
            }
            .into());
        }
    }

    Ok(())
}

/// Validates filter field references and operator/value compatibility
fn validate_filters(
    task: &SemanticQueryTask,
    valid_dimensions: &HashSet<String>,
    valid_measures: &HashSet<String>,
    topic: &str,
) -> Result<(), OxyError> {
    for filter in &task.query.filters {
        // Check if field exists
        if !valid_dimensions.contains(&filter.field) && !valid_measures.contains(&filter.field) {
            // Determine if this looks more like a dimension or measure based on suggestions
            let dimension_suggestions = find_suggestions(&filter.field, valid_dimensions, 5);
            let measure_suggestions = find_suggestions(&filter.field, valid_measures, 5);

            // Use the error type that has better suggestions, or default to dimension
            if !measure_suggestions.is_empty()
                && (dimension_suggestions.is_empty()
                    || measure_suggestions.len() >= dimension_suggestions.len())
            {
                return Err(SemanticQueryError::UnknownMeasure {
                    field: filter.field.clone(),
                    topic: topic.to_string(),
                    suggestions: measure_suggestions,
                }
                .into());
            } else {
                return Err(SemanticQueryError::UnknownDimension {
                    field: filter.field.clone(),
                    topic: topic.to_string(),
                    suggestions: dimension_suggestions,
                }
                .into());
            }
        }
    }

    Ok(())
}

/// Validates order field references
fn validate_orders(
    task: &SemanticQueryTask,
    valid_dimensions: &HashSet<String>,
    valid_measures: &HashSet<String>,
    topic: &str,
) -> Result<(), OxyError> {
    for order in &task.query.orders {
        // Check if field exists
        if !valid_dimensions.contains(&order.field) && !valid_measures.contains(&order.field) {
            // Determine if this looks more like a dimension or measure based on suggestions
            let dimension_suggestions = find_suggestions(&order.field, valid_dimensions, 5);
            let measure_suggestions = find_suggestions(&order.field, valid_measures, 5);

            // Use the error type that has better suggestions, or default to dimension
            if !measure_suggestions.is_empty()
                && (dimension_suggestions.is_empty()
                    || measure_suggestions.len() >= dimension_suggestions.len())
            {
                return Err(SemanticQueryError::UnknownMeasure {
                    field: order.field.clone(),
                    topic: topic.to_string(),
                    suggestions: measure_suggestions,
                }
                .into());
            } else {
                return Err(SemanticQueryError::UnknownDimension {
                    field: order.field.clone(),
                    topic: topic.to_string(),
                    suggestions: dimension_suggestions,
                }
                .into());
            }
        }
    }

    Ok(())
}

/// Extracts the view name from a fully-qualified field (e.g., "orders.total" -> "orders")
#[allow(dead_code)]
fn extract_view_from_field(field: &str) -> Option<String> {
    field.split('.').next().map(|s| s.to_string())
}

/// Gets a human-readable name for a JSON value type
fn _get_value_type_name(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(_) => "boolean".to_string(),
        Value::Number(_) => "number".to_string(),
        Value::String(_) => "string".to_string(),
        Value::Array(_) => "array".to_string(),
        Value::Object(_) => "object".to_string(),
    }
}

/// Checks for duplicate fields in dimensions and measures and emits warnings
fn check_duplicate_fields(task: &SemanticQueryTask) {
    let mut field_counts = std::collections::HashMap::new();

    // Count dimension occurrences
    for dimension in &task.query.dimensions {
        *field_counts.entry(dimension.clone()).or_insert(0) += 1;
    }

    // Count measure occurrences
    for measure in &task.query.measures {
        *field_counts.entry(measure.clone()).or_insert(0) += 1;
    }

    // Log warnings for duplicates
    for (field, count) in field_counts {
        if count > 1 {
            log::warn!(
                "Duplicate field '{}' appears {} times in semantic query dimensions and measures",
                field,
                count
            );
        }
    }

    // Check for duplicate filter fields
    let mut filter_fields = std::collections::HashMap::new();
    for filter in &task.query.filters {
        *filter_fields.entry(filter.field.clone()).or_insert(0) += 1;
    }

    for (field, count) in filter_fields {
        if count > 1 {
            log::warn!(
                "Duplicate filter field '{}' appears {} times (filters will be combined with AND logic)",
                field,
                count
            );
        }
    }

    // Check for duplicate order fields
    let mut order_fields = std::collections::HashMap::new();
    for order in &task.query.orders {
        *order_fields.entry(order.field.clone()).or_insert(0) += 1;
    }

    for (field, count) in order_fields {
        if count > 1 {
            log::warn!(
                "Duplicate order field '{}' appears {} times (only the last order will be applied)",
                field,
                count
            );
        }
    }
}

/// Finds field name suggestions using simple string distance heuristics
fn find_suggestions(
    field: &str,
    valid_fields: &HashSet<String>,
    max_suggestions: usize,
) -> Vec<String> {
    let mut suggestions: Vec<(String, usize)> = valid_fields
        .iter()
        .map(|valid_field| {
            let distance = levenshtein_distance(field, valid_field);
            (valid_field.clone(), distance)
        })
        .collect();

    // Sort by distance and take the closest matches
    suggestions.sort_by_key(|(_, distance)| *distance);

    suggestions
        .into_iter()
        .filter(|(_, distance)| *distance <= 3) // Only suggest if reasonably close
        .take(max_suggestions)
        .map(|(field, _)| field)
        .collect()
}

/// Simple Levenshtein distance implementation
fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let s1_chars: Vec<char> = s1.chars().collect();
    let s2_chars: Vec<char> = s2.chars().collect();
    let s1_len = s1_chars.len();
    let s2_len = s2_chars.len();

    if s1_len == 0 {
        return s2_len;
    }
    if s2_len == 0 {
        return s1_len;
    }

    let mut prev_row: Vec<usize> = (0..=s2_len).collect();
    let mut curr_row = vec![0; s2_len + 1];

    for i in 1..=s1_len {
        curr_row[0] = i;

        for j in 1..=s2_len {
            let cost = if s1_chars[i - 1] == s2_chars[j - 1] {
                0
            } else {
                1
            };
            curr_row[j] = std::cmp::min(
                std::cmp::min(prev_row[j] + 1, curr_row[j - 1] + 1),
                prev_row[j - 1] + cost,
            );
        }

        std::mem::swap(&mut prev_row, &mut curr_row);
    }

    prev_row[s2_len]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::model::SemanticQueryTask, service::types::SemanticQueryParams};
    use oxy_semantic::Topic;

    fn create_test_topic(name: &str, base_view: Option<String>) -> Topic {
        Topic {
            name: name.to_string(),
            description: "Test topic".to_string(),
            views: vec!["orders".to_string(), "customers".to_string()],
            base_view,
            retrieval: None,
            default_filters: None,
        }
    }

    fn create_test_task(
        topic: &str,
        dimensions: Vec<&str>,
        measures: Vec<&str>,
    ) -> SemanticQueryTask {
        SemanticQueryTask {
            query: SemanticQueryParams {
                topic: Some(topic.to_string()),
                dimensions: dimensions.iter().map(|d| d.to_string()).collect(),
                measures: measures.iter().map(|m| m.to_string()).collect(),
                filters: vec![],
                orders: vec![],
                limit: None,
                offset: None,
                variables: None,
            },
            export: None,
            variables: None,
        }
    }

    #[test]
    fn test_extract_view_from_field() {
        assert_eq!(
            extract_view_from_field("orders.total"),
            Some("orders".to_string())
        );
        assert_eq!(
            extract_view_from_field("customers.name"),
            Some("customers".to_string())
        );
        assert_eq!(
            extract_view_from_field("simple_field"),
            Some("simple_field".to_string())
        );
    }
}
