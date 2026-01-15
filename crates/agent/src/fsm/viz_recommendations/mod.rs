// Module declarations
pub mod analyzer;
pub mod column_analysis;
pub mod generators;
pub mod generators_bar;
pub mod generators_line;
pub mod generators_pie;
pub mod openai_schema;
pub mod registry;
pub mod scoring;
pub mod traits;
pub mod types;

pub use analyzer::ChartHeuristicsAnalyzerBuilder;
pub use openai_schema::{ChartResponseParser, ChartSelectionSchema};
