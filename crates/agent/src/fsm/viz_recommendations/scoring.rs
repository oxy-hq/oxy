use super::traits::ScoringStrategy;
use super::types::{ChartType, ScoringContext};

pub struct DefaultScoringStrategy;

impl ScoringStrategy for DefaultScoringStrategy {
    fn score(&self, chart_type: &ChartType, context: &ScoringContext) -> f64 {
        let mut score: f64 = 0.5;

        match chart_type {
            ChartType::Line => {
                if context.has_temporal {
                    score += 0.4;
                }
                if context.row_count > 10 {
                    score += 0.05;
                }
                if context.has_grouping && context.cardinality.unwrap_or(0) <= 10 {
                    score += 0.05;
                }
            }
            ChartType::Bar => {
                if let Some(card) = context.cardinality {
                    if card <= 10 {
                        score += 0.35;
                    } else if card <= 20 {
                        score += 0.20;
                    }
                }
                if context.has_grouping {
                    score += 0.05;
                }
            }
            ChartType::Pie => {
                if let Some(card) = context.cardinality {
                    if (3..=6).contains(&card) {
                        score += 0.35;
                    } else if card == 2 || (7..=12).contains(&card) {
                        score += 0.20;
                    }
                }
            }
        }

        score.min(1.0)
    }
}
