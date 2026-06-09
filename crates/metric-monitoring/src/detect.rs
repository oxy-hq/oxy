//! Anomaly detection on a single time-series.
//!
//! Algorithm: **MSTL + AutoETS** (multi-seasonal trend decomposition by Loess
//! + automatic exponential smoothing for the trend). Fits on the leading
//! window, forecasts a short tail, flags any tail observation whose residual
//! exceeds the prediction interval *and* whose z-score (residual / σ across
//! the historical in-sample residuals) exceeds the per-monitor sensitivity
//! cutoff.
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

use augurs_core::{Fit, Predict};
use augurs_ets::{AutoETS, trend::AutoETSTrendModel};
use augurs_mstl::MSTLModel;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::Sensitivity;

/// A single time-series observation, ordered ascending by timestamp.
#[derive(Debug, Clone, Copy)]
pub struct Observation {
    pub timestamp: DateTime<Utc>,
    pub value: f64,
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
}

impl Default for DetectInputs<'_> {
    fn default() -> Self {
        Self {
            series: &[],
            seasonal_periods: vec![],
            test_window: 1,
            sensitivity: Sensitivity::Medium,
            interval_level: 0.95,
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
    let train_values: Vec<f64> = inputs.series[..train_end].iter().map(|o| o.value).collect();
    let test = &inputs.series[train_end..];

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
                Observation {
                    timestamp: t,
                    value,
                }
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
            .map(|i| Observation {
                timestamp: base + chrono::Duration::days(i),
                value: 100.0,
            })
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
        })
        .unwrap();
        assert_eq!(out.len(), 1, "spike on zero-sigma series must be flagged");
        assert_eq!(out[0].severity, Severity::High);
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
        })
        .unwrap_err();
        matches!(err, DetectError::SeriesTooShort { .. });
    }
}
