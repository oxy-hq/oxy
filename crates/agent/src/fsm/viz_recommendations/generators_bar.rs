use std::sync::Arc;

use crate::fsm::viz_recommendations::{
    traits::{ChartOptionsGenerator, ScoringStrategy},
    types::{AnalysisContext, ChartDisplay, ChartRecommendation, ChartType, ScoringContext},
};
use oxy::config::model::BarChartDisplay;

use super::scoring::DefaultScoringStrategy;

pub struct BarChartGenerator {
    scoring: Arc<dyn ScoringStrategy>,
}

impl BarChartGenerator {
    pub fn new(scoring: Arc<dyn ScoringStrategy>) -> Self {
        Self { scoring }
    }

    pub fn with_default_scoring() -> Self {
        Self::new(Arc::new(DefaultScoringStrategy))
    }

    fn generate_title(x_col: &str, y_col: &str, series: Option<&str>) -> String {
        match series {
            Some(s) => format!("{} by {} (grouped by {})", y_col, x_col, s),
            None => format!("{} by {}", y_col, x_col),
        }
    }
}

impl ChartOptionsGenerator for BarChartGenerator {
    fn generate(&self, context: &AnalysisContext) -> Vec<ChartRecommendation> {
        let mut recommendations = Vec::new();

        let numeric_cols = context.numeric_columns();
        let categorical_cols = context.categorical_columns();

        // Basic categorical bar charts
        for cat_col in &categorical_cols {
            let cardinality = cat_col.cardinality().unwrap_or(0);

            for num_col in &numeric_cols {
                let scoring_ctx = ScoringContext {
                    row_count: context.row_count,
                    cardinality: Some(cardinality),
                    has_temporal: false,
                    has_grouping: false,
                    num_series: 1,
                };

                let score = self.scoring.score(&ChartType::Bar, &scoring_ctx);

                recommendations.push(ChartRecommendation {
                    chart_type: ChartType::Bar,
                    score,
                    rationale: format!(
                        "Compare {} across {} categories",
                        num_col.name, cardinality
                    ),
                    display: ChartDisplay::Bar(BarChartDisplay {
                        x: cat_col.name.clone(),
                        y: num_col.name.clone(),
                        title: Some(Self::generate_title(&cat_col.name, &num_col.name, None)),
                        data: context.data_reference.clone(),
                        series: None,
                    }),
                });
            }
        }

        // Grouped bar charts (two categorical + one numeric)
        if categorical_cols.len() >= 2 && !numeric_cols.is_empty() {
            for (i, cat1) in categorical_cols.iter().enumerate() {
                for cat2 in categorical_cols.iter().skip(i + 1) {
                    let card1 = cat1.cardinality().unwrap_or(0);
                    let card2 = cat2.cardinality().unwrap_or(0);

                    let (x_col, group_col) = if card1 >= card2 {
                        (cat1, cat2)
                    } else {
                        (cat2, cat1)
                    };

                    let group_card = group_col.cardinality().unwrap_or(0);

                    if group_card <= 8 {
                        for num_col in &numeric_cols {
                            let scoring_ctx = ScoringContext {
                                row_count: context.row_count,
                                cardinality: x_col.cardinality(),
                                has_temporal: false,
                                has_grouping: true,
                                num_series: group_card,
                            };

                            let score = self.scoring.score(&ChartType::Bar, &scoring_ctx);

                            recommendations.push(ChartRecommendation {
                                chart_type: ChartType::Bar,
                                score,
                                rationale: format!(
                                    "Grouped: {} by {}, grouped by {}",
                                    num_col.name, x_col.name, group_col.name
                                ),
                                display: ChartDisplay::Bar(BarChartDisplay {
                                    x: x_col.name.clone(),
                                    y: num_col.name.clone(),
                                    title: Some(Self::generate_title(
                                        &x_col.name,
                                        &num_col.name,
                                        Some(&group_col.name),
                                    )),
                                    data: context.data_reference.clone(),
                                    series: Some(group_col.name.clone()),
                                }),
                            });
                        }
                    }
                }
            }
        }

        // Boolean bar charts
        for bool_col in context.boolean_columns() {
            for num_col in &numeric_cols {
                recommendations.push(ChartRecommendation {
                    chart_type: ChartType::Bar,
                    score: 0.70,
                    rationale: format!("Binary comparison: {} by {}", num_col.name, bool_col.name),
                    display: ChartDisplay::Bar(BarChartDisplay {
                        x: bool_col.name.clone(),
                        y: num_col.name.clone(),
                        title: Some(format!("{} by {}", num_col.name, bool_col.name)),
                        data: context.data_reference.clone(),
                        series: None,
                    }),
                });
            }
        }

        // Temporal bar charts (aggregated by time period)
        for temp_col in context.temporal_columns() {
            for num_col in &numeric_cols {
                recommendations.push(ChartRecommendation {
                    chart_type: ChartType::Bar,
                    score: 0.75,
                    rationale: format!("Temporal bars: {} by {}", num_col.name, temp_col.name),
                    display: ChartDisplay::Bar(BarChartDisplay {
                        x: temp_col.name.clone(),
                        y: num_col.name.clone(),
                        title: Some(format!("{} by {}", num_col.name, temp_col.name)),
                        data: context.data_reference.clone(),
                        series: None,
                    }),
                });
            }
        }

        recommendations
    }
}
