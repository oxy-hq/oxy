//! The race statistic, against numbers computed outside this crate.
//!
//! The expected values in `a_worked_race_matches_an_independent_computation`
//! and its two siblings were produced by a separate Python implementation that
//! integrates the Student-t density directly (Romberg, to ~1e-15) rather than
//! going through a regularized incomplete beta — which is the path `statrs`
//! takes. Agreement is therefore evidence, not two copies of one bug. Its
//! critical values were checked against the published table: t(0.975, 4) =
//! 2.776445 and t(0.975, 1) = 12.7062.

use super::*;

/// The worked example: five shared world draws, machine against legacy.
fn worked() -> (ArmProfits, ArmProfits) {
    let machine = ArmProfits::collect(
        PolicyKind::Machine,
        [
            (0, 1200.0),
            (1, 1450.0),
            (2, 980.0),
            (3, 1610.0),
            (4, 1330.0),
        ],
    );
    let legacy = ArmProfits::collect(
        PolicyKind::Legacy,
        [
            (0, 1100.0),
            (1, 1400.0),
            (2, 900.0),
            (3, 1580.0),
            (4, 1240.0),
        ],
    );
    (machine, legacy)
}

fn close(actual: f64, expected: f64) {
    let tol = 1e-10 * expected.abs().max(1.0);
    assert!(
        (actual - expected).abs() <= tol,
        "expected {expected:.17e}, got {actual:.17e} (tolerance {tol:.3e})"
    );
}

fn tested(comparison: &PairedComparison) -> &PairedTest {
    match &comparison.inference {
        Inference::Tested(test) => test,
        other => panic!("expected an inference, got {other:?}"),
    }
}

#[test]
fn a_worked_race_matches_an_independent_computation() {
    let (machine, legacy) = worked();
    let result = compare(&machine, &legacy);

    assert_eq!(result.n_pairs, 5);
    assert_eq!(result.dropped_unpaired, 0);
    assert_eq!(result.dropped_nonfinite, 0);
    close(result.mean_difference.expect("five pairs"), 70.0);

    let test = tested(&result);
    assert_eq!(test.dof, 4);
    close(test.std_error, 13.038404810405297);
    close(test.t, 5.368754921931593);
    close(test.p_value, 0.005812156818410408);
    close(test.interval.0, 33.7995847845632);
    close(test.interval.1, 106.2004152154368);
    close(test.confidence, 0.95);
}

#[test]
fn the_per_arm_aggregation_reports_n_mean_and_sd() {
    let (machine, legacy) = worked();
    let result = compare(&machine, &legacy);

    assert_eq!(result.treatment.arm, PolicyKind::Machine);
    assert_eq!(result.treatment.n, 5);
    close(result.treatment.mean.expect("n = 5"), 1314.0);
    close(result.treatment.sd.expect("n = 5"), 240.27068069159);

    assert_eq!(result.baseline.arm, PolicyKind::Legacy);
    assert_eq!(result.baseline.n, 5);
    close(result.baseline.mean.expect("n = 5"), 1244.0);
    close(result.baseline.sd.expect("n = 5"), 262.83074401599214);
}

#[test]
fn two_pairs_is_one_degree_of_freedom_and_the_fattest_tail() {
    // dof = 1, where the t distribution is furthest from a normal: t = 1.67
    // buys p = 0.34, and the interval is eleven times the estimate wide. A
    // reader who saw a z-test's 0.095 here would call this a finding.
    let a = ArmProfits::collect(PolicyKind::Machine, [(0, 10.0), (1, 30.0)]);
    let b = ArmProfits::collect(PolicyKind::Legacy, [(0, 4.0), (1, 6.0)]);
    let result = compare(&a, &b);

    assert_eq!(result.n_pairs, 2);
    close(result.mean_difference.expect("two pairs"), 15.0);
    let test = tested(&result);
    assert_eq!(test.dof, 1);
    close(test.std_error, 9.0);
    close(test.t, 1.6666666666666667);
    close(test.p_value, 0.34404173924526127);
    close(test.interval.0, -99.35584262557043);
    close(test.interval.1, 129.35584262557043);
}

#[test]
fn pairing_is_what_makes_the_effect_visible() {
    // Eight worlds whose own swing is ±1500 against an arm effect of ~21. The
    // paired test sees the effect at p = 4e-7; an unpaired test over the same
    // sixteen numbers reports p = 0.96 — nothing at all — because the world
    // draw's variance, which pairing cancels, is seventy times the effect.
    // This is the whole reason `replicate_seed` takes no policy argument.
    let world = [0.0, 500.0, -400.0, 900.0, -700.0, 200.0, 1500.0, -1100.0];
    let effect = [22.0, 18.0, 25.0, 19.0, 21.0, 24.0, 17.0, 26.0];
    let treatment = ArmProfits::collect(
        PolicyKind::Machine,
        world
            .iter()
            .zip(effect.iter())
            .enumerate()
            .map(|(k, (w, e))| (k as u32, 1000.0 + w + e)),
    );
    let baseline = ArmProfits::collect(
        PolicyKind::Legacy,
        world
            .iter()
            .enumerate()
            .map(|(k, w)| (k as u32, 1000.0 + w)),
    );

    let result = compare(&treatment, &baseline);
    let test = tested(&result);
    assert_eq!(test.dof, 7);
    close(test.t, 18.217348733330955);
    close(test.p_value, 3.716811769471917e-07);
    assert!(
        test.p_value < 1e-5,
        "paired test should find this; got p = {}",
        test.p_value
    );

    // The unpaired view of the identical sixteen numbers, computed here so the
    // contrast is asserted rather than asserted-about: Welch on these arms is
    // p = 0.9605, and both arms' sd is ~850 against a 21-point difference.
    let sd_t = result.treatment.sd.expect("eight draws");
    let sd_b = result.baseline.sd.expect("eight draws");
    let welch_se = (sd_t * sd_t / 8.0 + sd_b * sd_b / 8.0).sqrt();
    let welch_t = result.mean_difference.expect("eight pairs") / welch_se;
    assert!(
        welch_t.abs() < 0.1,
        "an unpaired test should see nothing here; got t = {welch_t}"
    );
}

#[test]
fn one_replicate_reports_the_difference_and_refuses_to_infer() {
    // A single-replicate world is the common case for a first look. There is
    // one number and no spread, so the margin is real and the confidence in it
    // is undefined — a p-value here would be invented.
    let a = ArmProfits::collect(PolicyKind::Machine, [(0, 1200.0)]);
    let b = ArmProfits::collect(PolicyKind::Legacy, [(0, 1100.0)]);
    let result = compare(&a, &b);

    assert_eq!(result.n_pairs, 1);
    close(result.mean_difference.expect("one pair"), 100.0);
    assert_eq!(
        result.inference,
        Inference::Withheld(NoInference::SinglePair)
    );
    assert_eq!(result.treatment.n, 1);
    close(result.treatment.mean.expect("one draw"), 1200.0);
    assert_eq!(result.treatment.sd, None, "sd needs n >= 2");
}

#[test]
fn no_overlapping_replicates_is_no_comparison() {
    let a = ArmProfits::collect(PolicyKind::Machine, []);
    let b = ArmProfits::collect(PolicyKind::Legacy, []);
    let result = compare(&a, &b);

    assert_eq!(result.n_pairs, 0);
    assert_eq!(result.mean_difference, None);
    assert_eq!(result.inference, Inference::Withheld(NoInference::NoPairs));
    assert_eq!(result.treatment.mean, None);
    assert_eq!(result.treatment.sd, None);
}

#[test]
fn identical_arms_are_a_draw_not_a_zero_p_value() {
    let a = ArmProfits::collect(PolicyKind::Machine, [(0, 10.0), (1, 20.0), (2, 30.0)]);
    let b = ArmProfits::collect(PolicyKind::Legacy, [(0, 10.0), (1, 20.0), (2, 30.0)]);
    let result = compare(&a, &b);

    assert_eq!(result.n_pairs, 3);
    assert_eq!(
        result.mean_difference,
        Some(0.0),
        "exactly zero, not a rounding of it"
    );
    assert_eq!(
        result.inference,
        Inference::Withheld(NoInference::IdenticalArms)
    );
}

#[test]
fn a_constant_margin_is_reported_without_an_impossible_p_value() {
    // Every draw favours the treatment by exactly 5. t is infinite and the
    // interval collapses to a point; emitting p = 0 would claim certainty
    // three worlds cannot buy.
    let a = ArmProfits::collect(PolicyKind::Machine, [(0, 15.0), (1, 25.0), (2, 35.0)]);
    let b = ArmProfits::collect(PolicyKind::Legacy, [(0, 10.0), (1, 20.0), (2, 30.0)]);
    let result = compare(&a, &b);

    close(result.mean_difference.expect("three pairs"), 5.0);
    assert_eq!(
        result.inference,
        Inference::Withheld(NoInference::ConstantDifference)
    );
}

#[test]
fn a_missing_replicate_drops_its_pair_rather_than_shifting_the_rest() {
    // Replicate 2 of the baseline failed. The naive fix — zip two vectors —
    // would pair the treatment's world 2 against the baseline's world 3 and
    // every later world one off, which reads as a large effect drawn entirely
    // from world-to-world variance. The keys are the guard.
    let a = ArmProfits::collect(
        PolicyKind::Machine,
        [
            (0, 1200.0),
            (1, 1450.0),
            (2, 980.0),
            (3, 1610.0),
            (4, 1330.0),
        ],
    );
    let b = ArmProfits::collect(
        PolicyKind::Legacy,
        [(0, 1100.0), (1, 1400.0), (3, 1580.0), (4, 1240.0)],
    );
    let result = compare(&a, &b);

    assert_eq!(result.n_pairs, 4);
    assert_eq!(result.dropped_unpaired, 1);
    // (100 + 50 + 30 + 90) / 4
    close(result.mean_difference.expect("four pairs"), 67.5);
    // The arm summaries cover the paired subset only, so the two are comparable.
    assert_eq!(result.treatment.n, 4);
    close(
        result.treatment.mean.expect("four pairs"),
        (1200.0 + 1450.0 + 1610.0 + 1330.0) / 4.0,
    );
    assert_eq!(tested(&result).dof, 3);
}

#[test]
fn a_non_finite_profit_drops_its_pair_and_is_counted() {
    let a = ArmProfits::collect(
        PolicyKind::Machine,
        [
            (0, 1200.0),
            (1, f64::NAN),
            (2, 980.0),
            (3, f64::INFINITY),
            (4, 1330.0),
        ],
    );
    let b = ArmProfits::collect(
        PolicyKind::Legacy,
        [
            (0, 1100.0),
            (1, 1400.0),
            (2, 900.0),
            (3, 1580.0),
            (4, 1240.0),
        ],
    );
    let result = compare(&a, &b);

    assert_eq!(result.n_pairs, 3);
    assert_eq!(result.dropped_nonfinite, 2);
    assert_eq!(result.dropped_unpaired, 0);
    close(result.mean_difference.expect("three pairs"), 90.0);
    let test = tested(&result);
    assert!(test.t.is_finite() && test.p_value.is_finite());
    assert!((0.0..=1.0).contains(&test.p_value), "p = {}", test.p_value);
}

#[test]
fn every_non_finite_pair_leaves_nothing_to_compare() {
    let a = ArmProfits::collect(PolicyKind::Machine, [(0, f64::NAN), (1, f64::NAN)]);
    let b = ArmProfits::collect(PolicyKind::Legacy, [(0, 1.0), (1, 2.0)]);
    let result = compare(&a, &b);

    assert_eq!(result.n_pairs, 0);
    assert_eq!(result.dropped_nonfinite, 2);
    assert_eq!(result.mean_difference, None);
    assert_eq!(result.inference, Inference::Withheld(NoInference::NoPairs));
}

#[test]
fn the_direction_of_the_difference_follows_the_argument_order() {
    let (machine, legacy) = worked();
    let forward = compare(&machine, &legacy);
    let backward = compare(&legacy, &machine);

    close(
        backward.mean_difference.expect("five pairs"),
        -forward.mean_difference.expect("five pairs"),
    );
    close(tested(&backward).t, -tested(&forward).t);
    // A two-sided p-value is blind to the direction.
    close(tested(&backward).p_value, tested(&forward).p_value);
}

#[test]
fn a_race_runs_every_challenger_against_the_one_baseline() {
    // Per-comparison p-values: see the module docs on multiplicity.
    let (machine, legacy) = worked();
    let oracle = ArmProfits::collect(
        PolicyKind::Oracle,
        [
            (0, 1300.0),
            (1, 1550.0),
            (2, 1080.0),
            (3, 1710.0),
            (4, 1430.0),
        ],
    );
    let results = profit_race(&legacy, [&machine, &oracle]);

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].treatment.arm, PolicyKind::Machine);
    assert_eq!(results[1].treatment.arm, PolicyKind::Oracle);
    assert_eq!(results[0].baseline.arm, PolicyKind::Legacy);
    close(results[1].mean_difference.expect("five pairs"), 170.0);
}

#[test]
fn a_confidence_level_outside_zero_to_one_falls_back_to_the_default() {
    let (machine, legacy) = worked();
    let bogus = compare_at_confidence(&machine, &legacy, 95.0);
    let default = compare(&machine, &legacy);
    assert_eq!(bogus.inference, default.inference);
}

#[test]
fn a_wider_interval_needs_a_higher_confidence_level() {
    let (machine, legacy) = worked();
    let ninety = tested(&compare_at_confidence(&machine, &legacy, 0.90)).interval;
    let ninety_nine = tested(&compare_at_confidence(&machine, &legacy, 0.99)).interval;
    assert!(ninety_nine.0 < ninety.0 && ninety.1 < ninety_nine.1);
}
