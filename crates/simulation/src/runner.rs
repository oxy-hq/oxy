//! The loop: generate a period, ask the model, act, score.
//!
//! Everything the loop touches the outside world through is a trait —
//! [`RowSink`] for where rows land, [`SemanticProbe`] for what the model says.
//! The app layer supplies the real pair (a materialised workspace and
//! `fit_driver_coefficients` over it); the tests here supply stubs. That split
//! is not tidiness: it is what lets "the policy honours a refusal" be asserted
//! without standing up a warehouse, and what keeps `oxy-simulation` free of the
//! engine it exists to measure.
//!
//! # Where truth is allowed to be
//!
//! The loop is the scorer, so it holds the world's [`ResponseCurve`] and writes
//! it into [`FitScore`]. It must never reach the other way: nothing the policy
//! receives is derived from the curve. The `PeriodObservation` built inside
//! [`Runner::run`] is the only place that boundary could be broken, which is
//! why it is spelled out inline rather than hidden behind a helper.
//!
//! Note this cannot be checked by comparing two worlds — a different true curve
//! generates different rows, so a policy legitimately acts differently without
//! ever having been told anything. The test that guards it asserts the
//! observation carries the probe's answer *verbatim*.

use crate::SimulationError;
use crate::policy::{EdgeFit, PeriodObservation, Policy};
use crate::spec::ResponseCurve;
use crate::world::{EntityDay, TRAILING_WINDOW, World, total_profit, trailing_mean_sales};

/// Where a period's rows land so the semantic layer can read them back.
pub trait RowSink {
    fn append(&mut self, rows: &[EntityDay]) -> Result<(), SimulationError>;
}

/// What the model says about the history written so far.
pub trait SemanticProbe {
    /// Fit every declared edge over the rows the sink holds, and report whether
    /// `predict` could size the candidate move.
    fn probe(&mut self) -> Result<Probe, SimulationError>;
}

/// One period's answer from the model, verbatim.
#[derive(Debug, Clone, Default)]
pub struct Probe {
    /// `(edge label, fit)`. An edge the baseline returned no row for is absent
    /// rather than present-and-refused — a stronger silence, and the policy
    /// treats both the same way.
    pub fits: Vec<(String, EdgeFit)>,
    /// False when `predict` degraded the impact to `unquantifiable`.
    pub impact_quantified: bool,
}

/// The three outcomes a run reports. Only a simulation can tell the last two
/// apart: on real data `converged` and `confidently_wrong` are the same
/// response, byte for byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The model declined. Working as designed — costs an opportunity, not money.
    Refused,
    /// β̂ landed on β_true. The win.
    Converged,
    /// A confident, plausible, wrong number. The only outcome that hurts a
    /// customer, and the one nothing in production can detect.
    ConfidentlyWrong,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Refused => "refused",
            Outcome::Converged => "converged",
            Outcome::ConfidentlyWrong => "confidently_wrong",
        }
    }

    /// Classify a fit against the truth.
    ///
    /// `tolerance` is relative, because what matters to a policy is whether
    /// `m·β̂` lands on the right side of break-even, and that is a proportional
    /// question. The threshold is a scoring choice with no principled value
    /// until the grid has been run — [`CONVERGENCE_TOLERANCE`] is a starting
    /// point, and Phase 3 is what calibrates it.
    /// `fit` is read through [`EdgeFit::level_slope`], never raw: the fitter
    /// picks a form by AIC, so a coefficient may be an elasticity, and scoring
    /// one against a level slope is wrong by `target / driver` — silently, and
    /// in a way that looks like the estimator being badly biased.
    pub fn classify(fit: &EdgeFit, true_local_slope: f64, tolerance: f64) -> Self {
        // A fit whose coefficient exists but cannot be read as a marginal
        // effect is a refusal for scoring purposes too: there is no number to
        // compare, and calling it `confidently_wrong` would blame the estimator
        // for a missing operating point.
        let Some(beta) = fit.level_slope() else {
            return Outcome::Refused;
        };
        // A non-finite β̂ is `Refused` for the same reason a missing one is, and
        // the choice matters because the fallthrough is not neutral: both
        // `NaN <= tolerance` and `inf <= tolerance` are false, so without this
        // the fit lands on `ConfidentlyWrong` — the outcome documented above as
        // the only one that hurts a customer, and the headline of every outcome
        // map — on the strength of no number at all. `Converged` would be worse
        // still. `Refused` is what the fit actually did: it produced nothing
        // readable as a marginal effect, which is the same conclusion
        // `Machine::direction` reaches on the identical fit (`policy.rs`, where
        // `!beta.is_finite()` returns `None`). Scoring and acting must agree on
        // that, or one run reports the estimator's worst failure for a fit the
        // policy treated as its safest.
        //
        // Not reachable from a legal world today — the world emits finite
        // bounded rows and a collinear fit refuses cleanly below
        // `metric_tree_fit`'s pivot floor — but the plumbing carries it: that
        // fitter gates on `t.abs() < MIN_FIT_T`, and `NaN.abs() < 2.0` is
        // false, so a NaN-t fit passes the gate and arrives here unchecked.
        if !beta.is_finite() {
            return Outcome::Refused;
        }
        if true_local_slope == 0.0 || !true_local_slope.is_finite() {
            return Outcome::ConfidentlyWrong;
        }
        if (beta / true_local_slope - 1.0).abs() <= tolerance {
            Outcome::Converged
        } else {
            Outcome::ConfidentlyWrong
        }
    }
}

/// How close β̂ must sit to β_true to count as converged.
///
/// 20% is deliberately generous. The finding this exists to surface is the
/// *shape* of the outcome map — where the gate stops protecting you — and a
/// tight threshold would paint the whole map `confidently_wrong` and say
/// nothing. Confounding alone is already worth ~7% on the declared worlds.
pub const CONVERGENCE_TOLERANCE: f64 = 0.20;

/// One edge, scored.
#[derive(Debug, Clone)]
pub struct FitScore {
    pub edge: String,
    pub fit: EdgeFit,
    /// The true marginal response at the mean spend over **the fit's own
    /// window** — the rows the probe had when it answered, not the anchor the
    /// curve was calibrated from and not the history one period later.
    ///
    /// Both other readings book something that is not estimator error as
    /// estimator error. The anchor books a modelling difference. The
    /// one-period-later window books the policy's own next move: the period
    /// being scored is chosen *after* the fit, so folding its rows in shifts
    /// the scoring point toward wherever the policy just went, and on a
    /// saturating curve that lowers the slope. A climbing arm therefore reads
    /// as β̂ too high — the direction that turns `Converged` into
    /// `ConfidentlyWrong`, which is the one outcome this crate exists to count.
    pub true_local_slope: f64,
    pub outcome: Outcome,
}

/// One period of a run, ready to persist.
#[derive(Debug, Clone)]
pub struct PeriodResult {
    pub period: u32,
    pub mean_spend: f64,
    pub realized_profit: f64,
    pub cumulative_profit: f64,
    /// Per-entity spend. A mean cannot answer how much variation an `explore`
    /// arm left behind, which is the question that arm exists to answer.
    pub actions: Vec<f64>,
    pub fits: Vec<FitScore>,
}

/// What a finished run amounts to.
#[derive(Debug, Clone)]
pub struct RunSummary {
    pub periods: u32,
    pub cumulative_profit: f64,
    pub truth: ResponseCurve,
}

pub struct Runner<'a, S: RowSink, P: SemanticProbe> {
    world: &'a mut World,
    policy: &'a mut dyn Policy,
    sink: &'a mut S,
    probe: &'a mut P,
    tolerance: f64,
}

impl<'a, S: RowSink, P: SemanticProbe> Runner<'a, S, P> {
    pub fn new(
        world: &'a mut World,
        policy: &'a mut dyn Policy,
        sink: &'a mut S,
        probe: &'a mut P,
    ) -> Self {
        Self {
            world,
            policy,
            sink,
            probe,
            tolerance: CONVERGENCE_TOLERANCE,
        }
    }

    pub fn with_tolerance(mut self, tolerance: f64) -> Self {
        self.tolerance = tolerance;
        self
    }

    /// Run every declared period, calling `on_period` as each one lands.
    ///
    /// `on_period` is where persistence and the SSE event go. It fires *per
    /// period* rather than once at the end so a run is resumable and the panel
    /// can stream — a 40-period run is minutes of warehouse queries, and a
    /// result that only exists at the end is one an instance death destroys.
    pub fn run(
        &mut self,
        mut on_period: impl FnMut(&PeriodResult) -> Result<(), SimulationError>,
    ) -> Result<RunSummary, SimulationError> {
        // The history a customer already has when we arrive, under the legacy
        // budget rule. Written before the first probe, so period 1 fits on a
        // real past instead of refusing for lack of one.
        let history = self.world.warm_up();
        self.sink.append(&history)?;

        let entity_count = self.world.entity_count();
        let mut spend = spend_on_last_day(&history, entity_count);
        let mut all_rows = history;
        let mut cumulative_profit = 0.0;

        let periods = self.world.spec().periods;
        for period in 1..=periods {
            let probe = self.probe.probe()?;
            // The window the probe just fitted over, measured before this
            // period adds a row to it. `FitProbe` reads the whole dataset dir
            // with no date bound, so its sample is exactly what the sink holds
            // at this instant — and that is `WorldCheck::mean_spend`'s
            // convention too: the mean over the rows the fit actually ran on.
            let fit_window_mean_spend = mean_spend(&all_rows);

            // Everything the policy is allowed to know, and nothing derived
            // from the world's true parameters. This is the only place that
            // invariant could be broken, which is why it is spelled out here
            // rather than hidden behind a helper.
            let trailing = trailing_mean_sales(&all_rows, entity_count, TRAILING_WINDOW);
            let next = self.policy.decide(&PeriodObservation {
                current_spend: &spend,
                trailing_sales: &trailing,
                fit: probe.fits.first().map(|(_, f)| f.clone()),
                impact_quantified: probe.impact_quantified,
            });
            spend = next;

            let rows = self.world.step(&spend);
            self.sink.append(&rows)?;

            let realized_profit = total_profit(&rows);
            cumulative_profit += realized_profit;
            all_rows.extend_from_slice(&rows);

            let result = PeriodResult {
                period,
                mean_spend: mean(&spend),
                realized_profit,
                cumulative_profit,
                actions: spend.clone(),
                fits: self.score(&probe, fit_window_mean_spend),
            };
            on_period(&result)?;
        }

        Ok(RunSummary {
            periods,
            cumulative_profit,
            truth: self.world.truth(),
        })
    }

    /// `fit_window_mean_spend` must be the mean over the rows the probe was
    /// handed, not over the history as it stands once the period has run. See
    /// [`FitScore::true_local_slope`] for what the difference costs.
    fn score(&self, probe: &Probe, fit_window_mean_spend: f64) -> Vec<FitScore> {
        let curve = self.world.truth();
        let true_local_slope = curve.local_slope(fit_window_mean_spend);
        probe
            .fits
            .iter()
            .map(|(edge, fit)| FitScore {
                edge: edge.clone(),
                fit: fit.clone(),
                true_local_slope,
                outcome: Outcome::classify(fit, true_local_slope, self.tolerance),
            })
            .collect()
    }
}

/// Per-entity spend on the most recent date present in `rows`.
///
/// Read back off the rows rather than remembered, so the runner's idea of "what
/// we are spending now" can only ever be what the warehouse would report.
fn spend_on_last_day(rows: &[EntityDay], entity_count: usize) -> Vec<f64> {
    let mut out = vec![0.0; entity_count];
    let Some(last) = rows.iter().map(|r| r.date).max() else {
        return out;
    };
    for row in rows.iter().filter(|r| r.date == last) {
        if let Some(slot) = out.get_mut(row.entity_id as usize) {
            *slot = row.marketing_spend;
        }
    }
    out
}

fn mean_spend(rows: &[EntityDay]) -> f64 {
    if rows.is_empty() {
        return 0.0;
    }
    rows.iter().map(|r| r.marketing_spend).sum::<f64>() / rows.len() as f64
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

#[cfg(test)]
mod tests;
