use std::sync::Arc;

use crate::agent::builders::fsm::viz::recommendations::{
    traits::{ChartOptionsGenerator, ScoringStrategy},
    types::{
        AnalysisContext, ChartDisplay, ChartRecommendation, ChartType, ColumnInfo, ScoringContext,
    },
};
use crate::config::model::PieChartDisplay;

use super::super::scoring::DefaultScoringStrategy;

pub struct PieChartGenerator {
    scoring: Arc<dyn ScoringStrategy>,
}

impl PieChartGenerator {
    pub fn new(scoring: Arc<dyn ScoringStrategy>) -> Self {
        Self { scoring }
    }

    pub fn with_default_scoring() -> Self {
        Self::new(Arc::new(DefaultScoringStrategy))
    }

    fn generate_title(name_col: &str, value_col: &str) -> String {
        format!("{} distribution by {}", value_col, name_col)
    }
}

impl ChartOptionsGenerator for PieChartGenerator {
    fn generate(&self, context: &AnalysisContext) -> Vec<ChartRecommendation> {
        let mut recommendations = Vec::new();

        let numeric_cols: Vec<&ColumnInfo> = context
            .numeric_columns()
            .into_iter()
            .filter(|c| c.all_positive())
            .collect();

        let categorical_cols = context.categorical_columns();

        // Categorical pie charts
        for cat_col in &categorical_cols {
            let cardinality = cat_col.cardinality().unwrap_or(0);

            if !(2..=12).contains(&cardinality) {
                continue;
            }

            for num_col in &numeric_cols {
                let scoring_ctx = ScoringContext {
                    row_count: context.row_count,
                    cardinality: Some(cardinality),
                    has_temporal: false,
                    has_grouping: false,
                    num_series: 1,
                };

                let score = self.scoring.score(&ChartType::Pie, &scoring_ctx);

                recommendations.push(ChartRecommendation {
                    chart_type: ChartType::Pie,
                    score,
                    rationale: format!(
                        "Proportions: {} across {} categories",
                        num_col.name, cardinality
                    ),
                    display: ChartDisplay::Pie(PieChartDisplay {
                        name: cat_col.name.clone(),
                        value: num_col.name.clone(),
                        title: Some(Self::generate_title(&cat_col.name, &num_col.name)),
                        data: context.data_reference.clone(),
                    }),
                });
            }
        }

        // Boolean pie charts
        for bool_col in context.boolean_columns() {
            for num_col in &numeric_cols {
                recommendations.push(ChartRecommendation {
                    chart_type: ChartType::Pie,
                    score: 0.65,
                    rationale: format!("Binary split: {} by {}", num_col.name, bool_col.name),
                    display: ChartDisplay::Pie(PieChartDisplay {
                        name: bool_col.name.clone(),
                        value: num_col.name.clone(),
                        title: Some(format!("{} by {}", num_col.name, bool_col.name)),
                        data: context.data_reference.clone(),
                    }),
                });
            }
        }

        // Low-cardinality numeric as categorical
        for num_col in &numeric_cols {
            if num_col.unique_count >= 2 && num_col.unique_count <= 8 {
                for value_col in &numeric_cols {
                    if value_col.name != num_col.name {
                        recommendations.push(ChartRecommendation {
                            chart_type: ChartType::Pie,
                            score: 0.55,
                            rationale: format!(
                                "Discrete groups: {} by {} ({} values)",
                                value_col.name, num_col.name, num_col.unique_count
                            ),
                            display: ChartDisplay::Pie(PieChartDisplay {
                                name: num_col.name.clone(),
                                value: value_col.name.clone(),
                                title: Some(Self::generate_title(&num_col.name, &value_col.name)),
                                data: context.data_reference.clone(),
                            }),
                        });
                    }
                }
            }
        }

        recommendations
    }
}
