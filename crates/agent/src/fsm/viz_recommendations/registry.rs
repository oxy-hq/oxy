use std::sync::Arc;

use super::{
    generators::{BarChartGenerator, LineChartGenerator, PieChartGenerator},
    traits::ChartOptionsGenerator,
    types::{AnalysisContext, ChartRecommendation},
};

pub struct ChartGeneratorRegistry {
    generators: Vec<Arc<dyn ChartOptionsGenerator>>,
}

impl ChartGeneratorRegistry {
    pub fn new() -> Self {
        Self {
            generators: Vec::new(),
        }
    }

    /// Create registry with default generators
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(LineChartGenerator::with_default_scoring()));
        registry.register(Arc::new(BarChartGenerator::with_default_scoring()));
        registry.register(Arc::new(PieChartGenerator::with_default_scoring()));
        registry
    }

    /// Register a chart generator
    pub fn register(&mut self, generator: Arc<dyn ChartOptionsGenerator>) {
        self.generators.push(generator);
    }

    /// Get all registered generators
    pub fn generators(&self) -> &[Arc<dyn ChartOptionsGenerator>] {
        &self.generators
    }

    /// Generate all recommendations from all registered generators
    pub fn generate_all(&self, context: &AnalysisContext) -> Vec<ChartRecommendation> {
        let mut all_recommendations: Vec<ChartRecommendation> = self
            .generators
            .iter()
            .flat_map(|g| g.generate(context))
            .collect();

        all_recommendations.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        all_recommendations
    }
}

impl Default for ChartGeneratorRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}
