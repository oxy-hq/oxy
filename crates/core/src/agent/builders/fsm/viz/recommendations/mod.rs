// Module declarations
mod analyzer;
mod column_analysis;
mod generators;
mod openai_schema;
mod registry;
mod scoring;
mod traits;
mod types;

pub use analyzer::{ChartHeuristicsAnalyzer, ChartHeuristicsAnalyzerBuilder};
pub use column_analysis::DefaultColumnAnalyzer;
pub use generators::{BarChartGenerator, LineChartGenerator, PieChartGenerator};
pub use openai_schema::{ChartResponseParser, ChartSelectionSchema};
pub use registry::ChartGeneratorRegistry;
pub use scoring::DefaultScoringStrategy;
pub use traits::{ChartOptionsGenerator, ColumnAnalyzer, ScoringStrategy};
pub use types::{
    AnalysisContext, ChartDisplay, ChartRecommendation, ChartType, ColumnInfo, ColumnKind,
    ScoringContext,
};
