use super::*;
use crate::spec::{
    BaselineSpec, CalibrateSpec, DEFAULT_BUDGET_JITTER_SD, EntitiesSpec, LeverSpec, MechanismSpec,
};

fn spec() -> SimulationSpec {
    SimulationSpec {
        name: "test".into(),
        description: None,
        seed: 7,
        replicates: 1,
        periods: 10,
        period_days: 7,
        history_days: 120,
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
fn the_same_seed_reproduces_the_same_world() {
    let a = World::new(spec()).unwrap().warm_up();
    let b = World::new(spec()).unwrap().warm_up();
    assert_eq!(a, b, "a seed must mean one world");
}

#[test]
fn a_different_seed_is_a_different_world() {
    let mut other = spec();
    other.seed = 8;
    let a = World::new(spec()).unwrap().warm_up();
    let b = World::new(other).unwrap().warm_up();
    assert_ne!(a, b);
}

#[test]
fn warm_up_emits_one_row_per_entity_day() {
    let s = spec();
    let rows = World::new(s.clone()).unwrap().warm_up();
    assert_eq!(rows.len(), (s.history_days * s.entities.count) as usize);
}

#[test]
fn spending_more_produces_more_sales_after_the_lag() {
    // The intervention test, and the property a fixture cannot have: change
    // the input and different output follows.
    let s = spec();
    let lift = |multiple: f64| {
        let mut world = World::new(s.clone()).unwrap();
        world.warm_up();
        let spend = vec![world.anchor_spend() * multiple; world.entity_count()];
        // Two periods: the first is still paying out the lagged burn-in
        // spend, so the effect only fully lands in the second.
        world.step(&spend);
        let rows = world.step(&spend);
        rows.iter().map(|r| r.net_sales).sum::<f64>()
    };

    let base = lift(1.0);
    let doubled = lift(2.0);
    assert!(
        doubled > base,
        "doubling spend did not raise sales: {base} → {doubled}"
    );
}

#[test]
fn a_period_shorter_than_the_lag_still_lags_by_lag_days_not_period_days() {
    // Pins the runner claim `spec/tests.rs::a_period_shorter_than_the_lag_still_declares`
    // only asserts as far as "parses": `generate_day` pops one matured spend
    // off a queue `lag_days` deep, so a spend change lands `lag_days` days
    // later regardless of how short a period is — never after `period_days`,
    // here 7x shorter than the lag.
    let mut s = spec();
    s.period_days = 1;
    s.mechanism.lag_days = 7;
    let lag = s.mechanism.lag_days as usize;

    let reference = 200.0;
    let treatment = reference * 3.0;
    let ref_spend = vec![reference; s.entities.count as usize];
    let treat_spend = vec![treatment; s.entities.count as usize];

    let mut control = World::new(s.clone()).unwrap();
    control.warm_up();
    let mut treated = World::new(s.clone()).unwrap();
    treated.warm_up();

    // Flush both worlds' pending-spend queues to an identical, known state
    // before diverging, so any later difference is attributable to the spend
    // change alone rather than to whatever the burn-in happened to leave
    // queued.
    for _ in 0..lag {
        control.step(&ref_spend);
        treated.step(&ref_spend);
    }

    // From here, `control` keeps paying the reference spend and `treated`
    // switches to `treatment`, one 1-day period at a time. Same seed and the
    // same number of days executed on both sides keeps every RNG draw in
    // lockstep, so a difference in net sales can only come from the matured
    // spend the response curve sees.
    for day in 1..=(lag + 1) {
        let control_rows = control.step(&ref_spend);
        let treated_rows = treated.step(&treat_spend);
        let control_sales: Vec<f64> = control_rows.iter().map(|r| r.net_sales).collect();
        let treated_sales: Vec<f64> = treated_rows.iter().map(|r| r.net_sales).collect();
        if day <= lag {
            assert_eq!(
                control_sales, treated_sales,
                "day {day}: a 1-day period already elapsed since the spend \
                 changed, but the {lag}-day lag has not — sales should not \
                 have moved yet"
            );
        } else {
            assert_ne!(
                control_sales, treated_sales,
                "day {day}: the spend change is {lag} days old now and should \
                 have matured"
            );
        }
    }
}

#[test]
fn the_response_saturates_in_the_generated_rows() {
    // Not just in the curve — in what the world actually emits, since that is
    // all the fitter ever sees. Without this, nothing in a run distinguishes a
    // saturating truth from a linear one, and the whole declared-`form:`
    // experiment has no world to run in.
    //
    // The increments must be EQUAL. Comparing s→2s against 2s→4s compares a
    // step of `s` against a step of `2s`, and a concave curve can absolutely
    // return more over the wider one — diminishing returns are per unit of
    // spend, not per doubling.
    let s = spec();
    let sales_at = |multiple: f64| {
        let mut world = World::new(s.clone()).unwrap();
        world.warm_up();
        let spend = vec![world.anchor_spend() * multiple; world.entity_count()];
        // Two periods: the first is still paying out lagged burn-in spend, so
        // the new level only fully lands in the second.
        world.step(&spend);
        let rows = world.step(&spend);
        rows.iter().map(|r| r.net_sales).sum::<f64>()
    };

    let first_gain = sales_at(2.0) - sales_at(1.0);
    let second_gain = sales_at(3.0) - sales_at(2.0);
    assert!(
        second_gain < first_gain,
        "the second unit of spend bought more than the first ({second_gain} vs {first_gain}) \
             — the emitted world is not saturating"
    );
}

#[test]
fn legacy_spend_tracks_trailing_sales() {
    // The confounding mechanism, asserted directly: a bigger entity sells
    // more and therefore budgets more, which is exactly the contrast an
    // un-demeaned regression would mistake for a marketing effect.
    let mut world = World::new(spec()).unwrap();
    world.warm_up();
    let spend = world.legacy_spend();

    let mut sales_by_entity = vec![0.0; world.entity_count()];
    for row in world.step(&spend) {
        sales_by_entity[row.entity_id as usize] += row.net_sales;
    }
    let biggest = argmax(&sales_by_entity);
    let smallest = argmin(&sales_by_entity);
    assert!(
        spend[biggest] > spend[smallest],
        "legacy spend did not track sales across entities"
    );
}

#[test]
fn profit_is_margin_times_sales_less_spend() {
    let mut world = World::new(spec()).unwrap();
    let rows = world.warm_up();
    let row = &rows[0];
    let expected = 0.36 * row.net_sales - row.marketing_spend;
    assert!((row.profit() - expected).abs() < 1e-9);
}

#[test]
fn sales_never_go_negative() {
    let mut harsh = spec();
    harsh.baseline.demand_shock_sd = 0.9;
    harsh.mechanism.noise_ratio = 2.0;
    let rows = World::new(harsh).unwrap().warm_up();
    assert!(rows.iter().all(|r| r.net_sales >= 0.0));
}

fn argmax(values: &[f64]) -> usize {
    values
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .map(|(i, _)| i)
        .unwrap()
}

fn argmin(values: &[f64]) -> usize {
    values
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .map(|(i, _)| i)
        .unwrap()
}

#[test]
fn the_budget_jitter_knob_moves_how_much_the_lever_varies() {
    // The identification axis. Turning it down has to actually shrink the
    // within-panel spread of the regressor, or the left-hand column of the
    // outcome map — where the gate is the only thing saving you — cannot be
    // reached from a declared world.
    // WITHIN panel, not pooled. Pooled spread is dominated by entity size
    // (`scale_sigma`), which this knob does not touch and the fit demeans away
    // — measuring it would report a knob that barely moves while the quantity
    // the fitter actually sees collapsed.
    fn within_panel_spend_cv(tune: impl Fn(&mut SimulationSpec)) -> f64 {
        let mut s = spec();
        tune(&mut s);
        let rows = World::new(s).unwrap().warm_up();

        let mut by_entity: std::collections::BTreeMap<u32, Vec<f64>> = Default::default();
        for row in &rows {
            by_entity
                .entry(row.entity_id)
                .or_default()
                .push(row.marketing_spend);
        }
        let cvs: Vec<f64> = by_entity
            .values()
            .map(|spend| {
                let mean = spend.iter().sum::<f64>() / spend.len() as f64;
                let var =
                    spend.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / spend.len() as f64;
                var.sqrt() / mean
            })
            .collect();
        cvs.iter().sum::<f64>() / cvs.len() as f64
    }

    let wide = within_panel_spend_cv(|s| s.baseline.budget_jitter_sd = 0.12);
    let flat = within_panel_spend_cv(|s| s.baseline.budget_jitter_sd = 0.005);
    assert!(
        flat < wide,
        "budget_jitter_sd did not move the lever: {flat:.4} against {wide:.4}"
    );

    // Where the rest of it comes from, measured rather than assumed — because a
    // "flat lever" world that still moves 11% would otherwise look like the knob
    // not working. Under a budget set from trailing sales, EVERYTHING that moves
    // sales moves spend: strip the shock, the weekly cycle and the target noise
    // in turn and the lever finally goes still. That is the honest content of
    // this axis — a legacy budget rule cannot produce a truly flat lever, so the
    // left column of the outcome map is approached, never reached.
    let flat_no_shock = within_panel_spend_cv(|s| {
        s.baseline.budget_jitter_sd = 0.005;
        s.baseline.demand_shock_sd = 0.0;
    });
    let flat_nothing_moving = within_panel_spend_cv(|s| {
        s.baseline.budget_jitter_sd = 0.005;
        s.baseline.demand_shock_sd = 0.0;
        s.baseline.weekly_seasonality = 0.0;
        s.mechanism.noise_ratio = 0.0;
    });
    assert!(
        flat_nothing_moving < flat_no_shock && flat_no_shock < flat,
        "the lever's residual movement is not what it is claimed to be: \
         {flat:.4} (shock) > {flat_no_shock:.4} (seasonality + noise) > \
         {flat_nothing_moving:.4} (jitter alone)"
    );
    // And the floor the axis cannot go below, which is the thing worth knowing
    // about it: with the jitter at EXACTLY zero and nothing random left in the
    // world, the within-panel CV is still ~4%. That is the burn-in climbing
    // from its anchor to the budget rule's fixed point — spend raises sales,
    // which raises the budget — a deterministic ramp inside every panel.
    //
    // So `budget_jitter_sd: 0` does not buy a still lever, and the variation it
    // leaves is the worst kind: perfectly correlated with the mechanism it is
    // being used to measure. The flat-lever corner is approached, never reached.
    let no_jitter_at_all = within_panel_spend_cv(|s| {
        s.baseline.budget_jitter_sd = 0.0;
        s.baseline.demand_shock_sd = 0.0;
        s.baseline.weekly_seasonality = 0.0;
        s.mechanism.noise_ratio = 0.0;
    });
    assert!(
        no_jitter_at_all > 0.02,
        "the transient toward the budget fixed point has gone: {no_jitter_at_all:.4}. \
         Either the burn-in now starts at the fixed point, or the budget rule \
         stopped feeding back — both change what every declared world means"
    );
}
