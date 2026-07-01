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

/// One reconciliation outcome, persisted and surfaced in the health payload.
#[derive(Debug, Clone, Serialize)]
pub struct DriftVerdict {
    pub check: String,
    pub oxy: f64,
    pub ext: f64,
    pub abs_diff: f64,
    /// Percent drift relative to the external (authoritative) value, in percent
    /// units (3.0 == 3%). `0.0` when both values are zero.
    pub pct_diff: f64,
    pub status: HealthStatus,
    pub reason: Option<String>,
}

/// Compare an Oxy-computed number against the external authoritative number.
/// `pct_unhealthy` is the hard cutoff (percent units) above which any breach
/// escalates from Degraded to Unhealthy.
pub fn compare(
    check: &str,
    oxy: f64,
    ext: f64,
    tol: &Tolerance,
    pct_unhealthy: f64,
) -> DriftVerdict {
    let abs_diff = (oxy - ext).abs();
    // pct relative to the authoritative external value. ext == 0 → undefined;
    // report 0.0 when both are zero, else fall back to the abs arm only
    // (INFINITY, which compares as breached).
    let pct_diff = if ext == 0.0 {
        if oxy == 0.0 { 0.0 } else { f64::INFINITY }
    } else {
        (abs_diff / ext.abs()) * 100.0
    };

    let abs_breached = abs_diff > tol.abs;
    let pct_breached = pct_diff > tol.pct; // INFINITY > tol.pct is true
    let breached = match tol.combinator {
        Combinator::And => abs_breached && pct_breached,
        Combinator::Or => abs_breached || pct_breached,
    };

    let pct_stored = if pct_diff.is_finite() { pct_diff } else { 0.0 };

    if !breached {
        return DriftVerdict {
            check: check.to_string(),
            oxy,
            ext,
            abs_diff,
            pct_diff: pct_stored,
            status: HealthStatus::Healthy,
            reason: None,
        };
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
    let reason = Some(format!(
        "{check} drifts {pct_display} from source (oxy {oxy:.2} vs {ext:.2})"
    ));
    DriftVerdict {
        check: check.to_string(),
        oxy,
        ext,
        abs_diff,
        pct_diff: pct_stored,
        status,
        reason,
    }
}

/// Verdict for an external source that could not be reached / timed out. An
/// unreachable source is *unknown*, not *wrong* — Degraded, never Unhealthy.
pub fn unreachable_verdict(check: &str, source: &str) -> DriftVerdict {
    DriftVerdict {
        check: check.to_string(),
        oxy: f64::NAN,
        ext: f64::NAN,
        abs_diff: f64::NAN,
        pct_diff: 0.0,
        status: HealthStatus::Degraded,
        reason: Some(format!("{source} unreachable")),
    }
}

/// Verdict for a check that errored before comparison (bad measure, missing
/// secret, unknown source). Degraded with the supplied reason.
pub fn error_verdict(check: &str, reason: String) -> DriftVerdict {
    DriftVerdict {
        check: check.to_string(),
        oxy: f64::NAN,
        ext: f64::NAN,
        abs_diff: f64::NAN,
        pct_diff: 0.0,
        status: HealthStatus::Degraded,
        reason: Some(reason),
    }
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

    #[test]
    fn within_tolerance_is_healthy() {
        let v = compare(
            "net_sales",
            1000.4,
            1000.0,
            &tol(1.0, 0.5, Combinator::And),
            5.0,
        );
        assert_eq!(v.status, HealthStatus::Healthy);
        assert!(v.reason.is_none());
    }

    #[test]
    fn and_combinator_needs_both_breached() {
        // abs_diff = 3.0 (> 1.0) but pct_diff = 0.3% (< 0.5%): AND not satisfied → healthy.
        let v = compare("m", 1003.0, 1000.0, &tol(1.0, 0.5, Combinator::And), 5.0);
        assert_eq!(v.status, HealthStatus::Healthy);
    }

    #[test]
    fn and_combinator_both_breached_is_degraded() {
        // abs_diff = 30 (> 1.0) and pct_diff = 3% (> 0.5%, < 5% cutoff) → degraded.
        let v = compare("m", 1030.0, 1000.0, &tol(1.0, 0.5, Combinator::And), 5.0);
        assert_eq!(v.status, HealthStatus::Degraded);
        assert!(v.reason.as_ref().unwrap().contains("3.0%"));
    }

    #[test]
    fn or_combinator_one_breached_is_degraded() {
        // abs_diff = 3.0 (> 1.0), pct_diff = 0.3% (< 0.5%): OR satisfied by abs → degraded.
        let v = compare("m", 1003.0, 1000.0, &tol(1.0, 0.5, Combinator::Or), 5.0);
        assert_eq!(v.status, HealthStatus::Degraded);
    }

    #[test]
    fn breach_over_hard_cutoff_is_unhealthy() {
        // pct_diff = 10% > 5% cutoff → unhealthy regardless of combinator.
        let v = compare("m", 1100.0, 1000.0, &tol(1.0, 0.5, Combinator::And), 5.0);
        assert_eq!(v.status, HealthStatus::Unhealthy);
    }

    #[test]
    fn zero_external_with_nonzero_oxy_breaches_via_abs() {
        // ext == 0: pct is undefined (∞); abs arm breaches and ∞ escalates to unhealthy.
        let v = compare("m", 50.0, 0.0, &tol(1.0, 0.5, Combinator::Or), 5.0);
        assert_eq!(v.status, HealthStatus::Unhealthy);
        assert!(v.pct_diff.is_finite());
    }

    #[test]
    fn both_zero_is_healthy() {
        let v = compare("m", 0.0, 0.0, &tol(1.0, 0.5, Combinator::And), 5.0);
        assert_eq!(v.status, HealthStatus::Healthy);
        assert_eq!(v.pct_diff, 0.0);
    }

    #[test]
    fn unreachable_is_degraded_with_reason() {
        let v = unreachable_verdict("m", "toast");
        assert_eq!(v.status, HealthStatus::Degraded);
        assert!(v.reason.unwrap().contains("toast unreachable"));
    }
}
