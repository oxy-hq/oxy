//! Anomaly detection on a single time-series.
//!
//! Algorithm: **MSTL + AutoETS** (multi-seasonal trend decomposition by Loess
//! + automatic exponential smoothing for the trend). Fits on the leading
//! window, forecasts a short tail, flags any *measured* tail observation whose
//! residual exceeds the prediction interval *and* whose z-score (residual / σ
//! across the historical in-sample residuals) exceeds the per-monitor
//! sensitivity cutoff. Imputed buckets are scored on neither side of the split:
//! the invented `0.0` would suppress real anomalies in training and manufacture
//! a −100% one in the test window.
//!
//! The double check (interval + z-score) avoids two failure modes:
//! - Interval-only: with naive seasonality + few cycles, the interval can be
//!   very wide and silently swallow real anomalies.
//! - Z-score only: a metric with low variance produces tiny σ and flags every
//!   bucket as a 10σ event.
//!
//! Both checks must agree before we file an anomaly. That keeps the
//! insights inbox signal-dense. Exception: when σ == 0 (perfect in-sample
//! fit with no residual variance) the z-score gate is vacuously satisfied —
//! any point outside the prediction interval is unconditionally anomalous.
//!
//! Both of those checks read off the fitted model, so a *bad* fit satisfies
//! them together and confidently. [`crate::gates`] adds two further checks on
//! a different axis — the fit's own plausibility, and what the bucket's
//! seasonal phase has actually done — and supplies the robust training values
//! this module fits on.

use augurs_core::{Fit, Predict};
use augurs_ets::{AutoETS, trend::AutoETSTrendModel};
use augurs_mstl::MSTLModel;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::Sensitivity;
use crate::gates;

/// A single time-series observation, ordered ascending by timestamp.
#[derive(Debug, Clone, Copy)]
pub struct Observation {
    pub timestamp: DateTime<Utc>,
    pub value: f64,
    /// True when the warehouse returned no row for this bucket and the loader
    /// invented one to keep the series uniformly spaced (see
    /// `service::fill_gaps`). Such a bucket carries a value of `0.0` that means
    /// "no data", not "measured zero" — the gates in [`crate::gates`] must
    /// never treat it as evidence about what this metric normally does.
    pub imputed: bool,
}

impl Observation {
    /// A bucket the warehouse actually returned.
    pub fn measured(timestamp: DateTime<Utc>, value: f64) -> Self {
        Self {
            timestamp,
            value,
            imputed: false,
        }
    }

    /// A bucket invented by the loader to close a gap in the series.
    pub fn filled(timestamp: DateTime<Utc>) -> Self {
        Self {
            timestamp,
            value: 0.0,
            imputed: true,
        }
    }
}

/// Inputs to the detector.
pub struct DetectInputs<'a> {
    /// Full historical series, oldest → newest. Must be uniformly spaced at
    /// `granularity` (the loader is responsible for forward-filling gaps).
    pub series: &'a [Observation],
    /// Seasonal periods in *number of buckets*, e.g. `[7]` for weekly on
    /// daily data. Multi-seasonality is supported (`[7, 365]`).
    pub seasonal_periods: Vec<usize>,
    /// How many tail observations to evaluate as candidate anomalies. The
    /// rest of the series is the training window.
    pub test_window: usize,
    /// Sensitivity preset → z-score cutoff.
    pub sensitivity: Sensitivity,
    /// Prediction-interval confidence level (e.g. `0.95`). Wider intervals
    /// catch fewer anomalies; tighter ones flag more.
    pub interval_level: f64,
    /// An anomaly event already on record for this series, if any.
    ///
    /// Once a segment is in a known bad stretch, later buckets of the same
    /// stretch should not have to re-argue the case from scratch: a slide that
    /// has already been reported keeps sliding, and the second day of it is
    /// evidence about the *same* problem rather than a new claim needing its
    /// own proof. See [`CONTINUATION_GAP_BUCKETS`].
    pub continuation: Option<Continuation>,
}

/// The tail of an anomaly event already reported for this series.
#[derive(Debug, Clone, Copy)]
pub struct Continuation {
    /// Bucket start of the most recent flagged observation in the event.
    pub last_period: DateTime<Utc>,
    /// `true` when the event is a drop (observed below expected). A spike does
    /// not continue a slump.
    pub is_decrease: bool,
}

/// How many buckets past the event's last flagged bucket may still attach to
/// it.
///
/// Must exceed 1: a real run is rarely unbroken, because the middle day of a
/// three-day surge can sit just inside its own weekday envelope while the days
/// either side clear theirs. Three allows two quiet buckets before the event is
/// considered over, which is what links a Mon/Wed/Thu run into one event.
pub const CONTINUATION_GAP_BUCKETS: usize = 3;

impl Default for DetectInputs<'_> {
    fn default() -> Self {
        Self {
            series: &[],
            seasonal_periods: vec![],
            test_window: 1,
            sensitivity: Sensitivity::Medium,
            interval_level: 0.95,
            continuation: None,
        }
    }
}

/// A flagged observation. One per tail bucket whose residual cleared both
/// the prediction interval and the z-score gate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedAnomaly {
    pub timestamp: DateTime<Utc>,
    pub observed: f64,
    /// Point forecast at this timestamp.
    pub expected: f64,
    /// Lower bound of the prediction interval at `interval_level`.
    pub lower: f64,
    /// Upper bound of the prediction interval at `interval_level`.
    pub upper: f64,
    /// `observed - expected`.
    pub residual: f64,
    /// `residual / σ`, where σ is the std-dev of the in-sample fit residuals.
    pub z_score: f64,
    /// `"low" | "medium" | "high"` based on how far past the z-cutoff.
    pub severity: Severity,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Low,
    Medium,
    High,
}

#[derive(Debug, thiserror::Error)]
pub enum DetectError {
    #[error("series too short: need at least {needed} observations, got {got}")]
    SeriesTooShort { needed: usize, got: usize },
    #[error("no seasonal periods specified")]
    NoPeriods,
    #[error("MSTL fit failed: {0}")]
    Fit(String),
    #[error("MSTL predict failed: {0}")]
    Predict(String),
    #[error("test_window must be >= 1 and < series length")]
    BadTestWindow,
}

/// Run the detector. Returns one entry per tail observation that's
/// flagged; non-anomalous tail buckets are simply omitted.
pub fn detect(inputs: DetectInputs<'_>) -> Result<Vec<DetectedAnomaly>, DetectError> {
    if inputs.seasonal_periods.is_empty() {
        return Err(DetectError::NoPeriods);
    }
    if inputs.test_window == 0 || inputs.test_window >= inputs.series.len() {
        return Err(DetectError::BadTestWindow);
    }
    // MSTL needs >= 2*period observations for each period. Use the largest
    // period as the floor; add a small buffer so AutoETS has data to fit.
    let max_period = *inputs.seasonal_periods.iter().max().unwrap_or(&1);
    let needed = (max_period * 2).max(10) + inputs.test_window;
    if inputs.series.len() < needed {
        return Err(DetectError::SeriesTooShort {
            needed,
            got: inputs.series.len(),
        });
    }

    let train_end = inputs.series.len() - inputs.test_window;
    let train = &inputs.series[..train_end];
    let test = &inputs.series[train_end..];
    // The fit sees a robust version of the training window (gaps imputed to the
    // seasonal-phase median, single-date collapses winsorized); the gates below
    // see the raw one. See `gates::robust_training_values`.
    let train_values = gates::robust_training_values(train, &inputs.seasonal_periods);
    let window = gates::TrainingWindow::new(train, &inputs.seasonal_periods);

    // Where the known event ends, as an index into `series`. Buckets flagged
    // during this run move the tail forward, so a run that starts inside this
    // very test window extends itself without needing a second scan.
    let mut event_tail: Option<(usize, bool)> = inputs.continuation.and_then(|c| {
        inputs
            .series
            .iter()
            .position(|o| o.timestamp == c.last_period)
            .map(|idx| (idx, c.is_decrease))
    });

    // AutoETS with "ZZN" — automatic error + trend, no seasonal component
    // (MSTL strips seasonality before handing the deseasonalised series to
    // the trend model). season_length=1 because seasonality lives in MSTL.
    let ets = AutoETS::new(1, "ZZN").map_err(|e| DetectError::Fit(e.to_string()))?;
    let trend = AutoETSTrendModel::from(ets);
    let mstl = MSTLModel::new(inputs.seasonal_periods.clone(), trend);
    let fitted = mstl
        .fit(&train_values)
        .map_err(|e| DetectError::Fit(e.to_string()))?;

    let forecast = fitted
        .predict(inputs.test_window, Some(inputs.interval_level))
        .map_err(|e| DetectError::Predict(e.to_string()))?;

    // Estimate residual σ from the in-sample fit. Used to compute the
    // z-score for each test bucket. `fit.remainder()` is the per-bucket
    // residual after trend + seasonality are removed — exactly what we want.
    let residuals = fitted.fit().remainder();
    let sigma = std_dev(residuals);
    let z_cut = inputs.sensitivity.z_cutoff();

    let intervals = forecast.intervals.as_ref();
    let mut flagged = Vec::new();
    for (i, obs) in test.iter().enumerate() {
        // A bucket the warehouse never returned is not a measured zero.
        // `fill_gaps` invents `0.0` for it and marks it imputed; scoring that
        // invention reports a *missing row* as a −100% collapse, at the highest
        // severity there is, because 0 against any positive expectation is the
        // largest residual the series can produce. The training side already
        // refuses to fit on these (`gates::robust_training_values`); this is the
        // same refusal on the test side. Strictly a suppression — it can only
        // remove flags, never create one.
        if obs.imputed {
            tracing::debug!(
                target: "metric_monitoring",
                timestamp = %obs.timestamp,
                "suppressed: bucket was never returned by the warehouse"
            );
            continue;
        }
        let expected = forecast.point.get(i).copied().unwrap_or(f64::NAN);
        let (lower, upper) = match intervals {
            Some(iv) => (
                iv.lower.get(i).copied().unwrap_or(f64::NEG_INFINITY),
                iv.upper.get(i).copied().unwrap_or(f64::INFINITY),
            ),
            None => (f64::NEG_INFINITY, f64::INFINITY),
        };
        let residual = obs.value - expected;
        let z = if sigma > 0.0 { residual / sigma } else { 0.0 };
        let out_of_band = obs.value < lower || obs.value > upper;
        // sigma == 0: perfect in-sample fit — any interval breach is anomalous
        // regardless of z-score (dividing by 0 is undefined, not "no anomaly").
        let big_z = sigma == 0.0 || z.abs() >= z_cut;
        if !(out_of_band && big_z) {
            continue;
        }
        // Both tests so far were read off the fitted model, so a bad fit passes
        // them together. These two ask a different question: is the fit usable,
        // and has this seasonal phase ever done anything like this?
        if !gates::fit_is_sane(&window, lower) {
            tracing::debug!(
                target: "metric_monitoring",
                timestamp = %obs.timestamp,
                observed = obs.value,
                expected,
                lower,
                "suppressed: prediction interval is negative on a non-negative measure"
            );
            continue;
        }
        let index = train_end + i;
        if !gates::breaches_empirical_band(&window, index, obs.value) {
            // The band is waived for a bucket continuing an event already on
            // record — but nothing else is. This bucket still had to clear the
            // prediction interval and the z-score cutoff above, so a waived
            // band cannot admit an ordinary reading; it only spares the second
            // day of a known slide from having to out-do the first.
            let continues = event_tail.is_some_and(|(tail, was_decrease)| {
                index > tail
                    && index - tail <= CONTINUATION_GAP_BUCKETS
                    && (residual < 0.0) == was_decrease
            });
            if !continues {
                tracing::debug!(
                    target: "metric_monitoring",
                    timestamp = %obs.timestamp,
                    observed = obs.value,
                    expected,
                    z_score = z,
                    "suppressed: within the seasonal phase's observed range"
                );
                continue;
            }
            tracing::debug!(
                target: "metric_monitoring",
                timestamp = %obs.timestamp,
                observed = obs.value,
                "kept: continues an open event despite sitting inside its phase range"
            );
        }
        event_tail = Some((index, residual < 0.0));
        let severity = if sigma == 0.0 {
            Severity::High
        } else {
            severity_from_z(z.abs(), z_cut)
        };
        flagged.push(DetectedAnomaly {
            timestamp: obs.timestamp,
            observed: obs.value,
            expected,
            lower,
            upper,
            residual,
            z_score: z,
            severity,
        });
    }
    Ok(flagged)
}

fn std_dev(values: &[f32]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let n = values.len() as f64;
    let mean = values.iter().map(|&v| v as f64).sum::<f64>() / n;
    let var = values
        .iter()
        .map(|&v| {
            let d = v as f64 - mean;
            d * d
        })
        .sum::<f64>()
        / (n - 1.0);
    var.sqrt()
}

fn severity_from_z(abs_z: f64, cutoff: f64) -> Severity {
    let ratio = abs_z / cutoff;
    if ratio >= 2.0 {
        Severity::High
    } else if ratio >= 1.5 {
        Severity::Medium
    } else {
        Severity::Low
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn synth_series(n: usize, slope: f64, seasonal_amp: f64, noise: f64) -> Vec<Observation> {
        let base = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        (0..n)
            .map(|i| {
                let t = base + chrono::Duration::days(i as i64);
                let value = slope * i as f64
                    + seasonal_amp * (2.0 * std::f64::consts::PI * (i % 7) as f64 / 7.0).sin()
                    + ((i * 1103515245 + 12345) % 1000) as f64 / 1000.0 * noise;
                Observation::measured(t, value)
            })
            .collect()
    }

    #[test]
    fn flat_seasonal_series_has_no_anomalies() {
        let series = synth_series(60, 0.0, 5.0, 0.1);
        let out = detect(DetectInputs {
            series: &series,
            seasonal_periods: vec![7],
            test_window: 3,
            sensitivity: Sensitivity::Medium,
            interval_level: 0.95,
            continuation: None,
        })
        .unwrap();
        assert!(out.is_empty(), "expected zero anomalies, got {out:?}");
    }

    #[test]
    fn injected_spike_is_flagged() {
        let mut series = synth_series(60, 0.0, 5.0, 0.1);
        // 50σ-ish spike on the last bucket.
        let last = series.len() - 1;
        series[last].value += 200.0;
        let out = detect(DetectInputs {
            series: &series,
            seasonal_periods: vec![7],
            test_window: 1,
            sensitivity: Sensitivity::Medium,
            interval_level: 0.95,
            continuation: None,
        })
        .unwrap();
        assert_eq!(out.len(), 1);
        assert!(out[0].z_score.abs() > 3.0);
        assert_eq!(out[0].severity, Severity::High);
    }

    #[test]
    fn spike_on_zero_sigma_series_is_flagged() {
        // A perfectly constant training series gives sigma == 0; a spike in the
        // test window must still be detected (not silently swallowed).
        let base = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let mut series: Vec<Observation> = (0..20)
            .map(|i| Observation::measured(base + chrono::Duration::days(i), 100.0))
            .collect();
        // Large spike in the last bucket.
        let last = series.len() - 1;
        series[last].value = 5000.0;
        let out = detect(DetectInputs {
            series: &series,
            seasonal_periods: vec![7],
            test_window: 1,
            sensitivity: Sensitivity::Medium,
            interval_level: 0.95,
            continuation: None,
        })
        .unwrap();
        assert_eq!(out.len(), 1, "spike on zero-sigma series must be flagged");
        assert_eq!(out[0].severity, Severity::High);
    }

    /// A weekday-seasonal store series. Saturdays carry a wide, realistic
    /// spread (4,482–6,737); the rest of the week is calm.
    fn store_series(weeks: usize) -> Vec<Observation> {
        let base = Utc.with_ymd_and_hms(2026, 1, 5, 0, 0, 0).unwrap(); // a Monday
        let by_weekday = [1500.0, 1450.0, 1520.0, 1480.0, 1600.0, 0.0, 2300.0];
        let saturdays = [
            6737.0, 6552.0, 6239.0, 5897.0, 6075.0, 4482.0, 4556.0, 6300.0, 6600.0, 5800.0, 6400.0,
            6100.0,
        ];
        (0..weeks * 7)
            .map(|i| {
                let timestamp = base + chrono::Duration::days(i as i64);
                if i % 7 == 5 {
                    return Observation::measured(timestamp, saturdays[(i / 7) % saturdays.len()]);
                }
                // A little deterministic wobble so sigma is not degenerate.
                let jitter = ((i * 37) % 11) as f64 - 5.0;
                Observation::measured(timestamp, by_weekday[i % 7] + jitter * 4.0)
            })
            .collect()
    }

    /// Overwrite the last Saturday of `series` and return its timestamp.
    fn set_last_saturday(series: &mut [Observation], value: f64) -> DateTime<Utc> {
        let last = series.len() - 1;
        let saturday = last - (last % 7) + 5;
        series[saturday].value = value;
        series[saturday].timestamp
    }

    /// The `e814f363` 07-04 row: the warehouse returned no row for that
    /// store-day, `fill_gaps` invented `0.0`, and the detector filed it as a
    /// high-severity −100% drop (observed 0.00, expected 542.80, z = −10.1).
    /// A missing row is not a collapse to zero, and it is the *most* alarming
    /// thing the detector can say — exactly backwards for absent data.
    #[test]
    fn an_imputed_test_bucket_is_not_scored_as_a_collapse() {
        let mut series = store_series(12);
        let last = series.len() - 1;
        // A Wednesday (~1,520) the warehouse never returned.
        let gap = last - 2;
        series[gap] = Observation::filled(series[gap].timestamp);
        let gap_timestamp = series[gap].timestamp;

        let out = detect(DetectInputs {
            series: &series,
            seasonal_periods: vec![7],
            test_window: 3,
            sensitivity: Sensitivity::Medium,
            interval_level: 0.95,
            continuation: None,
        })
        .unwrap();

        assert!(
            !out.iter().any(|a| a.timestamp == gap_timestamp),
            "an invented 0.0 was reported as an anomaly: {out:?}"
        );
    }

    /// The suppression above must be about *provenance*, not about the value:
    /// a store that genuinely recorded 0.00 (closed for the day) is a real
    /// event and must still fire.
    #[test]
    fn a_measured_zero_is_still_flagged() {
        let mut series = store_series(12);
        let last = series.len() - 1;
        let gap = last - 2;
        series[gap] = Observation::measured(series[gap].timestamp, 0.0);
        let zero_timestamp = series[gap].timestamp;

        let out = detect(DetectInputs {
            series: &series,
            seasonal_periods: vec![7],
            test_window: 3,
            sensitivity: Sensitivity::Medium,
            interval_level: 0.95,
            continuation: None,
        })
        .unwrap();

        assert!(
            out.iter().any(|a| a.timestamp == zero_timestamp),
            "a measured zero must still be reported: {out:?}"
        );
    }

    #[test]
    fn a_bare_overshoot_of_its_own_weekday_range_is_suppressed() {
        // The false positive no z-cutoff can reach. 6,815 is 1.2% above the
        // highest Saturday on record — a series re-testing its own range — yet
        // it clears the model's prediction interval at z = 5.6, more
        // confidently than most real anomalies. The empirical band is the only
        // test that separates it, because it asks a different question: has
        // this weekday ever done anything like this?
        let mut series = store_series(12);
        let ts = set_last_saturday(&mut series, 6815.0);

        let out = detect(DetectInputs {
            series: &series,
            seasonal_periods: vec![7],
            test_window: 7,
            sensitivity: Sensitivity::Medium,
            interval_level: 0.95,
            continuation: None,
        })
        .unwrap();
        assert!(
            !out.iter().any(|a| a.timestamp == ts),
            "a bare overshoot of the Saturday envelope must not stand; got {out:?}"
        );
    }

    #[test]
    fn a_decisive_breach_of_the_weekday_range_still_fires() {
        // The counterweight: the gates must subtract false positives without
        // swallowing what monitors exist to catch. Both directions, one
        // fixture, so a gate that over-suppresses cannot pass this file.
        for (value, expect_negative) in [(7600.0, false), (3000.0, true)] {
            let mut series = store_series(12);
            let ts = set_last_saturday(&mut series, value);

            let out = detect(DetectInputs {
                series: &series,
                seasonal_periods: vec![7],
                test_window: 7,
                sensitivity: Sensitivity::Medium,
                interval_level: 0.95,
                continuation: None,
            })
            .unwrap();
            let hit = out
                .iter()
                .find(|a| a.timestamp == ts)
                .unwrap_or_else(|| panic!("{value} decisively breaches the Saturday range"));
            assert_eq!(hit.residual < 0.0, expect_negative);
        }
    }

    #[test]
    fn one_collapse_in_training_barely_moves_the_verdict() {
        // A single catastrophic day inflates the in-sample residual sigma, and
        // the prediction interval widens with it — so unrelated real anomalies
        // for weeks afterwards are quietly swallowed. Measured on this fixture,
        // one collapsed Saturday widened the band from +/-535 to +/-1,214 and
        // pushed a genuine anomaly from z = 5.6 down to z = 3.2, a hair above
        // the Medium cutoff of 3.0.
        //
        // Asserting insensitivity rather than a flagged/not-flagged outcome is
        // deliberate: a test balanced on either side of the cutoff would pass
        // or fail on an augurs point release.
        let verdict = |collapse: Option<f64>| {
            let mut series = store_series(12);
            if let Some(value) = collapse {
                // Index 40 is a Saturday, the store's biggest day — a holiday
                // there is the chain-wide collapse that actually moves sigma.
                series[40].value = value;
            }
            let ts = set_last_saturday(&mut series, 7200.0);
            let out = detect(DetectInputs {
                series: &series,
                seasonal_periods: vec![7],
                test_window: 7,
                sensitivity: Sensitivity::Medium,
                interval_level: 0.95,
                continuation: None,
            })
            .unwrap();
            let hit = out
                .iter()
                .find(|a| a.timestamp == ts)
                .expect("7200 is a decisive breach in both runs")
                .clone();
            (hit.z_score, hit.upper - hit.lower)
        };

        let (clean_z, clean_width) = verdict(None);
        let (holiday_z, holiday_width) = verdict(Some(781.0));

        assert!(
            (holiday_z - clean_z).abs() < 0.5,
            "one collapsed day must not move the verdict: z {clean_z} -> {holiday_z}"
        );
        assert!(
            holiday_width < clean_width * 1.5,
            "one collapsed day must not balloon the interval: {clean_width} -> {holiday_width}"
        );
    }

    #[test]
    fn a_bucket_inside_its_range_attaches_to_an_open_event() {
        // The boundary case from the field: a store already reported as sliding
        // posts another low day that misses its own Saturday floor by ~$2. On
        // its own that bucket must not stand — the margin is calibrated for
        // exactly that. But it is not on its own: it continues an event already
        // filed, and the second day of a known slide is evidence about the same
        // problem rather than a fresh claim needing its own proof.
        let mut series = store_series(12);
        let ts = set_last_saturday(&mut series, 6815.0); // 1.2% over the max
        let last = series.len() - 1;
        // Two buckets back — the Thursday of the same week. This is the field
        // shape exactly: the segment was flagged on Thursday, and Saturday is
        // the bucket that misses its own floor by a hair.
        let tail = series[last - (last % 7) + 5 - 2].timestamp;

        let inputs = |continuation| DetectInputs {
            series: &series,
            seasonal_periods: vec![7],
            test_window: 7,
            sensitivity: Sensitivity::Medium,
            interval_level: 0.95,
            continuation,
        };

        assert!(
            !detect(inputs(None))
                .unwrap()
                .iter()
                .any(|a| a.timestamp == ts),
            "without an open event this is the bare overshoot, and must not stand"
        );

        // Same bucket, same numbers, but an event is already on record within
        // the gap and pointing the same way.
        let out = detect(inputs(Some(Continuation {
            last_period: tail,
            is_decrease: false,
        })))
        .unwrap();
        assert!(
            out.iter().any(|a| a.timestamp == ts),
            "a bucket continuing an open event must attach to it; got {out:?}"
        );
    }

    #[test]
    fn an_open_event_does_not_waive_the_band_for_the_other_direction() {
        // A slump does not license a spike. If it did, one reported anomaly
        // would leave a segment permanently half-gated in both directions.
        let mut series = store_series(12);
        let ts = set_last_saturday(&mut series, 6815.0);
        let last = series.len() - 1;
        // Two buckets back, i.e. within range — so this test isolates
        // direction rather than accidentally passing on distance.
        let tail = series[last - (last % 7) + 5 - 2].timestamp;

        let out = detect(DetectInputs {
            series: &series,
            seasonal_periods: vec![7],
            test_window: 7,
            sensitivity: Sensitivity::Medium,
            interval_level: 0.95,
            continuation: Some(Continuation {
                last_period: tail,
                // The open event is a *drop*; this bucket is a rise.
                is_decrease: true,
            }),
        })
        .unwrap();
        assert!(
            !out.iter().any(|a| a.timestamp == ts),
            "an open decrease must not waive the band for an increase"
        );
    }

    // The stale-event test below uses the previous Saturday as its tail, which
    // is 7 buckets back. Pinned at compile time so widening the gap constant
    // past a week turns that test from meaningful into vacuous loudly, rather
    // than leaving it silently passing for the wrong reason.
    const _: () = assert!(CONTINUATION_GAP_BUCKETS < 7);

    #[test]
    fn a_stale_event_stops_waiving_the_band() {
        // Events expire by distance, or a single anomaly would keep a segment
        // half-gated forever. The prior Saturday is 7 buckets back, past
        // CONTINUATION_GAP_BUCKETS.
        let mut series = store_series(12);
        let ts = set_last_saturday(&mut series, 6815.0);
        let last = series.len() - 1;
        let stale = series[last - (last % 7) + 5 - 7].timestamp; // the prior Saturday

        let out = detect(DetectInputs {
            series: &series,
            seasonal_periods: vec![7],
            test_window: 7,
            sensitivity: Sensitivity::Medium,
            interval_level: 0.95,
            continuation: Some(Continuation {
                last_period: stale,
                is_decrease: false,
            }),
        })
        .unwrap();
        assert!(
            !out.iter().any(|a| a.timestamp == ts),
            "an event {CONTINUATION_GAP_BUCKETS}+ buckets back must no longer waive the band"
        );
    }

    #[test]
    fn too_short_series_errors() {
        let series = synth_series(8, 0.0, 5.0, 0.1);
        let err = detect(DetectInputs {
            series: &series,
            seasonal_periods: vec![7],
            test_window: 1,
            sensitivity: Sensitivity::Medium,
            interval_level: 0.95,
            continuation: None,
        })
        .unwrap_err();
        matches!(err, DetectError::SeriesTooShort { .. });
    }
}
