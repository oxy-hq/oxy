//! The worse-than-before rule: is a release worse than the one production
//! served before it?
//!
//! Pure, and integer-only. Every rate is compared by cross-multiplication
//! (`a/b >= 2 * c/d` as `a*d >= 2*c*b`), so a boundary never depends on how a
//! float rounded. The thresholds are constants here, and each reason a verdict
//! gives carries the rule it applied, so the console shows why without restating
//! them.
//!
//! - **Red (`worse`)** only from what the platform measured itself: the failure
//!   rate of the release's function calls.
//! - **Amber (`slower`)** from route latency (p95), and from Web Vitals, which
//!   the page's own JavaScript reports and could therefore forge. INP is never
//!   judged: the runtime records the worst interaction in the first 10 seconds,
//!   not a percentile of all of them.
//! - **`not_comparable`** when no rule had enough samples on both sides. It is
//!   never `not_worse`: no reading is never healthy.

use serde::Serialize;

/// Counted calls, both sides, for the failure rule.
pub const MIN_CALLS: i64 = 50;
/// The current side must have at least this many failed calls.
pub const MIN_FAILED: i64 = 3;
/// The current rate must be at least this multiple of the previous one…
pub const RATE_FACTOR: i64 = 2;
/// …and at least this many basis points higher (100 bp = 1 percentage point).
pub const RATE_POINTS_BP: i64 = 100;
/// Route-mode calls, both sides, for the p95 rule.
pub const MIN_ROUTE_CALLS: i64 = 50;
/// p95 at least `P95_NUM / P95_DEN` of the previous (1.5×)…
pub const P95_NUM: i64 = 3;
pub const P95_DEN: i64 = 2;
/// …and at least this many milliseconds slower.
pub const P95_FLOOR_MS: i64 = 200;
/// Page loads per screen, both sides, for the vitals rules.
pub const MIN_VITAL_SAMPLES: i64 = 20;
/// LCP p75 at least `LCP_NUM / LCP_DEN` of the previous (+25 %)…
pub const LCP_NUM: i64 = 5;
pub const LCP_DEN: i64 = 4;
/// …and at least this many milliseconds slower.
pub const LCP_FLOOR_MS: i64 = 500;
/// CLS p75 (×1000) at least this much higher (0.05)…
pub const CLS_RISE: i64 = 50;
/// …and now over the "good" threshold (0.1).
pub const CLS_POOR: i64 = 100;

const FAILURE_RULE: &str = "failure rate at least 2x and 1 point higher, 50 calls each side";
const P95_RULE: &str = "route p95 at least 1.5x and 200 ms slower, 50 route calls each side";
const LCP_RULE: &str = "LCP p75 at least 25% and 500 ms slower, 20 page loads each side";
const CLS_RULE: &str = "CLS p75 at least 0.05 higher and over 0.1, 20 page loads each side";

/// What one release measured.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Side {
    /// Calls that finished (not shed, not still running).
    pub calls: i64,
    pub failed: i64,
    pub route_calls: i64,
    pub p95_ms: Option<i64>,
    pub screens: Vec<Screen>,
}

/// One screen's Web Vitals in one release.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Screen {
    pub path: String,
    pub samples: i64,
    pub lcp_p75: Option<i64>,
    /// CLS × 1000, as the runtime sends it.
    pub cls_p75: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Red: the failure rule fired.
    Worse,
    /// Amber: only a latency or vitals rule fired.
    Slower,
    /// At least one rule had its samples and none fired. Not "healthy".
    NotWorse,
    NotComparable,
}

/// One rule that fired, with the numbers it fired on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Reason {
    /// `failure_rate` | `p95` | `lcp` | `cls`.
    pub metric: &'static str,
    /// The screen, for a vitals rule.
    pub screen: Option<String>,
    pub before: i64,
    pub after: i64,
    /// `bp` (basis points) | `ms` | `cls_milli`.
    pub unit: &'static str,
    pub rule: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Verdict {
    pub kind: Kind,
    pub reasons: Vec<Reason>,
    /// Why it could not be judged; set only for `not_comparable`.
    pub why: Option<String>,
}

impl Verdict {
    pub fn not_comparable(why: impl Into<String>) -> Self {
        Self {
            kind: Kind::NotComparable,
            reasons: Vec::new(),
            why: Some(why.into()),
        }
    }
}

/// A rule's answer: not enough samples (and why), measured and held, or fired.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Unmeasured(String),
    Held,
    Fired(Vec<Reason>),
}

/// `failed / calls` in basis points, rounded down. `None` with no calls.
pub fn rate_bp(failed: i64, calls: i64) -> Option<i64> {
    (calls > 0).then(|| failed * 10_000 / calls)
}

/// Judge `cur` against `prev`, the release production served before it.
pub fn judge(cur: &Side, prev: &Side) -> Verdict {
    let failure = failure_rule(cur, prev);
    let latency = p95_rule(cur, prev);
    let vitals = vitals_rule(cur, prev);
    let fired = |o: &Outcome| matches!(o, Outcome::Fired(_));
    let measured = |o: &Outcome| !matches!(o, Outcome::Unmeasured(_));
    let kind = if fired(&failure) {
        Kind::Worse
    } else if fired(&latency) || fired(&vitals) {
        Kind::Slower
    } else if measured(&failure) || measured(&latency) || measured(&vitals) {
        Kind::NotWorse
    } else {
        let why = match failure {
            Outcome::Unmeasured(why) => why,
            _ => unreachable!("an unmeasured verdict has an unmeasured failure rule"),
        };
        return Verdict::not_comparable(why);
    };
    let reasons = [failure, latency, vitals]
        .into_iter()
        .flat_map(|o| match o {
            Outcome::Fired(reasons) => reasons,
            _ => Vec::new(),
        })
        .collect();
    Verdict {
        kind,
        reasons,
        why: None,
    }
}

fn failure_rule(cur: &Side, prev: &Side) -> Outcome {
    let fewest = cur.calls.min(prev.calls);
    if fewest < MIN_CALLS {
        return Outcome::Unmeasured(format!("too few calls: {fewest} of {MIN_CALLS}"));
    }
    if cur.failed < MIN_FAILED {
        return Outcome::Held;
    }
    // i128: a hot app's call counts multiplied together must not overflow.
    let (cf, cc) = (i128::from(cur.failed), i128::from(cur.calls));
    let (pf, pc) = (i128::from(prev.failed), i128::from(prev.calls));
    let doubled = cf * pc >= i128::from(RATE_FACTOR) * pf * cc;
    // cf/cc - pf/pc >= RATE_POINTS_BP / 10_000
    let points = (cf * pc - pf * cc) * 10_000 >= i128::from(RATE_POINTS_BP) * cc * pc;
    if !(doubled && points) {
        return Outcome::Held;
    }
    Outcome::Fired(vec![Reason {
        metric: "failure_rate",
        screen: None,
        before: rate_bp(prev.failed, prev.calls).unwrap_or(0),
        after: rate_bp(cur.failed, cur.calls).unwrap_or(0),
        unit: "bp",
        rule: FAILURE_RULE,
    }])
}

fn p95_rule(cur: &Side, prev: &Side) -> Outcome {
    let (Some(after), Some(before)) = (cur.p95_ms, prev.p95_ms) else {
        return Outcome::Unmeasured("no route calls to time".into());
    };
    let fewest = cur.route_calls.min(prev.route_calls);
    if fewest < MIN_ROUTE_CALLS {
        return Outcome::Unmeasured(format!(
            "too few route calls: {fewest} of {MIN_ROUTE_CALLS}"
        ));
    }
    if P95_DEN * after >= P95_NUM * before && after - before >= P95_FLOOR_MS {
        return Outcome::Fired(vec![Reason {
            metric: "p95",
            screen: None,
            before,
            after,
            unit: "ms",
            rule: P95_RULE,
        }]);
    }
    Outcome::Held
}

fn vitals_rule(cur: &Side, prev: &Side) -> Outcome {
    let mut measured = false;
    let mut reasons = Vec::new();
    for screen in &cur.screens {
        let Some(before) = prev.screens.iter().find(|p| p.path == screen.path) else {
            continue;
        };
        if screen.samples.min(before.samples) < MIN_VITAL_SAMPLES {
            continue;
        }
        if let (Some(after), Some(was)) = (screen.lcp_p75, before.lcp_p75) {
            measured = true;
            if LCP_DEN * after >= LCP_NUM * was && after - was >= LCP_FLOOR_MS {
                reasons.push(vital("lcp", screen, was, after, "ms", LCP_RULE));
            }
        }
        if let (Some(after), Some(was)) = (screen.cls_p75, before.cls_p75) {
            measured = true;
            if after - was >= CLS_RISE && after > CLS_POOR {
                reasons.push(vital("cls", screen, was, after, "cls_milli", CLS_RULE));
            }
        }
    }
    match (measured, reasons.is_empty()) {
        (false, _) => Outcome::Unmeasured("no screen with enough page loads on both sides".into()),
        (true, true) => Outcome::Held,
        (true, false) => Outcome::Fired(reasons),
    }
}

fn vital(
    metric: &'static str,
    screen: &Screen,
    before: i64,
    after: i64,
    unit: &'static str,
    rule: &'static str,
) -> Reason {
    Reason {
        metric,
        screen: Some(screen.path.clone()),
        before,
        after,
        unit,
        rule,
    }
}

#[cfg(test)]
#[path = "rule_tests.rs"]
mod tests;
