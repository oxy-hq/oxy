// Essential semantic layer functionality
pub mod build_manifest;
pub mod builder;
pub mod change_detector;
pub mod cube;
pub mod errors;
pub mod models;
pub mod parser;
pub mod types;
pub mod validation;
pub mod variables;

// Re-export the most commonly used types
pub use build_manifest::{BuildManifest, hash_file, hash_string};
pub use builder::{
    DimensionBuilder, EntityBuilder, MeasureBuilder, SemanticLayerBuilder, TopicBuilder,
    ViewBuilder,
};
pub use change_detector::{
    ChangeDetectionResult, ChangeDetector, hash_database_config, hash_globals_registry,
};
pub use errors::SemanticLayerError;
pub use models::{
    AccessLevel, Dimension, DimensionType, Entity, EntityType, Measure, MeasureFilter, MeasureType,
    SemanticLayer, SemanticTableRef, Topic, TopicArrayFilter, TopicDateRangeFilter, TopicFilter,
    TopicFilterType, TopicScalarFilter, View,
};
pub use parser::{ParseResult, ParserConfig, SemanticLayerParser, parse_semantic_layer_from_dir};
pub use types::SyncMetrics;
pub use validation::{SemanticValidator, ValidationResult, validate_semantic_layer};
pub use variables::{VariableEncoder, VariableError, VariableMapping};
