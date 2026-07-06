//! Pure drift comparison: two numbers + a tolerance → a `DriftVerdict`.
//! No I/O, no DB — mirrors the `evaluator` purity split so it is unit-testable.

use serde::{Deserialize, Serialize};

use crate::server::api::admin::workspace_health::evaluator::HealthStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Combinator {
    And,
    Or,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tolerance {
    pub abs: f64,
    pub pct: f64,
    #[serde(default = "default_combinator")]
    pub combinator: Combinator,
}

fn default_combinator() -> Combinator {
    Combinator::And
}

/// Resolved presentation fields for a check, threaded through every verdict
/// constructor so the comparison and the degraded paths echo the same labels.
/// Labels are already defaulted by the caller ("Actual" / "Expected").
#[derive(Debug, Clone)]
pub struct VerdictMeta {
    pub check: String,
    pub description: Option<String>,
    pub actual_label: String,
    pub expected_label: String,
}

/// One reconciliation outcome, persisted and surfaced in the health payload.
#[derive(Debug, Clone, Serialize)]
pub struct DriftVerdict {
    /// Stable machine id.
    pub check: String,
    /// Friendly text echoed from the check (if any).
    pub description: Option<String>,
    /// Resolved actual-side label ("Actual" when unset).
    pub actual_label: String,
    /// Resolved expected-side label ("Expected" when unset).
    pub expected_label: String,
    pub actual: f64,
    pub expected: f64,
    pub abs_diff: f64,
    /// Percent drift relative to the `expected` (reference) value, in percent
    /// units (3.0 == 3%). `0.0` when both values are zero.
    pub pct_diff: f64,
    pub status: HealthStatus,
    pub reason: Option<String>,
}

/// Compare the `actual` number against the `expected` (reference) number.
/// `pct_unhealthy` is the hard cutoff (percent units) above which any breach
/// escalates from Degraded to Unhealthy. Drift is relative to `expected`.
pub fn compare(
    meta: &VerdictMeta,
    actual: f64,
    expected: f64,
    tol: &Tolerance,
    pct_unhealthy: f64,
) -> DriftVerdict {
    let abs_diff = (actual - expected).abs();
    // pct relative to the reference `expected` value. expected == 0 → undefined;
    // report 0.0 when both are zero, else fall back to the abs arm only
    // (INFINITY, which compares as breached).
    let pct_diff = if expected == 0.0 {
        if actual == 0.0 { 0.0 } else { f64::INFINITY }
    } else {
        (abs_diff / expected.abs()) * 100.0
    };

    let abs_breached = abs_diff > tol.abs;
    let pct_breached = pct_diff > tol.pct; // INFINITY > tol.pct is true
    let breached = match tol.combinator {
        Combinator::And => abs_breached && pct_breached,
        Combinator::Or => abs_breached || pct_breached,
    };

    let pct_stored = if pct_diff.is_finite() { pct_diff } else { 0.0 };

    if !breached {
        return verdict(
            meta,
            actual,
            expected,
            abs_diff,
            pct_stored,
            HealthStatus::Healthy,
            None,
        );
    }

    let over_cutoff = (pct_diff.is_finite() && pct_diff > pct_unhealthy) || pct_diff.is_infinite();
    let status = if over_cutoff {
        HealthStatus::Unhealthy
    } else {
        HealthStatus::Degraded
    };
    let pct_display = if pct_diff.is_finite() {
        format!("{pct_diff:.1}%")
    } else {
        "∞%".to_string()
    };
    let subject = meta.description.as_deref().unwrap_or(&meta.check);
    let reason = Some(format!(
        "{subject} drifts {pct_display} from {expected_label} ({actual_label} {actual:.2} vs \
         {expected_label} {expected:.2})",
        expected_label = meta.expected_label,
        actual_label = meta.actual_label,
    ));
    verdict(meta, actual, expected, abs_diff, pct_stored, status, reason)
}

/// Assemble a `DriftVerdict`, echoing the resolved presentation fields.
fn verdict(
    meta: &VerdictMeta,
    actual: f64,
    expected: f64,
    abs_diff: f64,
    pct_diff: f64,
    status: HealthStatus,
    reason: Option<String>,
) -> DriftVerdict {
    DriftVerdict {
        check: meta.check.clone(),
        description: meta.description.clone(),
        actual_label: meta.actual_label.clone(),
        expected_label: meta.expected_label.clone(),
        actual,
        expected,
        abs_diff,
        pct_diff,
        status,
        reason,
    }
}

/// Verdict for an external source that could not be reached / timed out. An
/// unreachable source is *unknown*, not *wrong* — Degraded, never Unhealthy.
pub fn unreachable_verdict(meta: &VerdictMeta, source: &str) -> DriftVerdict {
    degraded(meta, format!("{source} unreachable"))
}

/// Verdict for a check that errored before comparison (bad measure, missing
/// secret, unknown source). Degraded with the supplied reason.
pub fn error_verdict(meta: &VerdictMeta, reason: String) -> DriftVerdict {
    degraded(meta, reason)
}

/// A Degraded verdict with NaN values (no comparison happened).
fn degraded(meta: &VerdictMeta, reason: String) -> DriftVerdict {
    verdict(
        meta,
        f64::NAN,
        f64::NAN,
        f64::NAN,
        0.0,
        HealthStatus::Degraded,
        Some(reason),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::api::admin::workspace_health::evaluator::HealthStatus;

    fn tol(abs: f64, pct: f64, c: Combinator) -> Tolerance {
        Tolerance {
            abs,
            pct,
            combinator: c,
        }
    }

    fn meta(check: &str) -> VerdictMeta {
        VerdictMeta {
            check: check.to_string(),
            description: None,
            actual_label: "Actual".to_string(),
            expected_label: "Expected".to_string(),
        }
    }

    #[test]
    fn within_tolerance_is_healthy() {
        let v = compare(
            &meta("net_sales"),
            1000.4,
            1000.0,
            &tol(1.0, 0.5, Combinator::And),
            5.0,
        );
        assert_eq!(v.status, HealthStatus::Healthy);
        assert!(v.reason.is_none());
        assert_eq!(v.actual, 1000.4);
        assert_eq!(v.expected, 1000.0);
        assert_eq!(v.actual_label, "Actual");
        assert_eq!(v.expected_label, "Expected");
    }

    #[test]
    fn and_combinator_needs_both_breached() {
        // abs_diff = 3.0 (> 1.0) but pct_diff = 0.3% (< 0.5%): AND not satisfied → healthy.
        let v = compare(
            &meta("m"),
            1003.0,
            1000.0,
            &tol(1.0, 0.5, Combinator::And),
            5.0,
        );
        assert_eq!(v.status, HealthStatus::Healthy);
    }

    #[test]
    fn and_combinator_both_breached_is_degraded() {
        // abs_diff = 30 (> 1.0) and pct_diff = 3% (> 0.5%, < 5% cutoff) → degraded.
        let v = compare(
            &meta("m"),
            1030.0,
            1000.0,
            &tol(1.0, 0.5, Combinator::And),
            5.0,
        );
        assert_eq!(v.status, HealthStatus::Degraded);
        assert!(v.reason.as_ref().unwrap().contains("3.0%"));
    }

    #[test]
    fn reason_uses_resolved_labels_and_description() {
        let m = VerdictMeta {
            check: "net_sales_vs_toast".to_string(),
            description: Some("Daily net sales".to_string()),
            actual_label: "Oxy net sales".to_string(),
            expected_label: "Toast net sales".to_string(),
        };
        let v = compare(&m, 1030.0, 1000.0, &tol(1.0, 0.5, Combinator::And), 5.0);
        let reason = v.reason.unwrap();
        assert!(reason.starts_with("Daily net sales drifts"));
        assert!(reason.contains("from Toast net sales"));
        assert!(reason.contains("Oxy net sales 1030.00"));
        assert!(reason.contains("Toast net sales 1000.00"));
    }

    #[test]
    fn or_combinator_one_breached_is_degraded() {
        // abs_diff = 3.0 (> 1.0), pct_diff = 0.3% (< 0.5%): OR satisfied by abs → degraded.
        let v = compare(
            &meta("m"),
            1003.0,
            1000.0,
            &tol(1.0, 0.5, Combinator::Or),
            5.0,
        );
        assert_eq!(v.status, HealthStatus::Degraded);
    }

    #[test]
    fn breach_over_hard_cutoff_is_unhealthy() {
        // pct_diff = 10% > 5% cutoff → unhealthy regardless of combinator.
        let v = compare(
            &meta("m"),
            1100.0,
            1000.0,
            &tol(1.0, 0.5, Combinator::And),
            5.0,
        );
        assert_eq!(v.status, HealthStatus::Unhealthy);
    }

    #[test]
    fn zero_expected_with_nonzero_actual_breaches_via_abs() {
        // expected == 0: pct is undefined (∞); abs arm breaches and ∞ escalates.
        let v = compare(&meta("m"), 50.0, 0.0, &tol(1.0, 0.5, Combinator::Or), 5.0);
        assert_eq!(v.status, HealthStatus::Unhealthy);
        assert!(v.pct_diff.is_finite());
    }

    #[test]
    fn both_zero_is_healthy() {
        let v = compare(&meta("m"), 0.0, 0.0, &tol(1.0, 0.5, Combinator::And), 5.0);
        assert_eq!(v.status, HealthStatus::Healthy);
        assert_eq!(v.pct_diff, 0.0);
    }

    #[test]
    fn unreachable_is_degraded_with_reason() {
        let v = unreachable_verdict(&meta("m"), "toast");
        assert_eq!(v.status, HealthStatus::Degraded);
        assert!(v.reason.unwrap().contains("toast unreachable"));
        assert!(v.actual.is_nan());
        assert!(v.expected.is_nan());
    }
}
