use super::*;

fn calibrate(slope: f64, optimum_at: f64) -> CalibrateSpec {
    CalibrateSpec {
        anchor_spend_share: 0.085,
        local_slope_at_anchor: slope,
        optimum_at,
    }
}

#[test]
fn a_policy_is_named_the_way_the_charts_name_it() {
    // The plan, the profit race legend and the outcome map all write this
    // arm `machine+explore`. A spec that spelled it correctly and failed to
    // parse would be a confusing way to learn about serde's rename rules.
    for source in ["machine+explore", "machine_explore"] {
        let parsed: PolicyKind = serde_yaml::from_str(source).unwrap();
        assert_eq!(parsed, PolicyKind::MachineExplore, "failed on {source:?}");
    }
}

#[test]
fn an_absorbing_spend_floor_is_refused() {
    // A zero floor cannot be climbed out of under a multiplicative step, so
    // a run would report a collapse caused by the step rule rather than by
    // anything the estimator did.
    let lever = LeverSpec {
        min_multiple: 0.0,
        ..LeverSpec::default()
    };
    assert!(matches!(
        validate_lever(&lever),
        Err(SimulationError::Spec(_))
    ));
    assert!(validate_lever(&LeverSpec::default()).is_ok());
}

/// Extracted so the two lever rules can be exercised without standing up a
/// whole spec around them.
fn validate_lever(lever: &LeverSpec) -> Result<(), SimulationError> {
    let mut spec = minimal_spec();
    spec.lever = lever.clone();
    spec.validate()
}

fn minimal_spec() -> SimulationSpec {
    SimulationSpec {
        name: "lever".into(),
        description: None,
        seed: 1,
        replicates: 1,
        periods: 4,
        period_days: 7,
        history_days: 90,
        start_date: chrono::NaiveDate::from_ymd_opt(2025, 1, 6).unwrap(),
        entities: EntitiesSpec {
            count: 8,
            scale_sigma: 0.3,
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
fn solved_curve_hits_the_declared_slope_and_optimum() {
    let margin = 0.36;
    let baseline = 1_500.0;
    let curve = calibrate(6.0, 3.0).solve(margin, baseline).unwrap();

    // The two properties the author actually asked for.
    assert!(
        (curve.local_slope(curve.anchor_spend) - 6.0).abs() < 1e-9,
        "slope at the opening spend was {}",
        curve.local_slope(curve.anchor_spend)
    );
    assert!(
        (margin * curve.local_slope(curve.optimum_spend) - 1.0).abs() < 1e-9,
        "marginal profit at the optimum was {}",
        margin * curve.local_slope(curve.optimum_spend) - 1.0
    );
    assert!(
        curve.theta > 0.0 && curve.theta < 1.0,
        "theta {} is not a saturating exponent",
        curve.theta
    );
}

#[test]
fn profit_really_peaks_at_the_solved_optimum() {
    // The algebra above is only worth anything if the curve it produces
    // actually turns over where it claims. Walk the objective and check.
    let margin = 0.36;
    let curve = calibrate(6.0, 3.0).solve(margin, 1_500.0).unwrap();
    let profit = |s: f64| margin * curve.response(s) - s;

    let peak = profit(curve.optimum_spend);
    for step in 1..=40 {
        let s = curve.optimum_spend * (0.2 + 0.05 * step as f64);
        assert!(
            profit(s) <= peak + 1e-6,
            "profit at {s} exceeded the claimed optimum at {}",
            curve.optimum_spend
        );
    }
}

#[test]
fn unreachable_optimum_is_refused_and_names_its_floor() {
    // margin 0.36 × slope 6.0 = 2.16, so no saturating curve puts the optimum
    // at 1.8×. This is exactly the number I guessed wrong when drafting the
    // plan's example YAML, which is why the error states the bound.
    let err = calibrate(6.0, 1.8).solve(0.36, 1_500.0).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("unreachable"), "unexpected error: {msg}");
    assert!(msg.contains("2.16"), "error did not name the floor: {msg}");
}

#[test]
fn spend_that_already_loses_money_is_refused() {
    // margin 0.1 × slope 6.0 = 0.6 < 1.
    let err = calibrate(6.0, 3.0).solve(0.1, 1_500.0).unwrap_err();
    assert!(
        err.to_string().contains("already loses money"),
        "unexpected error: {err}"
    );
}

#[test]
fn response_is_concave_so_the_next_unit_buys_less() {
    let curve = calibrate(6.0, 3.0).solve(0.36, 1_500.0).unwrap();
    let mut previous = f64::INFINITY;
    for step in 1..=20 {
        let slope = curve.local_slope(curve.anchor_spend * step as f64 * 0.5);
        assert!(
            slope < previous,
            "response is not saturating at step {step}"
        );
        previous = slope;
    }
}

#[test]
fn zero_spend_produces_no_response() {
    let curve = calibrate(6.0, 3.0).solve(0.36, 1_500.0).unwrap();
    assert_eq!(curve.response(0.0), 0.0);
    assert_eq!(curve.response(-5.0), 0.0);
}

#[test]
fn a_world_that_still_declares_a_policy_is_refused() {
    // The arms of a profit race are runs of one world. A file that carries
    // `policy:` is a converted-but-not-really grid, and serde ignoring the field
    // would be the worst outcome: `policy: hold` running the machine arm, and
    // nothing saying so.
    let source = format!("{}\npolicy: hold\n", minimal_yaml());
    let err = SimulationSpec::from_yaml(&source).expect_err("a stale policy must not be ignored");
    assert!(
        err.to_string().contains("policy"),
        "the error does not name the offending field: {err}"
    );
}

#[test]
fn a_mistyped_field_is_refused_rather_than_defaulted() {
    // The same rule doing the work it exists for: a defaulted `noise_ratio`
    // still produces a well-formed world, so the run would report a confident
    // estimate of a world nobody declared.
    let source = minimal_yaml().replace("noise_ratio:", "noize_ratio:");
    assert!(SimulationSpec::from_yaml(&source).is_err());
}

#[test]
fn the_declared_lag_defaults_to_the_true_one() {
    // Most worlds are ones where the customer guessed right, and they should not
    // have to say so twice.
    let spec = SimulationSpec::from_yaml(&minimal_yaml()).unwrap();
    assert_eq!(spec.mechanism.declared_lag(), spec.mechanism.lag_days);
    assert_eq!(spec.mechanism.declared_lag_days, None);
}

#[test]
fn a_lag_error_world_keeps_the_two_lags_apart() {
    // The whole axis: the world generates at 7, the view claims 3, and nothing
    // in the fit's output says which was right.
    let source = minimal_yaml().replace("lag_days: 7", "lag_days: 7\n  declared_lag_days: 3");
    let spec = SimulationSpec::from_yaml(&source).unwrap();
    assert_eq!(spec.mechanism.lag_days, 7);
    assert_eq!(spec.mechanism.declared_lag(), 3);
}

#[test]
fn the_history_floor_is_measured_against_the_declared_lag() {
    // `n` is whatever the fitter can pair, and the fitter pairs on what the view
    // claims. Checking the true lag would clear a world the fit then refuses for
    // lack of history — a refusal that reads as "unidentified".
    let source = minimal_yaml()
        .replace("history_days: 90", "history_days: 32")
        .replace("count: 2", "count: 1")
        .replace("lag_days: 7", "lag_days: 1\n  declared_lag_days: 30");
    let err = SimulationSpec::from_yaml(&source).expect_err("should be under the fit floor");
    assert!(
        err.to_string().contains("declared lag 30"),
        "the error does not name the lag it measured against: {err}"
    );
}

#[test]
fn a_period_shorter_than_the_lag_still_declares() {
    // The inverse of a rule this file deliberately does NOT carry. A
    // `period_days < lag_days` world breaks `check::sales_at_flat_spend`'s
    // two-step measurement window and nothing else, so `check` refuses it (see
    // `check.rs`'s non-finite `d1` guard) — `validate` must not, because it
    // gates every `from_yaml`/`from_value` and the runner handles the case
    // correctly: `generate_day` pops one matured entry per day, so the lag
    // rides the queue's depth rather than the period's length.
    //
    // Pinned as a test because the refusal was briefly added here, and a
    // daily-decision world against a weekly lag is precisely the regime this
    // crate exists to measure.
    let source = minimal_yaml().replace("period_days: 7", "period_days: 1");
    let spec = SimulationSpec::from_yaml(&source)
        .expect("a period shorter than the lag is a declarable world");
    assert_eq!(spec.period_days, 1);
    assert!(spec.period_days < spec.mechanism.lag_days);
}

#[test]
fn a_world_nobody_runs_is_refused() {
    let source = minimal_yaml().replace("seed: 1", "seed: 1\nreplicates: 0");
    let err = SimulationSpec::from_yaml(&source).expect_err("zero replicates must not parse");
    assert!(err.to_string().contains("replicates"), "unexpected: {err}");
}

#[test]
fn budget_jitter_defaults_to_what_the_grid_was_measured_at() {
    // The identification knob was a constant in `world.rs` before it was a
    // field. A world that does not mention it must still be the world the
    // recorded findings came from.
    let spec = SimulationSpec::from_yaml(&minimal_yaml()).unwrap();
    assert_eq!(spec.baseline.budget_jitter_sd, DEFAULT_BUDGET_JITTER_SD);
}

#[test]
fn a_qualified_driver_or_target_is_refused() {
    // `mechanism.driver`/`target` become a measure `name:` in the one view
    // this world generates, then get re-qualified as `{view}.{name}` to build
    // the driver edge. A `view.member` value here survives both steps as
    // plain string concatenation and only fails three views down inside
    // airlayer's member-path parser, as an unreadable `store_days.view.member`
    // with nothing pointing back at the world file.
    let driver = minimal_yaml().replace(
        "driver: marketing_spend",
        "driver: quickbooks_pl.total_cogs",
    );
    let err = SimulationSpec::from_yaml(&driver).expect_err("a dotted driver must not parse");
    assert!(
        err.to_string().contains("mechanism.driver"),
        "the error does not name the offending field: {err}"
    );

    let target = minimal_yaml().replace("target: net_sales", "target: quickbooks_pl.store_sales");
    let err = SimulationSpec::from_yaml(&target).expect_err("a dotted target must not parse");
    assert!(
        err.to_string().contains("mechanism.target"),
        "the error does not name the offending field: {err}"
    );
}

#[test]
fn a_driver_or_target_outside_the_identifier_class_is_refused() {
    // The dot is not the only character that survives into the generated
    // world unescaped. The same string is interpolated raw into a CSV header
    // (`world_dir::csv_header`) and into a YAML document as a measure
    // `name:`, an `expr:` and a `drivers.measure` path (`world_dir::view_yml`):
    // a comma splits the header, a colon or quote or leading `#` breaks the
    // YAML, a space or leading digit is not a column any SQL engine will
    // resolve bare. All of them have to stop here, naming the rule and the
    // value, rather than three layers down as a parse error in a file the
    // author never wrote.
    for (label, bad) in [
        ("comma", "net,sales"),
        ("colon", "net:sales"),
        ("space", "net sales"),
        ("leading digit", "7d_sales"),
        ("quote", "net\"sales"),
        ("leading hash", "#sales"),
        ("hyphen", "net-sales"),
        ("empty", ""),
    ] {
        let mut spec = minimal_spec();
        spec.mechanism.driver = bad.to_string();
        let err = spec
            .validate()
            .expect_err(&format!("driver with a {label} must be refused"));
        let msg = err.to_string();
        assert!(
            msg.contains("mechanism.driver"),
            "{label}: the error does not name the offending field: {msg}"
        );
        assert!(
            msg.contains(&format!("'{bad}'")),
            "{label}: the error does not quote the offending value: {msg}"
        );
        assert!(
            msg.contains("letter or underscore"),
            "{label}: the error does not state the rule: {msg}"
        );

        let mut spec = minimal_spec();
        spec.mechanism.target = bad.to_string();
        let err = spec
            .validate()
            .expect_err(&format!("target with a {label} must be refused"));
        assert!(
            err.to_string().contains("mechanism.target"),
            "{label}: the error does not name the offending field: {err}"
        );
    }
}

#[test]
fn a_driver_or_target_with_digits_and_underscores_after_the_first_char_is_accepted() {
    // The identifier class is not "letters only": a rolling-window measure is
    // exactly what a world would want to call its target.
    let mut spec = minimal_spec();
    spec.mechanism.target = "net_sales_7d".into();
    spec.mechanism.driver = "_spend2".into();
    spec.validate()
        .expect("an identifier with digits and underscores after the first char must pass");
}

#[test]
fn is_bare_identifier_matches_the_documented_class() {
    for ok in ["a", "_", "net_sales_7d", "A9", "__x__"] {
        assert!(is_bare_identifier(ok), "{ok:?} should be an identifier");
    }
    for bad in [
        "", "7d", "a.b", "a,b", "a:b", "a b", "a-b", "#a", "a\n", "é", "a\"b",
    ] {
        assert!(
            !is_bare_identifier(bad),
            "{bad:?} should not be an identifier"
        );
    }
}

#[test]
fn a_bare_driver_or_target_other_than_the_default_pair_is_accepted() {
    // The mechanism itself (a driver lifts a target after a lag, cost is a
    // margin share of the target) is generic — nothing about it is specific
    // to marketing or sales. A world is free to call it something else.
    let source = minimal_yaml()
        .replace("driver: marketing_spend", "driver: ad_spend")
        .replace("target: net_sales", "target: signups");
    SimulationSpec::from_yaml(&source).expect("a renamed but otherwise valid pair must parse");
}

#[test]
fn a_driver_or_target_reserved_by_every_world_is_refused() {
    // `entity_id`, `date` and `prime_cost` are always declared by the
    // generated view (`world_dir::view_yml`) — a world naming its own driver
    // or target one of these would collide with a column that already means
    // something else.
    for reserved in ["entity_id", "date", "prime_cost"] {
        let source =
            minimal_yaml().replace("driver: marketing_spend", &format!("driver: {reserved}"));
        let err = SimulationSpec::from_yaml(&source)
            .expect_err(&format!("driver '{reserved}' must be refused"));
        assert!(
            err.to_string().contains("mechanism.driver"),
            "the error does not name the offending field: {err}"
        );
    }
}

#[test]
fn a_driver_equal_to_its_own_target_is_refused() {
    let source = minimal_yaml().replace("driver: marketing_spend", "driver: net_sales");
    let err = SimulationSpec::from_yaml(&source).expect_err("driver == target must be refused");
    assert!(
        err.to_string().contains("must be different"),
        "unexpected: {err}"
    );
}

#[test]
fn a_zero_margin_is_refused_by_the_spec_rule_not_by_the_curve() {
    // `(0.0..1.0).contains(&0.0)` is true, so a zero margin used to slip the
    // range check whose message says `(0, 1)`, reach `CalibrateSpec::solve`,
    // and come back as "Raise local_slope_at_anchor above inf" — a
    // division by the margin, reported as advice about a different field.
    for margin in [0.0, -0.1, f64::NAN] {
        let mut spec = minimal_spec();
        spec.baseline.margin = margin;
        let err = spec
            .validate()
            .expect_err(&format!("margin {margin} must be refused"));
        let msg = err.to_string();
        assert!(
            msg.contains("baseline.margin must be in (0, 1)"),
            "margin {margin}: expected the spec rule, got: {msg}"
        );
        assert!(
            !msg.contains("inf"),
            "margin {margin}: the curve solver ran and rendered inf: {msg}"
        );
    }
}

#[test]
fn a_negative_budget_jitter_is_refused_but_zero_is_not() {
    // Zero is the flat-lever corner and has to stay legal: it is where the only
    // thing moving spend is the confounder, and the honest answer is a refusal.
    let flat = minimal_yaml().replace(
        "weekly_seasonality: 0.15",
        "weekly_seasonality: 0.15\n  budget_jitter_sd: 0.0",
    );
    assert!(SimulationSpec::from_yaml(&flat).is_ok());

    let negative = minimal_yaml().replace(
        "weekly_seasonality: 0.15",
        "weekly_seasonality: 0.15\n  budget_jitter_sd: -0.1",
    );
    assert!(SimulationSpec::from_yaml(&negative).is_err());
}

#[test]
fn from_value_runs_the_same_checks_as_from_yaml() {
    // A form posting JSON has no YAML source to hand `from_yaml`, but it must
    // not get a laxer path: the same unreachable-optimum, same missing-field
    // rules have to apply before anything is ever written to a file.
    let value: serde_json::Value = serde_yaml::from_str(&minimal_yaml()).unwrap();
    let spec = SimulationSpec::from_value(value).expect("a valid world must parse from JSON too");
    assert_eq!(spec.name, "minimal");

    let mut bad: serde_json::Value = serde_yaml::from_str(&minimal_yaml()).unwrap();
    bad["mechanism"]["driver"] = serde_json::json!("net_sales");
    let err =
        SimulationSpec::from_value(bad).expect_err("driver == target must be refused here too");
    assert!(
        err.to_string().contains("must be different"),
        "unexpected: {err}"
    );
}

#[test]
fn a_policy_round_trips_through_its_wire_spelling() {
    // `format!("{:?}")` renders this arm `machineexplore`, which parses back as
    // nothing at all — and it is what a run row used to store.
    for arm in PolicyKind::ALL {
        assert_eq!(arm.as_str().parse::<PolicyKind>().unwrap(), arm);
    }
    assert_eq!(PolicyKind::MachineExplore.as_str(), "machine_explore");
    assert_eq!(
        "machine+explore".parse::<PolicyKind>().unwrap(),
        PolicyKind::MachineExplore,
        "the spelling the plan and the charts use no longer parses"
    );
    assert!("nonsense".parse::<PolicyKind>().is_err());
}

/// A minimal world as YAML, so the parse-level rules can be exercised on the
/// text a person actually writes rather than on a struct literal that cannot
/// carry an unknown field at all.
fn minimal_yaml() -> String {
    "\
name: minimal
seed: 1
periods: 4
period_days: 7
history_days: 90
start_date: 2025-01-06
entities:
  count: 2
  scale_sigma: 0.4
baseline:
  sales_per_entity_day: 1500.0
  margin: 0.36
  demand_shock_rho: 0.7
  demand_shock_sd: 0.12
  weekly_seasonality: 0.15
mechanism:
  driver: marketing_spend
  target: net_sales
  lag_days: 7
  noise_ratio: 0.05
  calibrate:
    anchor_spend_share: 0.02
    local_slope_at_anchor: 4.0
    optimum_at: 3.0
"
    .to_string()
}
