//! Validation rules for the **Execute** stage.
//!
//! | Rule | Name |
//! |---|---|
//! | [`NonEmptyRule`]            | `non_empty` |
//! | [`ShapeMatchRule`]          | `shape_match` |
//! | [`NoNanInfRule`]            | `no_nan_inf` |
//! | [`OutlierDetectionRule`]    | `outlier_detection` |
//! | [`TimeseriesDateCheckRule`] | `timeseries_date_check` |
//! | [`TruncationWarningRule`]   | `truncation_warning` |
//! | [`NullRatioCheckRule`]      | `null_ratio_check` |
//! | [`DuplicateRowCheckRule`]   | `duplicate_row_check` |
//!
//! Implementation: [`helpers`] holds the shared predicates; [`rules`] holds the
//! eight `SolvedRule` implementations.

use crate::{AnalyticsError, AnalyticsResult, QuerySpec};

use super::rule::{SolvedCtx, SolvedRule};

pub mod helpers;
pub mod rules;

#[cfg(test)]
mod tests;

pub use rules::{
    DuplicateRowCheckRule, NoNanInfRule, NonEmptyRule, NullRatioCheckRule, OutlierDetectionRule,
    ShapeMatchRule, TimeseriesDateCheckRule, TruncationWarningRule,
};

#[cfg(test)]
pub(super) use helpers::{infer_shape, looks_like_date};

// ---------------------------------------------------------------------------
// Backward-compatible free function
// ---------------------------------------------------------------------------

/// Validate that the results of an executed query are non-empty, match the
/// expected shape, and contain plausible numeric values.
#[allow(dead_code)]
pub fn validate_solved(result: &AnalyticsResult, spec: &QuerySpec) -> Result<(), AnalyticsError> {
    let ctx = SolvedCtx { result, spec };
    // Order matters: structural checks first, then value-level checks.
    // Structural errors route to Solve; ValueAnomaly routes to Interpret.
    NonEmptyRule.check(&ctx)?;
    TruncationWarningRule.check(&ctx)?;
    NoNanInfRule.check(&ctx)?;
    OutlierDetectionRule {
        threshold_sigma: 5.0,
        min_rows: 4,
    }
    .check(&ctx)?;
    NullRatioCheckRule { threshold: 0.5 }.check(&ctx)?;
    DuplicateRowCheckRule {
        max_duplicate_ratio: 0.1,
    }
    .check(&ctx)?;
    Ok(())
}
