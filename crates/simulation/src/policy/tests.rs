//! The three ways `machine` could quietly stop being the product, plus the
//! properties each other arm of the race is defined by.

use super::*;
use crate::spec::{
    BaselineSpec, CalibrateSpec, DEFAULT_BUDGET_JITTER_SD, EntitiesSpec, LeverSpec, MechanismSpec,
    SimulationSpec,
};
use chrono::NaiveDate;

const ENTITIES: usize = 4;
const MARGIN: f64 = 0.36;

fn spec() -> SimulationSpec {
    SimulationSpec {
        name: "policy".into(),
        description: None,
        seed: 7,
        replicates: 1,
        periods: 10,
        period_days: 7,
        history_days: 180,
        start_date: NaiveDate::from_ymd_opt(2025, 1, 6).unwrap(),
        entities: EntitiesSpec {
            count: ENTITIES as u32,
            scale_sigma: 0.4,
        },
        baseline: BaselineSpec {
            sales_per_entity_day: 1_500.0,
            margin: MARGIN,
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

fn build_for(policy: PolicyKind) -> (Box<dyn Policy>, ResponseCurve) {
    let spec = spec();
    let curve = spec.curve().unwrap();
    (build(policy, &spec, curve), curve)
}

/// A period where the model handed back a clean, decisive fit.
fn observation<'a>(
    current: &'a [f64],
    trailing: &'a [f64],
    fit: Option<EdgeFit>,
) -> PeriodObservation<'a> {
    PeriodObservation {
        current_spend: current,
        trailing_sales: trailing,
        fit,
        impact_quantified: true,
    }
}

fn flat(value: f64) -> Vec<f64> {
    vec![value; ENTITIES]
}

/// A slope whose marginal profit is unambiguously positive: `m·β − 1 = 0.8`,
/// with an `se` far too small for the dead band to swallow it.
fn clearly_profitable() -> EdgeFit {
    EdgeFit::fitted(5.0, 0.01)
}

/// `m·β − 1 = −0.28`, equally unambiguous in the other direction.
fn clearly_unprofitable() -> EdgeFit {
    EdgeFit::fitted(2.0, 0.01)
}

#[test]
fn machine_does_not_move_on_a_refusal() {
    // The first of the three. A refused edge shows the user nothing, so the
    // policy must show the world nothing.
    let (mut policy, _) = build_for(PolicyKind::Machine);
    let current = flat(100.0);
    let next = policy.decide(&observation(
        &current,
        &flat(1_500.0),
        Some(EdgeFit::refused("abs t < 2")),
    ));
    assert_eq!(next, current, "a refusal moved the lever");
}

#[test]
fn machine_treats_a_refusal_as_silence_not_as_a_zero_coefficient() {
    // The failure this is really guarding: β = 0 implies marginal profit of −1,
    // so a policy that defaulted the missing coefficient would not merely hold —
    // it would cut, hard, and the run would report the product losing money for
    // a reason the product never gave.
    let (mut policy, _) = build_for(PolicyKind::Machine);
    let current = flat(100.0);
    let refused = policy.decide(&observation(
        &current,
        &flat(1_500.0),
        Some(EdgeFit::refused("insufficient within-panel variation")),
    ));
    let zeroed = policy.decide(&observation(
        &current,
        &flat(1_500.0),
        Some(EdgeFit::fitted(0.0, 0.01)),
    ));

    assert_eq!(refused, current);
    assert!(
        zeroed[0] < current[0],
        "a genuine zero coefficient should cut spend, or this test proves nothing"
    );
    assert_ne!(refused, zeroed, "a refusal was read as a zero coefficient");
}

#[test]
fn machine_holds_when_the_impact_is_unquantifiable() {
    // The second. `predict` degrading to `unquantifiable` means the impact has a
    // direction and no size — acting on it is acting on a number that was never
    // produced.
    let (mut policy, _) = build_for(PolicyKind::Machine);
    let current = flat(100.0);
    let obs = PeriodObservation {
        current_spend: &current,
        trailing_sales: &flat(1_500.0),
        fit: Some(clearly_profitable()),
        impact_quantified: false,
    };
    assert_eq!(policy.decide(&obs), current);
}

#[test]
fn machine_holds_when_there_was_no_fit_row_at_all() {
    let (mut policy, _) = build_for(PolicyKind::Machine);
    let current = flat(100.0);
    assert_eq!(
        policy.decide(&observation(&current, &flat(1_500.0), None)),
        current
    );
}

#[test]
fn machine_respects_its_per_period_clip() {
    // The third. Under a linear fit the model believes marginal profit never
    // falls, so nothing but the clip stops it walking to the ceiling in one move.
    let (mut policy, _) = build_for(PolicyKind::Machine);
    let clip = LeverSpec::default().max_move_per_period;
    let current = flat(100.0);

    let next = policy.decide(&observation(
        &current,
        &flat(1_500.0),
        Some(clearly_profitable()),
    ));
    assert!(
        (next[0] - current[0] * (1.0 + clip)).abs() < 1e-9,
        "expected a {clip} move from {}, got {}",
        current[0],
        next[0]
    );
}

#[test]
fn machine_climbs_when_a_unit_of_spend_pays_and_cuts_when_it_does_not() {
    let (mut policy, _) = build_for(PolicyKind::Machine);
    let current = flat(100.0);
    let up = policy.decide(&observation(
        &current,
        &flat(1_500.0),
        Some(clearly_profitable()),
    ));
    let down = policy.decide(&observation(
        &current,
        &flat(1_500.0),
        Some(clearly_unprofitable()),
    ));
    assert!(up[0] > current[0], "did not climb on a profitable slope");
    assert!(down[0] < current[0], "did not cut on an unprofitable one");
}

#[test]
fn machine_holds_once_break_even_is_inside_the_confidence_interval() {
    // The settling mechanism, and the one that makes the predicted failure mode
    // reachable: the same slope moves the lever when it is measured precisely
    // and does not when it is not. Nothing about the point estimate changed.
    let (mut policy, _) = build_for(PolicyKind::Machine);
    let current = flat(100.0);
    // m·β − 1 = 0.08. Precise → act; noisy (se 0.5 → half-width 0.36) → hold.
    let precise = policy.decide(&observation(
        &current,
        &flat(1_500.0),
        Some(EdgeFit::fitted(3.0, 0.01)),
    ));
    let noisy = policy.decide(&observation(
        &current,
        &flat(1_500.0),
        Some(EdgeFit::fitted(3.0, 0.5)),
    ));
    assert!(precise[0] > current[0]);
    assert_eq!(noisy, current, "acted on a slope it could not resolve");
}

#[test]
fn machine_stays_inside_its_bounds() {
    let (mut policy, curve) = build_for(PolicyKind::Machine);
    let lever = LeverSpec::default();
    let ceiling = lever.max_multiple * curve.anchor_spend;
    let floor = lever.min_multiple * curve.anchor_spend;

    let mut spend = flat(curve.anchor_spend);
    for _ in 0..40 {
        spend = policy.decide(&observation(
            &spend,
            &flat(1_500.0),
            Some(clearly_profitable()),
        ));
    }
    assert!(spend[0] <= ceiling + 1e-9, "climbed past the ceiling");

    for _ in 0..80 {
        spend = policy.decide(&observation(
            &spend,
            &flat(1_500.0),
            Some(clearly_unprofitable()),
        ));
    }
    assert!(spend[0] >= floor - 1e-9, "fell through the floor");
    assert!(
        spend[0] > 0.0,
        "the floor must stay positive, or a multiplicative step can never climb back out"
    );
}

#[test]
fn hold_never_moves_whatever_the_model_says() {
    let (mut policy, _) = build_for(PolicyKind::Hold);
    let current = vec![10.0, 20.0, 30.0, 40.0];
    assert_eq!(
        policy.decide(&observation(
            &current,
            &flat(1_500.0),
            Some(clearly_profitable())
        )),
        current
    );
}

#[test]
fn legacy_budgets_from_trailing_sales_and_ignores_the_model() {
    let (mut policy, _) = build_for(PolicyKind::Legacy);
    let share = 0.02;
    let trailing = vec![1_000.0, 2_000.0, 3_000.0, 4_000.0];
    let next = policy.decide(&observation(&flat(30.0), &trailing, None));

    // Jittered, so assert the rule rather than the number: spend rises with
    // trailing sales, and sits in the right neighbourhood of the share.
    assert!(
        next[3] > next[0],
        "legacy spend did not track trailing sales: {next:?}"
    );
    for (spend, sales) in next.iter().zip(&trailing) {
        let implied = spend / sales;
        assert!(
            (implied - share).abs() < share * 0.5,
            "implied share {implied} is nowhere near {share}"
        );
    }
}

#[test]
fn oracle_holds_the_true_optimum() {
    let (mut policy, curve) = build_for(PolicyKind::Oracle);
    let next = policy.decide(&observation(&flat(1.0), &flat(1_500.0), None));
    assert!(
        next.iter().all(|s| (s - curve.optimum_spend).abs() < 1e-9),
        "oracle sat at {next:?}, not the optimum {}",
        curve.optimum_spend
    );
}

#[test]
fn explore_keeps_variation_alive_on_the_periods_the_machine_holds() {
    // The whole point of the arm. A perturbation that stopped when the policy
    // stopped would leave the estimator with nothing at exactly the moment the
    // machine settles — which is the failure mode it exists to answer.
    let (mut policy, _) = build_for(PolicyKind::MachineExplore);
    let current = flat(100.0);
    let next = policy.decide(&observation(
        &current,
        &flat(1_500.0),
        Some(EdgeFit::refused("abs t < 2")),
    ));

    assert_ne!(next, current, "explore went quiet with the machine");
    let spread = next.iter().fold(f64::NEG_INFINITY, |a, b| a.max(*b))
        - next.iter().fold(f64::INFINITY, |a, b| a.min(*b));
    assert!(
        spread > 0.0,
        "explore moved every entity identically, which identifies nothing: {next:?}"
    );
}

#[test]
fn explore_stays_inside_the_same_bounds_as_the_machine() {
    let (mut policy, curve) = build_for(PolicyKind::MachineExplore);
    let lever = LeverSpec::default();
    let mut spend = flat(curve.anchor_spend);
    for _ in 0..60 {
        spend = policy.decide(&observation(
            &spend,
            &flat(1_500.0),
            Some(clearly_profitable()),
        ));
        assert!(
            spend
                .iter()
                .all(|s| *s <= lever.max_multiple * curve.anchor_spend + 1e-9),
            "jitter carried spend past the ceiling: {spend:?}"
        );
    }
}

/// The width of the panel these two tests measure across.
///
/// A cross-sectional sd read off `n` draws is itself noisy — its relative
/// standard error is `1/sqrt(2(n−1))`, which is ~15% here. Every tolerance
/// below is derived from that number rather than picked, so a failure means the
/// *level* moved and not that the sample was unlucky.
const PANEL: usize = 24;
const PANEL_SD_RSE: f64 = 0.1476; // 1 / sqrt(2 * (PANEL - 1))

fn flat_n(value: f64, n: usize) -> Vec<f64> {
    vec![value; n]
}

/// Opening spend with the heterogeneity a real warm-up leaves behind.
///
/// Flat opening spend would let a compounding walk hide: it would start at
/// log-sd 0 and the first period's jitter would look like the whole story.
/// Starting from the world's own entity spread (`scale_sigma`) is what makes
/// "did the spread grow?" a question with a baseline.
fn opening_spread(anchor: f64, n: usize) -> Vec<f64> {
    let mut rng = Rng::stream(7, "policy_explore_level_fixture");
    (0..n).map(|_| anchor * rng.lognormal(0.4)).collect()
}

/// Cross-sectional sd in log space — the scale the jitter is actually drawn on,
/// and the only one in which "a fixed-width spread" is a constant.
fn log_sd(values: &[f64]) -> f64 {
    let logs: Vec<f64> = values.iter().map(|v| v.ln()).collect();
    let mean = logs.iter().sum::<f64>() / logs.len() as f64;
    let var = logs.iter().map(|l| (l - mean).powi(2)).sum::<f64>() / (logs.len() - 1) as f64;
    var.sqrt()
}

fn arith_mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

/// Geometric mean — the lognormal jitter has unit *median*, so this estimates
/// the level the policy intended, where the arithmetic mean estimates that level
/// times `exp(σ²/2)`.
fn geo_mean(values: &[f64]) -> f64 {
    (values.iter().map(|v| v.ln()).sum::<f64>() / values.len() as f64).exp()
}

#[test]
fn explore_jitter_is_a_spread_around_a_level_not_a_compounding_walk() {
    // The property the arm's name claims and its other tests never checked: a
    // *spread*, which is a fixed width around a level, versus a *walk*, which is
    // a width that grows without bound because each period multiplies the last
    // period's realized value again.
    //
    // The machine is held silent here (every period gets a refused edge), so
    // 100% of the movement below is jitter. If this fails, the `machine+explore`
    // arm is not exploring around the machine's choice — it is diffusing away
    // from it, and every profit number recorded for that arm is really a number
    // about a random walk that happens to drift toward the optimum.
    let (mut policy, curve) = build_for(PolicyKind::MachineExplore);
    let sd = LeverSpec::default().explore_jitter_sd;
    let trailing = flat_n(1_500.0, PANEL);

    let mut spend = opening_spread(curve.anchor_spend, PANEL);
    let opening_sd = log_sd(&spend);

    let mut first = Vec::new();
    let mut last = Vec::new();
    for period in 1..=30 {
        spend = policy.decide(&observation(
            &spend,
            &trailing,
            Some(EdgeFit::refused("abs t < 2")),
        ));
        if period <= 3 {
            first.push(arith_mean(&spend));
        }
        if period > 27 {
            last.push(arith_mean(&spend));
        }
    }
    let final_sd = log_sd(&spend);

    // A fixed-width spread around a level sits at `sqrt(sd₀² + σ²)`: the opening
    // heterogeneity plus exactly one layer of jitter, however many periods run.
    // A compounding walk sits at `sqrt(sd₀² + t·σ²)` — the same expression with
    // the period count multiplying σ², which after 30 periods is more than twice
    // as wide. Three sampling standard errors is the room between them.
    let level = (opening_sd * opening_sd + sd * sd).sqrt();
    let ceiling = level * (1.0 + 3.0 * PANEL_SD_RSE);
    let walk = (opening_sd * opening_sd + 30.0 * sd * sd).sqrt();
    assert!(
        final_sd <= ceiling,
        "cross-sectional log-sd grew from {opening_sd:.4} to {final_sd:.4} over 30 periods. \
         A spread around a level would sit near {level:.4} (ceiling {ceiling:.4}); a compounding \
         walk predicts {walk:.4}, before the lever's clamp compresses it."
    );

    // And the level itself must not walk. The jitter draw has unit *median*, so
    // its mean is `exp(σ²/2) > 1` — compounded period on period that is a
    // systematic climb of ~1.1%/period, and `CalibrateSpec::solve` requires the
    // optimum to sit *above* the anchor in every legal world, so the climb is
    // always toward the answer. An arm that finds the optimum by drifting into
    // it is not measuring the product.
    let opening_mean = arith_mean(&first);
    let closing_mean = arith_mean(&last);
    let drift = closing_mean / opening_mean - 1.0;
    assert!(
        drift.abs() < 0.15,
        "mean spend drifted {:.2}% ({opening_mean:.2} → {closing_mean:.2}) with the machine \
         silent, against an anchor of {:.2} and a true optimum of {:.2}. Jitter must not have \
         a direction.",
        drift * 100.0,
        curve.anchor_spend,
        curve.optimum_spend
    );
}

#[test]
fn explore_still_climbs_at_the_machine_s_own_rate() {
    // The other half of the same fix, and the reason it cannot be satisfied by
    // simply freezing the arm at its opening level. Anchoring the jitter must
    // leave the machine's own trajectory intact: six clipped moves on a clearly
    // profitable slope is `1.25^6` and nothing less.
    //
    // Read on the geometric mean, because that is what the intended level is:
    // the jitter's median is 1, so `geo_mean(realized) ≈ intended`, while the
    // arithmetic mean carries a fixed `exp(σ²/2)` bias that would blur the very
    // rate this asserts.
    let (mut policy, curve) = build_for(PolicyKind::MachineExplore);
    let lever = LeverSpec::default();
    let trailing = flat_n(1_500.0, PANEL);

    let opening = curve.anchor_spend;
    let mut spend = flat_n(opening, PANEL);
    let periods = 6;
    for _ in 0..periods {
        spend = policy.decide(&observation(&spend, &trailing, Some(clearly_profitable())));
    }

    let expected = opening * (1.0 + lever.max_move_per_period).powi(periods);
    assert!(
        expected < lever.max_multiple * curve.anchor_spend,
        "the fixture must stay clear of the ceiling or it asserts the clamp, not the climb"
    );
    let realized = geo_mean(&spend);
    // 10%: the geometric mean of `n` unit-median lognormals has log-space se
    // `σ/sqrt(n)` ≈ 3%, and a handful of entities may clip against the ceiling.
    assert!(
        (realized / expected - 1.0).abs() < 0.10,
        "machine+explore reached a level of {realized:.2} after {periods} clipped moves from \
         {opening:.2}; the machine alone would be at {expected:.2}. The jitter is either \
         damping the arm or replacing its trajectory."
    );
    assert!(
        log_sd(&spend) > 0.5 * lever.explore_jitter_sd,
        "the arm climbed but stopped spreading, which identifies nothing: {spend:?}"
    );
}

#[test]
fn a_policy_is_reproducible_from_its_seed() {
    // Recorded runs are the evidence. A policy that drew from an unseeded source
    // would make two runs of the same declared world incomparable.
    let run = || {
        let (mut policy, curve) = build_for(PolicyKind::MachineExplore);
        let mut spend = flat(curve.anchor_spend);
        for _ in 0..10 {
            spend = policy.decide(&observation(
                &spend,
                &flat(1_500.0),
                Some(clearly_profitable()),
            ));
        }
        spend
    };
    assert_eq!(run(), run());
}
