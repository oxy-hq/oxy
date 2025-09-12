// Essential semantic layer functionality
pub mod builder;
pub mod cube;
pub mod errors;
pub mod models;
pub mod parser;
pub mod types;
pub mod validation;

// Re-export the most commonly used types
pub use builder::{
    DimensionBuilder, EntityBuilder, MeasureBuilder, SemanticLayerBuilder, TopicBuilder,
    ViewBuilder,
};
pub use errors::SemanticLayerError;
pub use models::{
    AccessLevel, Dimension, DimensionType, Entity, EntityType, Measure, MeasureFilter, MeasureType,
    SemanticLayer, SemanticTableRef, Topic, View,
};
pub use parser::{ParseResult, ParserConfig, SemanticLayerParser, parse_semantic_layer_from_dir};
pub use types::SyncMetrics;
pub use validation::{SemanticValidator, ValidationResult, validate_semantic_layer};
