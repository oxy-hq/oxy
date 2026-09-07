//! Forward projection of a single metric series.
//!
//! Same model as [`crate::detect`] — MSTL + AutoETS — pointed the other way.
//! The detector fits a leading window and predicts the *measured* tail so it
//! can score what actually happened; a projection fits the whole window and
//! predicts buckets that do not exist yet. One estimator, two readings, so a
//! forecast the scenario canvas draws and an expectation the insights inbox
//! flags can never disagree about what "normal" was.
//!
//! What this module refuses is the point of it. A projection is a claim about
//! the future, and the cheapest way to make a confident wrong one is to fit a
//! trend to three weeks of a new store's opening ramp and extend it. So the
//! statistical floor here is [`gates::min_history_buckets`] — the *same* eight
//! seasonal cycles the monitors demand before they will score a series — not
//! the algebraic floor of what MSTL will accept without erroring. A series
//! under that floor comes back [`ProjectError::NotEnoughHistory`], which the
//! caller renders as "no forecast, and here's why", never as a flat line.

use augurs_core::{Fit, Predict};
use augurs_ets::{AutoETS, trend::AutoETSTrendModel};
use augurs_mstl::MSTLModel;
use chrono::NaiveDate;

use crate::config::Granularity;
use crate::detect::Observation;
use crate::gates;
use crate::service::{advance_period, fill_gaps};

/// Default prediction-interval level. Matches the detector's default, so the
/// band a projection draws is the band an anomaly would have had to breach.
pub const DEFAULT_INTERVAL_LEVEL: f64 = 0.95;

/// One projected bucket: the point forecast and its prediction interval.
///
/// The interval travels with the point because a projection without one
/// invites the reading that the line is what will happen. It is the same
/// interval [`crate::detect`] scores against.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectedBucket {
    /// Start of the bucket, in the same calendar terms the history uses.
    pub date: NaiveDate,
    pub point: f64,
    pub lower: f64,
    pub upper: f64,
}

/// A series and what to do with it.
pub struct ProjectInputs<'a> {
    /// Ascending by date, one entry per bucket the warehouse returned. May be
    /// sparse — gaps are filled and marked, never scored as measured zeros.
    pub history: &'a [(NaiveDate, f64)],
    pub granularity: Granularity,
    /// Empty → [`Granularity::default_seasonality`].
    pub seasonal_periods: Vec<usize>,
    /// Buckets to project past the last historical one.
    pub horizon: usize,
    pub interval_level: f64,
}

/// Why a series produced no projection.
///
/// Each names a different fix, which is why they are not one variant with a
/// string: "wait for more history" and "the fit blew up" are different
/// sentences, and only one of them is the user's to act on.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProjectError {
    #[error("no horizon requested")]
    ZeroHorizon,
    #[error(
        "only {got} measured bucket(s) of history, need {needed} — a forecast off a shorter \
         window extrapolates the series' opening ramp"
    )]
    NotEnoughHistory { needed: usize, got: usize },
    #[error("could not fit the series: {0}")]
    Fit(String),
    #[error("could not project the fitted series: {0}")]
    Predict(String),
}

/// Project `horizon` buckets past the end of `history`.
///
/// The returned dates continue the history's own calendar walk, so a monthly
/// series projects month starts rather than 31-day jumps.
pub fn project(inputs: ProjectInputs<'_>) -> Result<Vec<ProjectedBucket>, ProjectError> {
    let prepared = prepare(&inputs)?;

    // "ZZN" — automatic error and trend, no seasonal term: MSTL has already
    // stripped seasonality before the trend model sees the series.
    let ets = AutoETS::new(1, "ZZN").map_err(|e| ProjectError::Fit(e.to_string()))?;
    let mstl = MSTLModel::new(prepared.periods.clone(), AutoETSTrendModel::from(ets));
    let fitted = mstl
        .fit(&prepared.values)
        .map_err(|e| ProjectError::Fit(e.to_string()))?;
    let forecast = fitted
        .predict(inputs.horizon, Some(inputs.interval_level))
        .map_err(|e| ProjectError::Predict(e.to_string()))?;

    Ok(collect_buckets(
        &forecast,
        prepared.last_bucket,
        inputs.granularity,
    ))
}

/// A series that has cleared both floors and been made robust, ready for
/// [`project`]'s estimator.
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedSeries {
    /// Robust training values: gaps imputed to their seasonal phase's median,
    /// single-date collapses winsorized.
    pub values: Vec<f64>,
    /// Resolved seasonal periods — the request's, or the granularity default.
    pub periods: Vec<usize>,
    /// Last historical bucket. The forecast's first bucket is one period past it.
    pub last_bucket: NaiveDate,
}

/// Resolve periods, fill gaps, check both floors, and build the robust training
/// values — everything before the estimator.
pub fn prepare(inputs: &ProjectInputs<'_>) -> Result<PreparedSeries, ProjectError> {
    if inputs.horizon == 0 {
        return Err(ProjectError::ZeroHorizon);
    }
    let periods = if inputs.seasonal_periods.is_empty() {
        inputs.granularity.default_seasonality()
    } else {
        inputs.seasonal_periods.clone()
    };

    let filled = fill_gaps(inputs.history.to_vec(), inputs.granularity);
    let observations = to_observations(&filled);
    check_history(&observations, inputs.granularity, &periods)?;

    // The fit sees the same robust series the detector's does: gaps imputed to
    // their seasonal phase's median, single-date collapses winsorized. An
    // invented `0.0` left in place would drag the trend down and project a
    // decline that only ever existed as a missing row.
    let values = gates::robust_training_values(&observations, &periods);
    let last_bucket = filled
        .last()
        .map(|(date, _, _)| *date)
        .ok_or(ProjectError::NotEnoughHistory { needed: 1, got: 0 })?;

    Ok(PreparedSeries {
        values,
        periods,
        last_bucket,
    })
}

/// The calendar the forecast lands on: `horizon` buckets after `last_bucket`,
/// walking the granularity's own calendar so a monthly series projects month
/// starts rather than 31-day jumps.
///
/// Module-private: its only caller is `collect_buckets` below, and it is not in
/// `lib.rs`'s re-export list. It was `pub` for the forecaster seam that
/// `f0468d9a3` deleted — a public surface nothing outside can reach is an
/// invitation to re-grow that seam by accident.
fn bucket_dates(
    last_bucket: NaiveDate,
    granularity: Granularity,
    horizon: usize,
) -> Vec<NaiveDate> {
    let mut date = last_bucket;
    (0..horizon)
        .map(|_| {
            date = advance_period(date, granularity);
            date
        })
        .collect()
}

/// Both floors, in the order that makes the message useful.
///
/// The statistical floor is the higher of the two and names something the
/// analyst can wait out, so it is checked first — reporting MSTL's algebraic
/// minimum to someone whose real problem is eight weeks of history sends them
/// looking for a bug in the fit.
fn check_history(
    observations: &[Observation],
    granularity: Granularity,
    periods: &[usize],
) -> Result<(), ProjectError> {
    let measured = observations.iter().filter(|o| !o.imputed).count();
    let needed = gates::min_history_buckets(granularity, periods);
    if measured < needed {
        return Err(ProjectError::NotEnoughHistory {
            needed,
            got: measured,
        });
    }
    // MSTL wants two full cycles of the longest period before it can separate
    // seasonality from trend at all. Above the statistical floor this can only
    // bite on a granularity/period pair the floor does not cover.
    let max_period = periods.iter().copied().max().unwrap_or(1).max(1);
    let algebraic = (max_period * 2).max(10);
    if observations.len() < algebraic {
        return Err(ProjectError::NotEnoughHistory {
            needed: algebraic,
            got: observations.len(),
        });
    }
    Ok(())
}

/// Gap-filled rows as observations. Timestamps are UTC midnight: nothing
/// downstream reads them — [`gates::robust_training_values`] groups by *index*
/// — but `imputed` is load-bearing and travels with them.
fn to_observations(filled: &[(NaiveDate, f64, bool)]) -> Vec<Observation> {
    filled
        .iter()
        .map(|(date, value, imputed)| {
            let ts = date
                .and_hms_opt(0, 0, 0)
                .expect("midnight is always valid")
                .and_utc();
            if *imputed {
                Observation::filled(ts)
            } else {
                Observation::measured(ts, *value)
            }
        })
        .collect()
}

/// Pair each forecast point with the bucket date it belongs to.
///
/// A missing interval is widened to infinity rather than collapsed onto the
/// point: an absent band means "unknown spread", and drawing the point as if
/// it were the whole story is the claim this module exists to avoid making.
fn collect_buckets(
    forecast: &augurs_core::Forecast,
    last_history: NaiveDate,
    granularity: Granularity,
) -> Vec<ProjectedBucket> {
    let intervals = forecast.intervals.as_ref();
    let dates = bucket_dates(last_history, granularity, forecast.point.len());
    forecast
        .point
        .iter()
        .zip(dates)
        .enumerate()
        .map(|(i, (point, date))| ProjectedBucket {
            date,
            point: *point,
            lower: intervals
                .and_then(|iv| iv.lower.get(i).copied())
                .unwrap_or(f64::NEG_INFINITY),
            upper: intervals
                .and_then(|iv| iv.upper.get(i).copied())
                .unwrap_or(f64::INFINITY),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `days` daily buckets from 2026-01-01, with a weekly cycle on a rising
    /// trend — the shape MSTL is meant to decompose.
    fn seasonal_series(days: usize) -> Vec<(NaiveDate, f64)> {
        let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        (0..days)
            .map(|i| {
                let weekday = (i % 7) as f64;
                (
                    start + chrono::Duration::days(i as i64),
                    100.0 + i as f64 * 0.5 + weekday * 10.0,
                )
            })
            .collect()
    }

    fn inputs(history: &[(NaiveDate, f64)], horizon: usize) -> ProjectInputs<'_> {
        ProjectInputs {
            history,
            granularity: Granularity::Day,
            seasonal_periods: Vec::new(),
            horizon,
            interval_level: DEFAULT_INTERVAL_LEVEL,
        }
    }

    #[test]
    fn projects_the_requested_number_of_buckets_in_calendar_order() {
        let history = seasonal_series(120);
        let out = project(inputs(&history, 14)).expect("120 daily buckets is well past the floor");
        assert_eq!(out.len(), 14);
        // Continues the history's calendar rather than restarting it.
        assert_eq!(out[0].date, NaiveDate::from_ymd_opt(2026, 5, 1).unwrap());
        for pair in out.windows(2) {
            assert_eq!(pair[1].date, pair[0].date + chrono::Duration::days(1));
        }
    }

    #[test]
    fn every_point_sits_inside_its_own_interval() {
        let out = project(inputs(&seasonal_series(120), 7)).expect("fits");
        for bucket in &out {
            assert!(
                bucket.lower <= bucket.point && bucket.point <= bucket.upper,
                "point {} outside [{}, {}]",
                bucket.point,
                bucket.lower,
                bucket.upper
            );
        }
    }

    /// The whole reason this module has a gate: 21 daily buckets clears MSTL's
    /// algebraic floor and would happily project a ramp forward.
    #[test]
    fn refuses_a_series_under_the_statistical_floor() {
        let err = project(inputs(&seasonal_series(21), 7)).unwrap_err();
        assert_eq!(
            err,
            ProjectError::NotEnoughHistory {
                needed: gates::min_history_buckets(Granularity::Day, &[7]),
                got: 21,
            }
        );
    }

    /// Gaps are not history. A window long enough only because most of it was
    /// invented must refuse exactly as a short one does.
    #[test]
    fn imputed_buckets_do_not_count_toward_the_floor() {
        let dense = seasonal_series(120);
        // Keep the first 20 and the last, so `fill_gaps` invents the middle.
        let mut sparse: Vec<_> = dense[..20].to_vec();
        sparse.push(*dense.last().unwrap());
        let err = project(inputs(&sparse, 7)).unwrap_err();
        assert!(
            matches!(err, ProjectError::NotEnoughHistory { got: 21, .. }),
            "measured buckets should be counted, got {err:?}"
        );
    }

    #[test]
    fn refuses_a_zero_horizon_before_touching_the_series() {
        assert_eq!(
            project(inputs(&[], 0)).unwrap_err(),
            ProjectError::ZeroHorizon
        );
    }

    /// Monthly asks for two cycles, not eight — eight years would disable it.
    #[test]
    fn monthly_uses_the_monthly_floor() {
        let start = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
        let history: Vec<(NaiveDate, f64)> = (0..30)
            .scan(start, |date, i| {
                let current = *date;
                *date = advance_period(*date, Granularity::Month);
                Some((current, 1000.0 + (i % 12) as f64 * 50.0))
            })
            .collect();
        let out = project(ProjectInputs {
            history: &history,
            granularity: Granularity::Month,
            seasonal_periods: Vec::new(),
            horizon: 3,
            interval_level: DEFAULT_INTERVAL_LEVEL,
        })
        .expect("30 monthly buckets clears the 24-bucket monthly floor");
        assert_eq!(out.len(), 3);
        // Month starts, not 31-day jumps.
        assert_eq!(out[0].date, NaiveDate::from_ymd_opt(2022, 7, 1).unwrap());
    }
}
