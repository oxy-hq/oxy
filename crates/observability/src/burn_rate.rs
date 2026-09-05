//! Multi-window error-budget burn-rate evaluation for custom apps.
//!
//! Pure: no I/O, no clock reads. Callers hand it counts from
//! [`crate::store::ObservabilityStore::get_app_availability`] and it returns a
//! verdict, so every rule below is unit-testable without a ClickHouse.
//!
//! ## Why burn rate and not "N failures in a row"
//!
//! A consecutive-failure rule is the obvious thing and it is wrong in both
//! directions at once. It pages for a three-request blip on a quiet app, and it
//! stays silent while a busy app fails 20% of requests forever — because the
//! failures are never consecutive. Burn rate asks the question that actually
//! matters: *at this rate, how fast are we spending the error budget the SLO
//! allows?* An app allowed 1% failures that is failing 14% is burning budget
//! 14× as fast as it may, and will exhaust a month's allowance in two days.
//!
//! ## Why two windows per rule
//!
//! The long window decides **whether** something is wrong; the short one
//! decides whether it is **still** wrong. Without the short window an alert
//! keeps firing for hours after an incident ends, because the long window still
//! contains it — the classic "alert that nobody can silence and everybody
//! learns to ignore". Both must exceed the threshold, so recovery clears the
//! alert as soon as the short window drains.
//!
//! Thresholds follow the Google SRE workbook's canonical table.

use crate::types::AppAvailabilityWindow;

/// Every window the evaluator needs, in minutes. Query these once and hand the
/// whole set to [`evaluate`] — the rules below index into it by length.
pub const ALERT_WINDOWS_MINUTES: &[u32] = &[5, 30, 60, 120, 360, 1440];

/// How much of the error budget a rule tolerates being spent, and over what.
struct Rule {
    long_minutes: u32,
    short_minutes: u32,
    /// Multiple of the allowed failure rate.
    burn_rate: f64,
    severity: Severity,
}

/// The canonical multi-window table. Ordered most-urgent first so [`evaluate`]
/// can return on the first match.
const RULES: &[Rule] = &[
    // ~2% of a 30-day budget in one hour.
    Rule {
        long_minutes: 60,
        short_minutes: 5,
        burn_rate: 14.4,
        severity: Severity::Page,
    },
    // ~5% in six hours.
    Rule {
        long_minutes: 360,
        short_minutes: 30,
        burn_rate: 6.0,
        severity: Severity::Page,
    },
    // ~10% in a day — real, but it can wait for a working hour.
    Rule {
        long_minutes: 1440,
        short_minutes: 120,
        burn_rate: 3.0,
        severity: Severity::Ticket,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Wake someone.
    Page,
    /// File it; it is degrading, not down.
    Ticket,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BurnVerdict {
    /// Not enough traffic to say anything. **Distinct from healthy on purpose**:
    /// an app nobody is using is not an app that is working, and reporting it
    /// as healthy is how a dead app pages nobody. Callers should render this as
    /// "no data", never as a green tick.
    NoOpinion,
    /// Burning budget no faster than the SLO allows.
    Healthy,
    /// A rule matched.
    Burning {
        severity: Severity,
        /// Multiple of the allowed failure rate, from the long window.
        burn_rate: f64,
        long_minutes: u32,
        short_minutes: u32,
        /// Failure ratio in the long window, for the alert text.
        failure_ratio: f64,
    },
}

/// Availability objective and the traffic floor below which no opinion is
/// offered.
#[derive(Debug, Clone, Copy)]
pub struct SloConfig {
    /// e.g. `0.99` for 99%.
    pub objective: f64,
    /// Minimum requests in a window before it may trigger a rule.
    ///
    /// Without this, one failed request out of two on a nearly-idle app is a
    /// 50% failure rate and a 50× burn — a page, every time an internal tool
    /// nobody is using hiccups at 3am. The floor is what makes this usable on a
    /// long tail of low-traffic apps, which is most of them.
    pub min_requests: u64,
}

impl Default for SloConfig {
    fn default() -> Self {
        Self {
            // 99% — deliberately not 99.9%. These are internal tools on a
            // platform whose own dependencies (a warehouse, an LLM provider)
            // are nowhere near three nines, and an objective the platform
            // cannot meet produces alerts nobody can act on.
            objective: 0.99,
            min_requests: 20,
        }
    }
}

impl SloConfig {
    /// The failure ratio the objective permits. `0.99` → `0.01`.
    fn allowed_failure_ratio(&self) -> f64 {
        (1.0 - self.objective).max(f64::EPSILON)
    }
}

fn window<'a>(
    windows: &'a [AppAvailabilityWindow],
    minutes: u32,
) -> Option<&'a AppAvailabilityWindow> {
    windows.iter().find(|w| w.window_minutes == minutes)
}

/// Burn rate for one window, or `None` when it lacks the traffic to have an
/// opinion.
fn burn_rate_of(w: &AppAvailabilityWindow, cfg: &SloConfig) -> Option<f64> {
    if w.total < cfg.min_requests {
        return None;
    }
    let ratio = w.failure_ratio()?;
    Some(ratio / cfg.allowed_failure_ratio())
}

/// Evaluate every rule against the supplied windows, most urgent first.
///
/// A rule whose windows are missing from `windows` is skipped rather than
/// treated as passing — an absent measurement is not evidence of health.
pub fn evaluate(windows: &[AppAvailabilityWindow], cfg: &SloConfig) -> BurnVerdict {
    let mut saw_enough_traffic = false;

    for rule in RULES {
        let (Some(long), Some(short)) = (
            window(windows, rule.long_minutes),
            window(windows, rule.short_minutes),
        ) else {
            continue;
        };
        let (Some(long_burn), Some(short_burn)) =
            (burn_rate_of(long, cfg), burn_rate_of(short, cfg))
        else {
            // The long window carrying traffic is what lets us say "healthy"
            // rather than "no opinion" when no rule fires. The short one going
            // quiet is normal and must not, on its own, downgrade the verdict.
            saw_enough_traffic |= burn_rate_of(long, cfg).is_some();
            continue;
        };
        saw_enough_traffic = true;

        if long_burn >= rule.burn_rate && short_burn >= rule.burn_rate {
            return BurnVerdict::Burning {
                severity: rule.severity,
                burn_rate: long_burn,
                long_minutes: rule.long_minutes,
                short_minutes: rule.short_minutes,
                failure_ratio: long.failure_ratio().unwrap_or(0.0),
            };
        }
    }

    if saw_enough_traffic {
        BurnVerdict::Healthy
    } else {
        BurnVerdict::NoOpinion
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Requests per minute in the fixtures. Traffic **must** scale with window
    /// length — a 24h window sees ~288× the requests of a 5m one, and a fixture
    /// that gives every window the same total makes a fixed number of failures
    /// look equally severe over five minutes and over a day. That is precisely
    /// the dilution these rules are built on, so flattening it tests nothing.
    const RATE_PER_MIN: u64 = 20;

    fn total_for(minutes: u32) -> u64 {
        RATE_PER_MIN * minutes as u64
    }

    /// Every window, at a constant failure ratio.
    fn at_ratio(ratio: f64) -> Vec<AppAvailabilityWindow> {
        ALERT_WINDOWS_MINUTES
            .iter()
            .map(|m| {
                let total = total_for(*m);
                AppAvailabilityWindow {
                    window_minutes: *m,
                    total,
                    failed: (total as f64 * ratio).round() as u64,
                }
            })
            .collect()
    }

    /// Every window, with a FIXED number of failures — one bounded incident,
    /// diluted more the longer the window.
    fn with_incident(failed: u64, only_windows_at_least: u32) -> Vec<AppAvailabilityWindow> {
        ALERT_WINDOWS_MINUTES
            .iter()
            .map(|m| AppAvailabilityWindow {
                window_minutes: *m,
                total: total_for(*m),
                failed: if *m >= only_windows_at_least {
                    failed
                } else {
                    0
                },
            })
            .collect()
    }

    fn w(minutes: u32, total: u64, failed: u64) -> AppAvailabilityWindow {
        AppAvailabilityWindow {
            window_minutes: minutes,
            total,
            failed,
        }
    }

    #[test]
    fn a_healthy_app_is_healthy() {
        assert_eq!(
            evaluate(&at_ratio(0.001), &SloConfig::default()),
            BurnVerdict::Healthy
        );
    }

    /// The reason `NoOpinion` exists. An app with no traffic must not report as
    /// healthy — "nobody is using it" and "it works" are different facts, and
    /// only one of them means an outage would be noticed.
    #[test]
    fn an_idle_app_has_no_opinion_rather_than_a_clean_bill() {
        let quiet: Vec<_> = ALERT_WINDOWS_MINUTES.iter().map(|m| w(*m, 0, 0)).collect();
        assert_eq!(
            evaluate(&quiet, &SloConfig::default()),
            BurnVerdict::NoOpinion
        );
    }

    /// The 3am-page case the traffic floor exists to prevent: one failure out
    /// of two requests is a 50× burn rate and must still say nothing.
    #[test]
    fn a_trickle_of_traffic_cannot_page_however_bad_the_ratio() {
        let trickle: Vec<_> = ALERT_WINDOWS_MINUTES.iter().map(|m| w(*m, 2, 1)).collect();
        assert_eq!(
            evaluate(&trickle, &SloConfig::default()),
            BurnVerdict::NoOpinion
        );
    }

    /// A hard, ongoing outage: 15% failures at every timescale burns a 1%
    /// budget 15×, over the 14.4× page threshold on the fastest rule.
    #[test]
    fn a_sustained_outage_pages_on_the_fastest_rule() {
        match evaluate(&at_ratio(0.15), &SloConfig::default()) {
            BurnVerdict::Burning {
                severity,
                long_minutes,
                short_minutes,
                ..
            } => {
                assert_eq!(severity, Severity::Page);
                assert_eq!((long_minutes, short_minutes), (60, 5));
            }
            other => panic!("expected a page, got {other:?}"),
        }
    }

    /// **The rule the short window earns its keep on.** A 180-failure incident
    /// that ended ~35 minutes ago is gone from the 5m and 30m windows but still
    /// sits in the 1h one, where it reads as 15% — a 15× burn. A long-window
    /// rule alone would keep paging right through the recovery, which is how an
    /// alert becomes something people mute. Requiring the short window to agree
    /// clears it as soon as the failures stop.
    #[test]
    fn a_recovered_incident_stops_alerting_even_though_the_long_window_is_dirty() {
        let recovering = with_incident(180, 60);
        // Precondition: the 1h window really is dirty enough to page on its own.
        let hour = recovering
            .iter()
            .find(|x| x.window_minutes == 60)
            .expect("60m window");
        assert!(
            burn_rate_of(hour, &SloConfig::default()).unwrap() >= 14.4,
            "fixture must actually be page-worthy over the hour, else this \
             test would pass for the wrong reason"
        );
        assert_eq!(
            evaluate(&recovering, &SloConfig::default()),
            BurnVerdict::Healthy
        );
    }

    /// The converse: a spike that has only just started fills the short window
    /// but not the long one, so it does not page yet. That delay is deliberate —
    /// it is what makes a page mean "sustained", not "one bad minute".
    #[test]
    fn a_brand_new_spike_does_not_page_until_the_long_window_agrees() {
        let mut spiking = with_incident(50, 5);
        // Half of the last five minutes failed.
        let five = spiking
            .iter_mut()
            .find(|x| x.window_minutes == 5)
            .expect("5m window");
        assert!(
            burn_rate_of(five, &SloConfig::default()).unwrap() >= 14.4,
            "the short window must be on fire, else this proves nothing"
        );
        assert_eq!(
            evaluate(&spiking, &SloConfig::default()),
            BurnVerdict::Healthy
        );
    }

    /// A slow leak: 4% failures is 4× a 1% budget — over the 3× ticket rule,
    /// under the 6× and 14.4× page rules.
    #[test]
    fn a_slow_leak_files_a_ticket_rather_than_paging() {
        match evaluate(&at_ratio(0.04), &SloConfig::default()) {
            BurnVerdict::Burning {
                severity,
                long_minutes,
                ..
            } => {
                assert_eq!(severity, Severity::Ticket);
                assert_eq!(long_minutes, 1440);
            }
            other => panic!("expected a ticket, got {other:?}"),
        }
    }

    /// An absent window must not read as a passing one.
    #[test]
    fn missing_windows_are_skipped_not_treated_as_healthy() {
        // Only the 24h pair present, and it is burning at 4×.
        let partial = vec![
            w(
                1440,
                total_for(1440),
                (total_for(1440) as f64 * 0.04) as u64,
            ),
            w(120, total_for(120), (total_for(120) as f64 * 0.04) as u64),
        ];
        match evaluate(&partial, &SloConfig::default()) {
            BurnVerdict::Burning { severity, .. } => assert_eq!(severity, Severity::Ticket),
            other => panic!("expected the 24h rule to still fire, got {other:?}"),
        }
        // Nothing at all present: no opinion, not healthy.
        assert_eq!(evaluate(&[], &SloConfig::default()), BurnVerdict::NoOpinion);
    }

    /// A 100%-available objective would make the allowed ratio zero and every
    /// burn rate infinite. Clamped rather than dividing by zero.
    #[test]
    fn a_perfect_objective_does_not_divide_by_zero() {
        let cfg = SloConfig {
            objective: 1.0,
            min_requests: 1,
        };
        let verdict = evaluate(&at_ratio(0.001), &cfg);
        assert!(
            matches!(verdict, BurnVerdict::Burning { burn_rate, .. } if burn_rate.is_finite()),
            "expected a finite burn rate, got {verdict:?}"
        );
    }

    /// Severity ordering: when both a page rule and a ticket rule match, the
    /// page wins. Evaluated most-urgent-first, so this pins the RULES order.
    #[test]
    fn a_page_outranks_a_ticket_when_both_match() {
        match evaluate(&at_ratio(0.5), &SloConfig::default()) {
            BurnVerdict::Burning { severity, .. } => assert_eq!(severity, Severity::Page),
            other => panic!("expected a page, got {other:?}"),
        }
    }
}
