use arrow::array::Array;
use arrow::datatypes::DataType;

use super::types::{AnalysisContext, ChartRecommendation, ChartType, ColumnInfo, ScoringContext};

pub trait ChartOptionsGenerator: Send + Sync {
    /// Generate recommendations based on analyzed column data
    fn generate(&self, context: &AnalysisContext) -> Vec<ChartRecommendation>;
}

/// Trait for column analysis strategy - allows custom column classification
pub trait ColumnAnalyzer: Send + Sync {
    fn analyze(&self, name: &str, data_type: &DataType, array: &dyn Array) -> ColumnInfo;
}

/// Trait for scoring chart recommendations
pub trait ScoringStrategy: Send + Sync {
    fn score(&self, chart_type: &ChartType, context: &ScoringContext) -> f64;
}
