//! Re-measure a world's declared truth from the rows it actually emits.
//!
//! A world whose realized parameters differ from its declared ones invalidates
//! every convergence claim downstream, and does it **silently** — a drifted world
//! is still a well-formed world, and the run still draws a confident-looking
//! curve. This is the same contract `gen_restaurant_data.py --check` holds, for
//! the same reason.
//!
//! Two measurements, and the difference between them is the point of the whole
//! exercise:
//!
//! * **Interventional** — run the same seed at several spend levels and difference
//!   the sales. Because every stream is labelled and seeded, base, shock and noise
//!   are *identical* across those runs, so the difference is the mechanism alone.
//!   This recovers θ and the scale exactly, and it is what gets asserted.
//! * **Observational** — the within-panel OLS a fitter would run on the same
//!   history. This is *reported*, never asserted: the gap between it and the truth
//!   is the finding, not a defect.

use crate::SimulationError;
use crate::spec::{ResponseCurve, SimulationSpec};
use crate::world::{EntityDay, World};

/// What a world really contains, measured rather than declared.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldCheck {
    pub theta_declared: f64,
    pub theta_recovered: f64,
    pub scale_declared: f64,
    pub scale_recovered: f64,
    /// Within-panel OLS slope of sales on lagged spend, over the burn-in
    /// history — paired on the **declared** lag, the way a fitter would.
    pub observational_slope: f64,
    /// Where the world actually settled: mean spend per entity-day over history.
    ///
    /// Reported because it is generally **not** the anchor the curve was
    /// calibrated at — a budget set as a share of revenue is a fixed point, since
    /// spend raises sales which raises the budget. Scoring a fit against the
    /// anchor slope instead of the slope here compares it to a point the world
    /// never occupied.
    pub mean_spend: f64,
    /// The true marginal response at `mean_spend` — what an unbiased fit on this
    /// history should land on.
    pub true_local_slope: f64,
    /// Paired observations the observational fit had.
    pub n_pairs: usize,
}

impl WorldCheck {
    /// How far an observational fit lands from the truth, as a multiple.
    ///
    /// `1.0` is unbiased. Above `1.0` the history overstates what a unit of spend
    /// buys — which is what a budget set from trailing sales produces, and what
    /// makes a policy that trusts it overspend.
    pub fn bias_ratio(&self) -> f64 {
        if self.true_local_slope == 0.0 {
            return f64::NAN;
        }
        self.observational_slope / self.true_local_slope
    }
}

impl std::fmt::Display for WorldCheck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "  interventional   θ {:.6} (declared {:.6})  scale {:.4} (declared {:.4})",
            self.theta_recovered, self.theta_declared, self.scale_recovered, self.scale_declared
        )?;
        writeln!(
            f,
            "  observational    slope {:.4} over {} pairs",
            self.observational_slope, self.n_pairs
        )?;
        write!(
            f,
            "  truth            local slope {:.4} at the settled spend {:.2}  →  bias {:.2}×",
            self.true_local_slope,
            self.mean_spend,
            self.bias_ratio()
        )
    }
}

/// Tolerance on the interventional recovery. Tight on purpose: this is arithmetic
/// against a closed form, not an estimate, so anything past rounding means the
/// world is not emitting the mechanism it declares.
const RECOVERY_TOL: f64 = 1e-6;

/// Measure a world, and fail if its emitted mechanism has drifted from its spec.
pub fn check(spec: &SimulationSpec) -> Result<WorldCheck, SimulationError> {
    let curve = spec.curve()?;
    let report = measure(spec, &curve)?;

    if (report.theta_recovered - report.theta_declared).abs() > RECOVERY_TOL {
        return Err(SimulationError::Drift(format!(
            "the emitted world has θ {:.9}, the spec declares {:.9}. Every convergence claim \
             from this world would be scored against a curve it does not contain.",
            report.theta_recovered, report.theta_declared
        )));
    }
    let scale_error = (report.scale_recovered / report.scale_declared - 1.0).abs();
    if scale_error > RECOVERY_TOL {
        return Err(SimulationError::Drift(format!(
            "the emitted world has response scale {:.9}, the spec declares {:.9} ({:.4}% out).",
            report.scale_recovered,
            report.scale_declared,
            scale_error * 100.0
        )));
    }
    Ok(report)
}

fn measure(spec: &SimulationSpec, curve: &ResponseCurve) -> Result<WorldCheck, SimulationError> {
    // Three levels, because two points cannot separate a curve's exponent from
    // its scale.
    let multiples = [1.0_f64, 2.0, 4.0];
    let mut totals = Vec::with_capacity(multiples.len());
    for m in multiples {
        totals.push(sales_at_flat_spend(spec, m)?);
    }
    let responding_rows = responding_rows_per_window(spec);

    let s = multiples.map(|m| m * curve.anchor_spend);
    let d1 = totals[1] - totals[0];
    let d2 = totals[2] - totals[0];
    // When `2 * period_days <= lag_days`, `sales_at_flat_spend`'s second `step`
    // is still paying out `warm_up`'s burn-in spend for every day it measures
    // (see `responding_rows_per_window`). Every window then returns the same
    // total, d1 is exactly 0.0, and `target = d2 / d1` is NaN — a value
    // `solve_theta`'s `<=`/`>=` range guard does not catch, since both
    // comparisons are false against NaN. Bisection would then run anyway and
    // converge on a bogus theta instead of refusing.
    //
    // This is the ONLY place the period/lag relation is enforced, and it is
    // enforced at `2 * period_days > lag_days` — not at `period_days >=
    // lag_days`, which is the *worse* measurement, not an impossible one. It is
    // deliberately not in `SimulationSpec::validate`: the limitation belongs to
    // this measurement window, not to the world — the runner pops one matured
    // spend entry per day, so a short period runs correctly and only
    // `sales_at_flat_spend` cannot measure it. Refusing it at load would gate
    // every `from_yaml`/`from_value` and delete the daily-decision-against-
    // weekly-lag regime this crate exists to measure.
    //
    // States the measurement first and the diagnosis second, so a `d1` that
    // arrives non-finite by some other route is not misattributed.
    if d1 == 0.0 || !d1.is_finite() {
        return Err(SimulationError::Spec(format!(
            "the interventional difference between the flat-spend windows came out {d1}, so \
             no response can be recovered from it — a world that does not move when spend \
             does. Check period_days {} against mechanism.lag_days {}: two periods no longer \
             than the lag leave every measurement window still draining burn-in spend, \
             which produces exactly this. If two periods already cover the lag, the world's \
             emitted rows are not responding to the lever at all.",
            spec.period_days, spec.mechanism.lag_days
        )));
    }
    let theta_recovered = solve_theta(&s, d2 / d1)?;
    let scale_recovered =
        d1 / (responding_rows as f64 * (s[1].powf(theta_recovered) - s[0].powf(theta_recovered)));

    let history = World::new(spec.clone())?.warm_up();
    // The **declared** lag, because this measurement exists to mirror what a
    // fitter would get, and a fitter only ever knows what the `.view.yml`
    // claims. On a lag-error world the two differ, and the gap between this
    // slope and the truth is exactly the cost of the customer's wrong guess.
    let (observational_slope, n_pairs) =
        within_panel_slope(&history, spec.mechanism.declared_lag());
    let mean_spend = mean(
        &history
            .iter()
            .map(|r| r.marketing_spend)
            .collect::<Vec<_>>(),
    );

    Ok(WorldCheck {
        theta_declared: curve.theta,
        theta_recovered,
        scale_declared: curve.scale,
        scale_recovered,
        observational_slope,
        mean_spend,
        true_local_slope: curve.local_slope(mean_spend),
        n_pairs,
    })
}

/// Total sales over one measurement window at a flat spend.
///
/// A fresh world per call, so every labelled stream restarts at the same place
/// and the only thing that differs between calls is the spend. Two `step`s: the
/// first is still paying out lagged burn-in spend.
fn sales_at_flat_spend(spec: &SimulationSpec, multiple: f64) -> Result<f64, SimulationError> {
    let mut world = World::new(spec.clone())?;
    world.warm_up();
    let spend = vec![world.anchor_spend() * multiple; world.entity_count()];
    world.step(&spend);
    let rows = world.step(&spend);
    Ok(rows.iter().map(|r| r.net_sales).sum())
}

/// How many of the measurement window's rows actually responded to the flat
/// spend — the divisor `d1` has to be scaled by, and **not** the size of the
/// window.
///
/// The two differ, and the difference is the whole reason this is a named
/// function. `generate_day` pushes today's spend onto a queue holding
/// `lag_days` entries and pops the matured one, so the spend landing on day `d`
/// was chosen on day `d − L`. `sales_at_flat_spend` burns in `H` days under the
/// legacy rule, then holds a flat spend from day `H` onward and returns the
/// *second* period, days `H + P … H + 2P − 1`. A day in that window responds
/// only once `d − L >= H`, i.e. `d >= H + L`, which leaves
/// `2P − max(P, L)` responding days per entity:
///
/// * `P >= L` — all `P` days respond, and this equals the window.
/// * `L/2 < P < L` — only `2P − L` do; the rest are still paying out burn-in
///   spend that is identical across the three runs and so cancels out of `d1`
///   entirely.
/// * `2P <= L` — none do, and `d1` is exactly `0.0`; `measure`'s guard refuses
///   there rather than dividing by zero.
///
/// Dividing by the full window in the middle band would understate the
/// recovered scale by exactly `(2P − L) / P` and make `check` report `Drift` —
/// documented as "always a bug in the engine, never in a spec" — for a world
/// `SimulationSpec::validate` deliberately admits. θ never noticed, because
/// `d1` and `d2` carry the same row count and their ratio is invariant, so this
/// was visible only in the scale.
fn responding_rows_per_window(spec: &SimulationSpec) -> usize {
    let period = spec.period_days as i64;
    let lag = spec.mechanism.lag_days as i64;
    let days = (2 * period - period.max(lag)).max(0);
    days as usize * spec.entities.count as usize
}

/// Recover θ from the ratio of two interventional differences.
///
/// `(s₂^θ − s₀^θ) / (s₁^θ − s₀^θ)` rises monotonically in θ over `(0, 1)`, so
/// bisection is enough and cannot land on the wrong root.
fn solve_theta(s: &[f64; 3], target: f64) -> Result<f64, SimulationError> {
    let ratio =
        |theta: f64| (s[2].powf(theta) - s[0].powf(theta)) / (s[1].powf(theta) - s[0].powf(theta));
    let (mut lo, mut hi) = (1e-9_f64, 1.0 - 1e-9);
    if target <= ratio(lo) || target >= ratio(hi) {
        return Err(SimulationError::Drift(format!(
            "interventional differences imply a response ratio of {target:.6}, outside the \
             ({:.6}, {:.6}) a saturating curve can produce — the world is not emitting a power \
             response at all.",
            ratio(lo),
            ratio(hi)
        )));
    }
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if ratio(mid) < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Ok(0.5 * (lo + hi))
}

/// Within-panel demeaned OLS of `sales(d + lag)` on `spend(d)`.
///
/// Written out rather than borrowed from the engine under test: if the scorer
/// shared code with `fit_one_edge`, a shared bug would make the two agree and the
/// run would certify a broken fitter.
fn within_panel_slope(rows: &[EntityDay], lag_days: u32) -> (f64, usize) {
    let mut panels: std::collections::BTreeMap<u32, Vec<&EntityDay>> = Default::default();
    for row in rows {
        panels.entry(row.entity_id).or_default().push(row);
    }

    let mut pairs: Vec<(f64, f64)> = Vec::new();
    let mut demeaned: Vec<(f64, f64)> = Vec::new();
    for entity_rows in panels.values() {
        let mut sorted: Vec<&EntityDay> = entity_rows.clone();
        sorted.sort_by_key(|r| r.date);

        // Day d's spend against day d+lag's sales. A day with no partner carries
        // no lead-lag information and is dropped, never zero-filled.
        let mut panel: Vec<(f64, f64)> = Vec::new();
        for (i, row) in sorted.iter().enumerate() {
            let Some(target) = sorted.get(i + lag_days as usize) else {
                break;
            };
            panel.push((row.marketing_spend, target.net_sales));
        }
        if panel.len() < 2 {
            continue;
        }

        let x_bar = mean(&panel.iter().map(|p| p.0).collect::<Vec<_>>());
        let y_bar = mean(&panel.iter().map(|p| p.1).collect::<Vec<_>>());
        for (x, y) in &panel {
            demeaned.push((x - x_bar, y - y_bar));
        }
        pairs.extend(panel);
    }

    let sxx: f64 = demeaned.iter().map(|(x, _)| x * x).sum();
    let sxy: f64 = demeaned.iter().map(|(x, y)| x * y).sum();
    let slope = if sxx.abs() < f64::EPSILON {
        f64::NAN
    } else {
        sxy / sxx
    };
    (slope, pairs.len())
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{
        BaselineSpec, CalibrateSpec, DEFAULT_BUDGET_JITTER_SD, EntitiesSpec, LeverSpec,
        MechanismSpec,
    };
    use chrono::NaiveDate;

    fn spec() -> SimulationSpec {
        SimulationSpec {
            name: "check".into(),
            description: None,
            seed: 7,
            replicates: 1,
            periods: 10,
            period_days: 7,
            history_days: 180,
            start_date: NaiveDate::from_ymd_opt(2025, 1, 6).unwrap(),
            entities: EntitiesSpec {
                count: 24,
                scale_sigma: 0.4,
            },
            baseline: BaselineSpec {
                sales_per_entity_day: 1_500.0,
                margin: 0.36,
                demand_shock_rho: 0.7,
                demand_shock_sd: 0.12,
                weekly_seasonality: 0.15,
                budget_jitter_sd: DEFAULT_BUDGET_JITTER_SD,
            },
            mechanism: MechanismSpec {
                driver: "marketing_spend".into(),
                target: "net_sales".into(),
                lag_days: 7,
                declared_lag_days: None,
                noise_ratio: 0.05,
                calibrate: CalibrateSpec {
                    anchor_spend_share: 0.02,
                    local_slope_at_anchor: 4.0,
                    optimum_at: 3.0,
                },
            },
            lever: LeverSpec::default(),
        }
    }

    #[test]
    fn intervention_recovers_the_declared_curve_exactly() {
        // Exact, not approximate: differencing two seeded runs cancels base,
        // shock and noise outright, so what is left is the mechanism alone. If
        // this ever becomes noisy, the labelled streams have stopped being
        // independent and every other guarantee here is already gone.
        let report = check(&spec()).unwrap();
        assert!(
            (report.theta_recovered - report.theta_declared).abs() < RECOVERY_TOL,
            "θ {} vs declared {}",
            report.theta_recovered,
            report.theta_declared
        );
        assert!(
            (report.scale_recovered / report.scale_declared - 1.0).abs() < RECOVERY_TOL,
            "scale {} vs declared {}",
            report.scale_recovered,
            report.scale_declared
        );
    }

    #[test]
    fn the_observational_fit_is_biased_upward_by_the_legacy_budget() {
        // The finding the whole plan turns on, asserted as a property of the
        // world rather than hoped for at run time: a budget set from trailing
        // sales makes spend track the demand shock, and a fit that sees only the
        // history reads that correlation as marketing working better than it does.
        let report = check(&spec()).unwrap();
        eprintln!("confounded world:\n{report}");
        assert!(
            report.n_pairs > 1_000,
            "too few pairs to say anything: {}",
            report.n_pairs
        );
        // A loose floor on purpose. The magnitude of the bias is a property of
        // the constants and will move whenever a world is retuned; what must not
        // move is the *sign*. The comparative test below is the strong one — it
        // holds the estimator fixed and varies only the confounder.
        assert!(
            report.bias_ratio() > 1.02,
            "expected the confounded history to overstate the truth, got a ratio of {:.3} \
             (observational {:.3} vs true local {:.3})",
            report.bias_ratio(),
            report.observational_slope,
            report.true_local_slope
        );
    }

    #[test]
    fn a_world_with_no_confounding_is_far_less_biased() {
        // The control. Kill the persistence in the demand shock and the legacy
        // rule has nothing to track, so the same estimator lands much closer.
        // Without this, "biased" above could just mean "this estimator is broken".
        let mut clean = spec();
        clean.baseline.demand_shock_rho = 0.0;
        clean.baseline.demand_shock_sd = 0.01;

        let confounded = check(&spec()).unwrap().bias_ratio();
        let clean_ratio = check(&clean).unwrap().bias_ratio();
        assert!(
            (clean_ratio - 1.0).abs() < (confounded - 1.0).abs(),
            "removing the confounder did not reduce the bias: clean {clean_ratio:.3} vs \
             confounded {confounded:.3}"
        );
    }

    #[test]
    fn a_period_shorter_than_the_lag_is_refused_not_reported_as_drift() {
        // period_days: 1 against lag_days: 7 is a legal daily-decision world by
        // the type system, but `sales_at_flat_spend`'s two `step`s no longer
        // clear the burn-in queue: the second step is still paying out spend
        // `warm_up` chose, so every measurement window returns the same total,
        // d1 comes out exactly 0.0, and `target = d2 / d1` is NaN. Both arms of
        // `solve_theta`'s range guard (`<=`/`>=`) are false against NaN, so
        // bisection used to run anyway, converge on `lo`, and report a bogus
        // "θ 0.000000001" drift instead of refusing outright.
        //
        // The spec is mutated directly rather than round-tripped through
        // `from_yaml`, so `validate` never runs — which is the point: this
        // relation is enforced HERE and nowhere else (see the guard's comment
        // in `measure`), so the assertion below can only be satisfied by the
        // `d1` guard. Were the rule ever moved back into `validate`, this test
        // would still pass while testing something else entirely.
        let mut short_period = spec();
        short_period.period_days = 1;
        let err = check(&short_period).expect_err("period_days < lag_days must be refused");
        let msg = err.to_string();
        assert!(
            msg.contains("the interventional difference"),
            "the refusal did not come from `measure`'s d1 guard: {msg}"
        );
        assert!(
            msg.contains("period_days") && msg.contains("lag_days"),
            "error does not name the period/lag relation: {msg}"
        );
        assert!(
            !msg.contains("θ"),
            "a measurement-window failure was reported as a theta drift instead: {msg}"
        );
    }

    #[test]
    fn the_scale_is_recovered_across_the_whole_period_lag_band() {
        // `sales_at_flat_spend` measures the second of two periods, and the
        // world pops one matured spend entry per day, so only `2P − max(P, L)`
        // days of that window have actually seen the flat spend. Dividing the
        // interventional difference by the *full* window instead understates
        // the recovered scale by exactly `(2P − L) / P` — 16.7% at P = 6,
        // 40% at P = 5, 75% at P = 4 against L = 7 — and `check` reports that
        // as `Drift`, i.e. an engine bug, for a world `SimulationSpec::validate`
        // deliberately admits (see the block comment at spec.rs's period/lag
        // check). The whole band must recover the declared scale exactly.
        for period_days in [7_u32, 6, 5, 4] {
            let mut s = spec();
            s.period_days = period_days;
            let report = check(&s).unwrap_or_else(|e| {
                panic!("period_days {period_days} against lag_days 7 must measure, got: {e}")
            });
            let scale_error = (report.scale_recovered / report.scale_declared - 1.0).abs();
            assert!(
                scale_error < RECOVERY_TOL,
                "period_days {period_days}: scale {} vs declared {} ({:.4}% out)",
                report.scale_recovered,
                report.scale_declared,
                scale_error * 100.0
            );
            // θ was never affected — d1 and d2 scale by the same row count, so
            // their ratio is invariant. Asserted so a fix to the divisor cannot
            // quietly break the exponent instead.
            assert!(
                (report.theta_recovered - report.theta_declared).abs() < RECOVERY_TOL,
                "period_days {period_days}: θ {} vs declared {}",
                report.theta_recovered,
                report.theta_declared
            );
        }

        // Below `2P > L` no day of the window has responded, d1 is exactly 0.0,
        // and there is nothing to recover from. That stays a `Spec` refusal — a
        // limit of the measurement window, not a defect in the engine.
        for period_days in [3_u32, 2, 1] {
            let mut s = spec();
            s.period_days = period_days;
            let err = check(&s).expect_err("2 * period_days <= lag_days cannot be measured");
            assert!(
                matches!(err, SimulationError::Spec(_)),
                "period_days {period_days} must refuse as a Spec limit, got: {err}"
            );
        }
    }

    #[test]
    fn drift_in_the_emitted_mechanism_is_caught() {
        // Simulate the regression this check exists to catch: a world that emits
        // a different curve from the one it declares. Recovering from the rows
        // must disagree with the spec.
        let base = spec();
        let steeper = {
            let mut s = base.clone();
            s.mechanism.calibrate.optimum_at = 5.0;
            s
        };
        let a = check(&base).unwrap();
        let b = check(&steeper).unwrap();
        assert!(
            (a.theta_recovered - b.theta_recovered).abs() > 1e-3,
            "two different declared curves recovered the same θ — the check cannot see the \
             mechanism at all"
        );
    }
}
