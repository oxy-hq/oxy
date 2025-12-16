use arrow::record_batch::RecordBatch;
use std::sync::Arc;

use crate::execute::types::Table;

use super::{
    column_analysis::DefaultColumnAnalyzer,
    generators::{BarChartGenerator, LineChartGenerator, PieChartGenerator},
    registry::ChartGeneratorRegistry,
    traits::{ChartOptionsGenerator, ColumnAnalyzer, ScoringStrategy},
    types::{AnalysisContext, ChartRecommendation, ColumnInfo},
};

pub struct ChartHeuristicsAnalyzer {
    column_analyzer: Arc<dyn ColumnAnalyzer>,
    generator_registry: ChartGeneratorRegistry,
}

impl ChartHeuristicsAnalyzer {
    /// Analyze a RecordBatch and generate context
    pub fn analyze(&self, batch: &RecordBatch, data_reference: &str) -> AnalysisContext {
        let row_count = batch.num_rows();
        let schema = batch.schema();

        let columns: Vec<ColumnInfo> = (0..batch.num_columns())
            .map(|i| {
                let field = schema.field(i);
                let array = batch.column(i);
                self.column_analyzer
                    .analyze(field.name(), field.data_type(), array.as_ref())
            })
            .collect();

        AnalysisContext {
            columns,
            row_count,
            data_reference: data_reference.to_string(),
        }
    }

    /// Analyze and generate all recommendations
    pub fn generate_recommendations(
        &self,
        batch: &RecordBatch,
        data_reference: &str,
    ) -> Vec<ChartRecommendation> {
        let context = self.analyze(batch, data_reference);
        self.generator_registry.generate_all(&context)
    }

    /// Get top N recommendations
    pub fn top_recommendations(
        &self,
        tables: &[&Table], // Batches from multiple tables
        n: usize,
    ) -> Vec<ChartRecommendation> {
        let mut all_recommendations = Vec::new();
        for table in tables.iter() {
            if let Ok(batch) = table.sample() {
                let data_ref = &table.name;
                let mut recs = self
                    .generate_recommendations(batch, data_ref)
                    .into_iter()
                    .take(n)
                    .collect();
                all_recommendations.append(&mut recs);
            }
        }
        tracing::info!(
            "Total recommendations before sorting: {}",
            all_recommendations.len()
        );
        all_recommendations.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        all_recommendations.into_iter().take(n).collect()
    }
}

pub struct ChartHeuristicsAnalyzerBuilder {
    column_analyzer: Option<Arc<dyn ColumnAnalyzer>>,
    generators: Vec<Arc<dyn ChartOptionsGenerator>>,
    scoring_strategy: Option<Arc<dyn ScoringStrategy>>,
}

impl ChartHeuristicsAnalyzerBuilder {
    pub fn new() -> Self {
        Self {
            column_analyzer: None,
            generators: Vec::new(),
            scoring_strategy: None,
        }
    }

    /// Add default line chart generator
    pub fn with_line_charts(mut self) -> Self {
        let scoring = self.get_scoring();
        self.generators
            .push(Arc::new(LineChartGenerator::new(scoring)));
        self
    }

    /// Add default bar chart generator
    pub fn with_bar_charts(mut self) -> Self {
        let scoring = self.get_scoring();
        self.generators
            .push(Arc::new(BarChartGenerator::new(scoring)));
        self
    }

    /// Add default pie chart generator
    pub fn with_pie_charts(mut self) -> Self {
        let scoring = self.get_scoring();
        self.generators
            .push(Arc::new(PieChartGenerator::new(scoring)));
        self
    }

    /// Add all default generators
    pub fn with_all_defaults(self) -> Self {
        self.with_line_charts().with_bar_charts().with_pie_charts()
    }

    fn get_scoring(&self) -> Arc<dyn ScoringStrategy> {
        self.scoring_strategy
            .clone()
            .unwrap_or_else(|| Arc::new(super::scoring::DefaultScoringStrategy))
    }

    /// Build the analyzer
    pub fn build(self) -> ChartHeuristicsAnalyzer {
        let column_analyzer = self
            .column_analyzer
            .unwrap_or_else(|| Arc::new(DefaultColumnAnalyzer));

        let mut registry = ChartGeneratorRegistry::new();
        for generator in self.generators {
            registry.register(generator);
        }

        if registry.generators().is_empty() {
            registry = ChartGeneratorRegistry::with_defaults();
        }

        ChartHeuristicsAnalyzer {
            column_analyzer,
            generator_registry: registry,
        }
    }
}

impl Default for ChartHeuristicsAnalyzerBuilder {
    fn default() -> Self {
        Self::new()
    }
}
