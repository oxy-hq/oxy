//! Statistical trust gates layered on top of the [`crate::detect`] model.
//!
//! `detect` flags a bucket when its residual clears both the prediction
//! interval and the z-score cutoff. Both of those tests are read off the
//! *fitted model*, so when the fit itself is wrong they agree with each other
//! and produce a confident, wrong answer: a store in its opening ramp gets a
//! forecast frozen at its first week and reads as a +22σ event every day; a
//! weekly series with two near-zero buckets gets a prediction interval whose
//! lower bound is negative on a non-negative measure.
//!
//! Empirically, model-driven false positives are *more* confident than real
//! anomalies — they occupy a higher z-range — so no cutoff separates them.
//! The gates here therefore test on a different axis than z: what the series
//! has actually done, and whether the fit is self-evidently broken.
//!
//! Two properties are worth stating because they are what make the gates safe:
//!
//! - **They only subtract.** Every gate is an additional AND on an already
//!   flagged bucket, so a mis-calibrated gate can hide a real anomaly but can
//!   never fabricate one. The one exception is
//!   [`robust_training_values`], which changes the fit itself and so can move
//!   flags in both directions; it is documented at its definition.
//! - **No evidence means no suppression.** A gate that cannot be evaluated
//!   (too few samples in a seasonal phase, an empty training window) passes the
//!   bucket through. Suppressing on thin data would silently disable detection
//!   exactly where [`min_history_buckets`] is already the right answer.

use crate::config::Granularity;
use crate::detect::Observation;

/// Scale factor that turns a median-absolute-deviation into a σ-comparable
/// spread for normally-distributed data. Standard constant: `1 / Φ⁻¹(0.75)`.
const MAD_TO_SIGMA: f64 = 1.4826;

/// How far beyond a seasonal phase's observed envelope a value must sit before
/// the breach counts as evidence, in robust σ.
///
/// Zero would mean "any new high or low", which is far too loose: a Tuesday
/// coming in 1% above the highest Tuesday on record is a normal series
/// re-testing its own range, not an anomaly, and the min/max of ~8–12 samples
/// is itself noisy. Half a robust σ past anything the phase has ever recorded
/// is a deliberate compromise — it kills bare-envelope overshoots while
/// keeping genuine breaches, which in practice clear the envelope by 25%+.
const ENVELOPE_MARGIN_MADS: f64 = 0.5;

/// Minimum measured samples in a seasonal phase before its envelope is treated
/// as evidence. Below this the "range" is an artifact of having two points.
const MIN_PHASE_SAMPLES: usize = 3;

/// Winsorization threshold for [`robust_training_values`], in robust σ.
///
/// Deliberately loose: the aim is to defang chain-wide collapses (a holiday, a
/// POS outage) that `seasonality: [7]` cannot represent and that otherwise drag
/// a baseline down for weeks, *not* to flatten ordinary variation.
const WINSOR_MADS: f64 = 4.0;

/// The training slice, indexed by seasonal phase.
///
/// "Phase" is a bucket's position within the dominant seasonal cycle — for
/// `seasonality: [7]` on daily data, its weekday. Comparing a Saturday against
/// every other day of the week is what produces the mid-range-Saturday false
/// positives; every gate here compares like with like.
pub(crate) struct TrainingWindow<'a> {
    series: &'a [Observation],
    period: usize,
}

impl<'a> TrainingWindow<'a> {
    /// `train` must be the training slice alone — the test window is scored
    /// against it and must not be part of it.
    pub(crate) fn new(train: &'a [Observation], seasonal_periods: &[usize]) -> Self {
        Self {
            series: train,
            // The *nearest* cycle, matching the `seasonal_period` snapshotted
            // onto the stored anomaly row by `persist`, so the gate and the
            // explain path reason about the same phase.
            period: seasonal_periods.iter().copied().min().unwrap_or(1).max(1),
        }
    }

    /// Measured values sharing `index`'s seasonal phase, where `index` is
    /// absolute within the full series (training + test), matching how the
    /// caller counts buckets.
    fn phase_values(&self, index: usize) -> Vec<f64> {
        let phase = index % self.period;
        self.series
            .iter()
            .enumerate()
            .filter(|(i, o)| i % self.period == phase && !o.imputed)
            .map(|(_, o)| o.value)
            .collect()
    }

    fn measured_values(&self) -> impl Iterator<Item = f64> + '_ {
        self.series.iter().filter(|o| !o.imputed).map(|o| o.value)
    }
}

/// Reject a flag whose prediction interval is self-evidently broken.
///
/// A negative lower bound on a measure that has never once been negative is not
/// a wide interval, it is a wrong one: the fit is extrapolating outside the
/// data's own support, and the "expected" value it pairs with that bound is
/// equally untrustworthy. Returns `true` when the fit is usable.
pub(crate) fn fit_is_sane(train: &TrainingWindow<'_>, lower: f64) -> bool {
    if !lower.is_finite() || lower >= 0.0 {
        return true;
    }
    let mut measured = train.measured_values().peekable();
    if measured.peek().is_none() {
        return true; // no evidence either way
    }
    // A negative bound is legitimate on a measure that does go negative
    // (margin, net change); it is only a defect on a non-negative one.
    !measured.all(|v| v >= 0.0)
}

/// Require the observation to breach what its own seasonal phase has actually
/// done, not merely what the model predicted.
///
/// `index` is the bucket's absolute position in the full series. Returns `true`
/// when the flag should stand.
pub(crate) fn breaches_empirical_band(
    train: &TrainingWindow<'_>,
    index: usize,
    observed: f64,
) -> bool {
    let values = train.phase_values(index);
    if values.len() < MIN_PHASE_SAMPLES {
        return true; // not enough of this phase to have an opinion
    }
    let (min, max) = values
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &v| {
            (lo.min(v), hi.max(v))
        });
    // A zero MAD (a phase that has never varied) leaves the margin at zero, so
    // the test degrades to the bare envelope — which is the right answer there:
    // any departure from a constant is a real departure.
    let margin = ENVELOPE_MARGIN_MADS * scaled_mad(&values);
    observed < min - margin || observed > max + margin
}

/// The values MSTL actually fits on: per seasonal phase, imputed buckets take
/// the phase median and measured outliers are clamped to `WINSOR_MADS` robust σ
/// either side of it.
///
/// This is the one thing in this module that is **not** a pure subtraction — it
/// changes the fit, so it can in principle create flags as well as remove them.
/// That is deliberate, and it is the only way to fix two problems at their
/// source rather than at the symptom:
///
/// - A zero-filled gap is missing data, not a measured zero. Feeding `0.0` to
///   the fit inflates σ (widening the interval until real anomalies pass
///   through) and drags the trend down.
/// - A chain-wide collapse on a single date — a holiday — contaminates every
///   store's baseline for weeks afterwards, because a weekly seasonal term has
///   nowhere to put an annual event.
///
/// Downstream, [`breaches_empirical_band`] reads the *raw* observations, never
/// these cleaned values, so a tighter fit cannot widen what the gate accepts.
pub(crate) fn robust_training_values(
    train: &[Observation],
    seasonal_periods: &[usize],
) -> Vec<f64> {
    let window = TrainingWindow::new(train, seasonal_periods);
    train
        .iter()
        .enumerate()
        .map(|(i, obs)| {
            let values = window.phase_values(i);
            let center = median(&values);
            // Imputed first, and unconditionally. An invented bucket carries a
            // `0.0` that means "no data", so returning it — which is what the
            // thin-phase branch below does — would feed the fit the exact
            // contamination this function exists to remove. Any measured
            // neighbour at all beats that zero; a phase with no measured
            // samples yields `median(&[]) == 0.0` and so degrades no further.
            if obs.imputed {
                return center;
            }
            // Below this point the bucket is measured, so its own value is the
            // best evidence available and only a well-populated phase earns the
            // right to clamp it.
            if values.len() < MIN_PHASE_SAMPLES {
                return obs.value;
            }
            let spread = WINSOR_MADS * scaled_mad(&values);
            if spread <= 0.0 {
                return obs.value; // constant phase: nothing to clamp against
            }
            obs.value.clamp(center - spread, center + spread)
        })
        .collect()
}

/// Minimum **measured** buckets before a series is trustworthy enough to score.
///
/// Distinct from the floor inside [`crate::detect`], which is algebraic — the
/// fewest points MSTL needs to not error. This one is statistical: the fewest
/// points it needs to be *right*. A new store clears the algebraic floor at 21
/// daily buckets and is then forecast off its own opening ramp, pinning the
/// expectation at its first week's level while the store triples; every day
/// after that reads as a high-severity anomaly.
///
/// Eight seasonal cycles is the compromise. It costs a new store roughly two
/// months of monitoring — pure delay, since the rolling test window re-scores
/// those buckets once the history exists — and it removes the single largest
/// class of false positive.
///
/// Monthly is special-cased: eight cycles of `[12]` is eight years, which would
/// disable monthly monitoring outright, so it asks for two.
pub fn min_history_buckets(granularity: Granularity, seasonal_periods: &[usize]) -> usize {
    let max_period = seasonal_periods.iter().copied().max().unwrap_or(1).max(1);
    match granularity {
        Granularity::Day => (max_period * 8).max(28),
        Granularity::Week => (max_period * 8).max(26),
        Granularity::Month => (max_period * 2).max(12),
    }
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<f64> = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}

/// Median absolute deviation, scaled to be comparable to a standard deviation.
fn scaled_mad(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let center = median(values);
    let deviations: Vec<f64> = values.iter().map(|v| (v - center).abs()).collect();
    MAD_TO_SIGMA * median(&deviations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Duration, TimeZone, Utc};

    fn base() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
    }

    /// Build a daily series from raw values, all measured.
    fn measured(values: &[f64]) -> Vec<Observation> {
        values
            .iter()
            .enumerate()
            .map(|(i, &v)| Observation::measured(base() + Duration::days(i as i64), v))
            .collect()
    }

    #[test]
    fn median_and_mad_are_robust_to_a_single_collapse() {
        let values = vec![100.0, 102.0, 98.0, 101.0, 3.0];
        assert_eq!(median(&values), 100.0);
        // Deviations: 0, 2, 2, 1, 97 -> median 2 -> scaled ~2.97. A standard
        // deviation over the same values is ~43; that gap is the whole point.
        assert!((scaled_mad(&values) - 2.9652).abs() < 1e-3);
    }

    #[test]
    fn negative_lower_bound_on_a_non_negative_measure_is_rejected() {
        let series = measured(&[2600.0, 2700.0, 2500.0, 0.0, 2800.0]);
        let window = TrainingWindow::new(&series, &[1]);
        assert!(
            !fit_is_sane(&window, -1463.0),
            "a negative bound on a never-negative measure is a broken fit"
        );
        assert!(fit_is_sane(&window, 12.0), "a positive bound is usable");
    }

    #[test]
    fn negative_lower_bound_is_allowed_when_the_measure_goes_negative() {
        // Net change / margin measures legitimately dip below zero, so the
        // bound says nothing about fit quality there.
        let series = measured(&[10.0, -4.0, 8.0, -2.0, 6.0]);
        let window = TrainingWindow::new(&series, &[1]);
        assert!(fit_is_sane(&window, -12.0));
    }

    #[test]
    fn bare_envelope_overshoot_is_suppressed_but_a_real_breach_survives() {
        // One seasonal phase (period 1) of Tuesdays.
        let series = measured(&[6737.0, 6552.0, 6239.0, 5897.0, 6075.0, 4482.0, 4556.0]);
        let window = TrainingWindow::new(&series, &[1]);
        let next = series.len();
        // 1.2% above the highest value on record: the series re-testing its
        // own range, and the case no z-cutoff can separate.
        assert!(
            !breaches_empirical_band(&window, next, 6815.0),
            "a bare overshoot of the envelope must not stand"
        );
        // Far below anything the phase has done.
        assert!(breaches_empirical_band(&window, next, 2100.0));
    }

    #[test]
    fn the_band_compares_a_phase_against_its_own_history() {
        // Alternating weekday/weekend levels: period 2. A value normal for a
        // high phase is a large drop for that phase's neighbour, and vice
        // versa — a phase-blind envelope would see one series spanning 100–900
        // and suppress both.
        let series = measured(&[
            900.0, 100.0, 880.0, 120.0, 910.0, 90.0, 890.0, 110.0, 905.0, 105.0,
        ]);
        let window = TrainingWindow::new(&series, &[2]);
        // Index 10 is the high phase (10 % 2 == 0): 500 is a collapse there...
        assert!(breaches_empirical_band(&window, 10, 500.0));
        // ...but on the low phase (index 11) it is an equally clear spike.
        assert!(breaches_empirical_band(&window, 11, 500.0));
        // And each phase's own normal level stands unflagged.
        assert!(!breaches_empirical_band(&window, 10, 895.0));
        assert!(!breaches_empirical_band(&window, 11, 108.0));
    }

    #[test]
    fn a_thin_phase_never_suppresses() {
        let series = measured(&[500.0, 520.0]);
        let window = TrainingWindow::new(&series, &[1]);
        assert!(
            breaches_empirical_band(&window, 2, 505.0),
            "two samples are not an envelope; the flag must pass through"
        );
    }

    #[test]
    fn imputed_buckets_are_not_evidence_for_the_envelope() {
        // A zero-filled gap would drag the phase minimum to 0 and swallow every
        // real drop — the exact failure this mask exists to prevent.
        let mut series = measured(&[2500.0, 2400.0, 2600.0, 2550.0, 2450.0]);
        series[2] = Observation::filled(series[2].timestamp);
        let window = TrainingWindow::new(&series, &[1]);
        assert!(
            breaches_empirical_band(&window, series.len(), 489.0),
            "a near-zero reading must still breach a phase whose real floor is ~2400"
        );
    }

    #[test]
    fn training_values_replace_gaps_and_clamp_collapses() {
        let mut series = measured(&[
            3400.0, 3300.0, 3500.0, 3450.0, 781.0, 3350.0, 3400.0, 3380.0, 3420.0,
        ]);
        // A chain-wide holiday collapse at index 4, and a gap at index 7.
        series[7] = Observation::filled(series[7].timestamp);
        let values = robust_training_values(&series, &[1]);

        assert!(
            values[4] > 2500.0,
            "the holiday collapse must be pulled back toward the phase, got {}",
            values[4]
        );
        assert!(
            values[7] > 2500.0,
            "a zero-filled gap must not enter the fit as a measured zero, got {}",
            values[7]
        );
        // Ordinary variation is left exactly alone.
        assert_eq!(values[0], 3400.0);
        assert_eq!(values[3], 3450.0);
    }

    #[test]
    fn a_gap_in_a_thin_phase_still_never_enters_the_fit_as_zero() {
        // The ordering trap: the thin-phase branch returns `obs.value`, which
        // for an imputed bucket is the invented 0.0. A source that
        // systematically drops one day of the week (a warehouse that never
        // loads Sundays) produces exactly this — a phase too thin to winsorize,
        // every bucket of it imputed — and the zero-fill contamination the
        // module exists to remove walks straight back into the fit.
        let mut series = measured(&[900.0, 100.0, 880.0, 120.0, 910.0, 130.0]);
        // Phase 1 (the odd indices) keeps only ONE measured sample, below
        // MIN_PHASE_SAMPLES, and the bucket under test is imputed.
        series[1] = Observation::filled(series[1].timestamp);
        series[3] = Observation::filled(series[3].timestamp);

        let values = robust_training_values(&series, &[2]);
        assert_eq!(
            values[1], 130.0,
            "an imputed bucket must take its phase's median even when the phase \
             is too thin to winsorize"
        );
        assert_eq!(values[3], 130.0);
        // The measured buckets are untouched: too thin to clamp is not a
        // licence to rewrite real readings.
        assert_eq!(values[0], 900.0);
        assert_eq!(values[5], 130.0);
    }

    #[test]
    fn a_wholly_unmeasured_phase_degrades_no_further() {
        // Nothing to impute from: the result is the same 0.0 as before, not a
        // panic and not a NaN poisoning the whole fit.
        let mut series = measured(&[900.0, 100.0, 880.0, 120.0]);
        series[1] = Observation::filled(series[1].timestamp);
        series[3] = Observation::filled(series[3].timestamp);
        let values = robust_training_values(&series, &[2]);
        assert_eq!(values[1], 0.0);
        assert_eq!(values[3], 0.0);
        assert_eq!(values[0], 900.0);
    }

    #[test]
    fn training_values_leave_a_constant_phase_untouched() {
        // A zero MAD must not collapse every point onto the median.
        let series = measured(&[100.0, 100.0, 100.0, 100.0, 5000.0]);
        let values = robust_training_values(&series, &[1]);
        assert_eq!(values[4], 5000.0);
    }

    #[test]
    fn history_floor_is_eight_cycles_except_for_monthly() {
        assert_eq!(min_history_buckets(Granularity::Day, &[7]), 56);
        assert_eq!(min_history_buckets(Granularity::Week, &[4]), 32);
        // Eight cycles of [12] would be eight years of history.
        assert_eq!(min_history_buckets(Granularity::Month, &[12]), 24);
        // Sub-weekly seasonality still gets a usable floor.
        assert_eq!(min_history_buckets(Granularity::Day, &[2]), 28);
    }
}
