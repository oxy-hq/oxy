use std::sync::Arc;

use crate::agent::builders::fsm::viz::recommendations::{
    traits::{ChartOptionsGenerator, ScoringStrategy},
    types::{AnalysisContext, ChartDisplay, ChartRecommendation, ChartType, ScoringContext},
};
use crate::config::model::LineChartDisplay;

use super::super::scoring::DefaultScoringStrategy;

pub struct LineChartGenerator {
    scoring: Arc<dyn ScoringStrategy>,
}

impl LineChartGenerator {
    pub fn new(scoring: Arc<dyn ScoringStrategy>) -> Self {
        Self { scoring }
    }

    pub fn with_default_scoring() -> Self {
        Self::new(Arc::new(DefaultScoringStrategy))
    }

    fn generate_title(x_col: &str, y_col: &str, series: Option<&str>) -> String {
        match series {
            Some(s) => format!("{} by {} (grouped by {})", y_col, x_col, s),
            None => format!("{} over {}", y_col, x_col),
        }
    }
}

impl ChartOptionsGenerator for LineChartGenerator {
    fn generate(&self, context: &AnalysisContext) -> Vec<ChartRecommendation> {
        let mut recommendations = Vec::new();

        let numeric_cols = context.numeric_columns();
        let temporal_cols = context.temporal_columns();
        let categorical_cols = context.categorical_columns();

        // Time series line charts
        for temp_col in &temporal_cols {
            for num_col in &numeric_cols {
                let scoring_ctx = ScoringContext {
                    row_count: context.row_count,
                    cardinality: None,
                    has_temporal: true,
                    has_grouping: false,
                    num_series: 1,
                };

                let score = self.scoring.score(&ChartType::Line, &scoring_ctx);

                recommendations.push(ChartRecommendation {
                    chart_type: ChartType::Line,
                    score,
                    rationale: format!("Time series: {} over {}", num_col.name, temp_col.name),
                    display: ChartDisplay::Line(LineChartDisplay {
                        x: temp_col.name.clone(),
                        y: num_col.name.clone(),
                        x_axis_label: Some(temp_col.name.clone()),
                        y_axis_label: Some(num_col.name.clone()),
                        data: context.data_reference.clone(),
                        series: None,
                        title: Some(Self::generate_title(&temp_col.name, &num_col.name, None)),
                    }),
                });

                // Grouped time series
                for cat_col in &categorical_cols {
                    if cat_col.cardinality().unwrap_or(0) <= 10 {
                        let scoring_ctx = ScoringContext {
                            row_count: context.row_count,
                            cardinality: cat_col.cardinality(),
                            has_temporal: true,
                            has_grouping: true,
                            num_series: cat_col.cardinality().unwrap_or(1),
                        };

                        let score = self.scoring.score(&ChartType::Line, &scoring_ctx);

                        recommendations.push(ChartRecommendation {
                            chart_type: ChartType::Line,
                            score: score - 0.05,
                            rationale: format!(
                                "Grouped time series: {} over {}, by {}",
                                num_col.name, temp_col.name, cat_col.name
                            ),
                            display: ChartDisplay::Line(LineChartDisplay {
                                x: temp_col.name.clone(),
                                y: num_col.name.clone(),
                                x_axis_label: Some(temp_col.name.clone()),
                                y_axis_label: Some(num_col.name.clone()),
                                data: context.data_reference.clone(),
                                series: Some(cat_col.name.clone()),
                                title: Some(Self::generate_title(
                                    &temp_col.name,
                                    &num_col.name,
                                    Some(&cat_col.name),
                                )),
                            }),
                        });
                    }
                }
            }
        }

        // Numeric X with Numeric Y (sequential data)
        if numeric_cols.len() >= 2 {
            for (i, x_col) in numeric_cols.iter().enumerate() {
                for y_col in numeric_cols.iter().skip(i + 1) {
                    let is_sequential =
                        x_col.unique_count >= (context.row_count as f64 * 0.8) as usize;

                    if is_sequential {
                        recommendations.push(ChartRecommendation {
                            chart_type: ChartType::Line,
                            score: 0.75,
                            rationale: format!("Continuous: {} vs {}", y_col.name, x_col.name),
                            display: ChartDisplay::Line(LineChartDisplay {
                                x: x_col.name.clone(),
                                y: y_col.name.clone(),
                                x_axis_label: Some(x_col.name.clone()),
                                y_axis_label: Some(y_col.name.clone()),
                                data: context.data_reference.clone(),
                                series: None,
                                title: Some(format!("{} vs {}", y_col.name, x_col.name)),
                            }),
                        });
                    }
                }
            }
        }

        // Categorical X with Numeric Y (trend across categories)
        for cat_col in &categorical_cols {
            let cardinality = cat_col.cardinality().unwrap_or(0);
            if cardinality >= 4 && cardinality <= 20 {
                for num_col in &numeric_cols {
                    recommendations.push(ChartRecommendation {
                        chart_type: ChartType::Line,
                        score: 0.70,
                        rationale: format!("Trend: {} across {}", num_col.name, cat_col.name),
                        display: ChartDisplay::Line(LineChartDisplay {
                            x: cat_col.name.clone(),
                            y: num_col.name.clone(),
                            x_axis_label: Some(cat_col.name.clone()),
                            y_axis_label: Some(num_col.name.clone()),
                            data: context.data_reference.clone(),
                            series: None,
                            title: Some(format!("{} by {}", num_col.name, cat_col.name)),
                        }),
                    });
                }
            }
        }

        recommendations
    }
}
