// Essential semantic model functionality
pub mod entity_graph;
pub mod errors;
pub mod metric_tree;
pub mod models;
pub mod parser;
pub mod validation;
pub mod variables;

// Re-export the most commonly used types
pub use errors::SemanticLayerError;
pub use metric_tree::{
    LeverConflict, build as build_metric_tree, lever_conflicts, predict, predict_with_values,
    sensitivity, subtree,
};
pub use models::{
    Dimension, DimensionType, Driver, DriverConfidence, DriverDirection, DriverForm,
    DriverStrength, Entity, EntityType, Measure, MeasureFilter, MeasureType, SemanticLayer, Topic,
    TopicArrayFilter, TopicDateRangeFilter, TopicFilter, TopicFilterType, TopicScalarFilter, View,
};
pub use parser::{ParseResult, ParserConfig, SemanticLayerParser, parse_semantic_layer_from_dir};
pub use validation::{SemanticValidator, ValidationResult, validate_semantic_layer};
pub use variables::{VariableEncoder, VariableError};
