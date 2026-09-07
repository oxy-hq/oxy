//! Every world in `simulations/` must be coherent.
//!
//! A declared world that does not parse, or whose emitted rows no longer carry
//! the mechanism it declares, invalidates every convergence claim downstream —
//! and does it **silently**, because a drifted world still produces a
//! confident-looking run. The grid is evidence, so it gets the same treatment
//! as code.

use std::path::{Path, PathBuf};

use oxy_simulation::{SimulationSpec, check};

/// Every directory that holds declared worlds.
///
/// Two of them, and both are checked for the same reason: `simulations/` is the
/// grid the outcome map is built from, and `example_new/simulations/` is what
/// somebody meets first. A demo world that has silently drifted from its own
/// spec is worse than one that is missing — it still produces a confident run.
fn worlds_dirs() -> Vec<PathBuf> {
    // `crates/simulation` → repo root.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    ["simulations", "example_new/simulations"]
        .iter()
        .map(|d| {
            root.join(d)
                .canonicalize()
                .unwrap_or_else(|e| panic!("declared-worlds directory {d} is missing: {e}"))
        })
        .collect()
}

fn declared() -> Vec<(String, SimulationSpec)> {
    let mut out = Vec::new();
    for dir in worlds_dirs() {
        for entry in std::fs::read_dir(&dir).expect("read a declared-worlds directory") {
            let path = entry.expect("dir entry").path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if !name.ends_with(".simulation.yml") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("read world");
            let spec = SimulationSpec::from_yaml(&source)
                .unwrap_or_else(|e| panic!("{name} does not parse or is incoherent: {e}"));
            out.push((name, spec));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(
        !out.is_empty(),
        "no declared worlds found — the grid is empty"
    );
    out
}

#[test]
fn every_declared_world_parses_and_is_coherent() {
    // `from_yaml` validates, so this also catches an unreachable optimum and a
    // history too short to fit on — both of which produce a run that measures
    // nothing rather than an error.
    let worlds = declared();
    for (name, spec) in &worlds {
        assert_eq!(
            spec.name,
            name.trim_end_matches(".simulation.yml"),
            "{name} declares a different `name:` than its filename — a run would \
             be recorded under a name nobody can find the file for"
        );
    }
    assert!(
        worlds.len() >= 10,
        "the grid shrank: {} worlds",
        worlds.len()
    );
}

#[test]
fn no_declared_world_carries_a_policy() {
    // The arms of a race are runs of ONE world, not separate files. A leftover
    // `policy:` line is rejected by `deny_unknown_fields` — this asserts the
    // grid was actually converted rather than relying on that error surfacing
    // somewhere a person would see it.
    for dir in worlds_dirs() {
        for entry in std::fs::read_dir(&dir).expect("read a declared-worlds directory") {
            let path = entry.expect("dir entry").path();
            if !path.to_string_lossy().ends_with(".simulation.yml") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("read world");
            assert!(
                !source
                    .lines()
                    .any(|l| l.trim_start().starts_with("policy:")),
                "{} declares a policy — that is a property of a run, and four \
                 files differing by one line are four worlds that happen to look \
                 alike",
                path.display()
            );
        }
    }
}

#[test]
fn every_declared_world_emits_the_mechanism_it_declares() {
    // The self-check, applied to the grid: re-derive θ and the scale from the
    // rows each world actually generates, by an estimator that shares no code
    // with the world engine, and fail on drift.
    for (name, spec) in declared() {
        let report =
            check(&spec).unwrap_or_else(|e| panic!("{name} has drifted from its spec: {e}"));
        assert!(
            report.n_pairs > 0,
            "{name} produced no paired observations at all"
        );
        assert!(
            report.true_local_slope > 0.0,
            "{name} has a non-positive true slope at its settled spend"
        );
    }
}

#[test]
fn the_grid_actually_varies_what_it_claims_to() {
    // A grid whose points all measure the same thing is one experiment run ten
    // times. Each of these pins an axis the plan says the map turns on.
    let by_name: std::collections::HashMap<_, _> = declared()
        .into_iter()
        .map(|(n, s)| (n.trim_end_matches(".simulation.yml").to_string(), s))
        .collect();

    let confounded = &by_name["confounded"];
    let clean = &by_name["clean"];
    assert!(
        confounded.baseline.demand_shock_rho > clean.baseline.demand_shock_rho,
        "the clean control is not actually less confounded than the confounded world"
    );

    assert!(
        by_name["thin_history"].history_days < confounded.history_days,
        "the thin-history world is not thinner"
    );
    assert!(
        by_name["few_panels"].entities.count < confounded.entities.count
            && by_name["many_panels"].entities.count > confounded.entities.count,
        "the panel-count axis does not bracket the baseline"
    );
    assert!(
        by_name["noisy"].mechanism.noise_ratio > confounded.mechanism.noise_ratio,
        "the noisy world is not noisier"
    );
    assert!(
        by_name["flat_entities"].entities.scale_sigma < confounded.entities.scale_sigma,
        "the flat-entity world still has size spread, so demeaning still has work to do"
    );

    // Identification: the only movement in the regressor that is NOT the
    // confounder. The plan calls this the axis most customers fail, and it was
    // a constant in `world.rs` until it became a field.
    assert!(
        by_name["flat_lever"].baseline.budget_jitter_sd < confounded.baseline.budget_jitter_sd,
        "the flat-lever world's budget still moves as much as the baseline's, so \
         the identification axis is not actually swept"
    );

    // Lag error: `lag:` is a human guess on real data, and a world where the
    // guess is right cannot show what a wrong one costs.
    let lag_error = &by_name["lag_error"];
    assert_ne!(
        lag_error.mechanism.declared_lag(),
        lag_error.mechanism.lag_days,
        "the lag-error world declares the lag it generates with, so the fitter is \
         right by construction"
    );

    // The corners where one draw is the draw, not the world.
    for marginal in ["noisy", "few_panels", "flat_lever"] {
        assert!(
            by_name[marginal].replicates > 1,
            "{marginal} sits on a boundary and runs a single seed — its cell of \
             the outcome map would report a coin toss"
        );
    }
}

#[test]
fn the_confounding_axis_moves_the_measured_bias() {
    // Not just that the knob differs — that turning it changes what an estimator
    // sees. Without this the "clean" control could be a relabelled duplicate.
    let by_name: std::collections::HashMap<_, _> = declared()
        .into_iter()
        .map(|(n, s)| (n.trim_end_matches(".simulation.yml").to_string(), s))
        .collect();

    let confounded = check(&by_name["confounded"]).unwrap().bias_ratio();
    let clean = check(&by_name["clean"]).unwrap().bias_ratio();
    assert!(
        (clean - 1.0).abs() < (confounded - 1.0).abs(),
        "removing the confounder did not reduce the measured bias: clean {clean:.3} vs \
         confounded {confounded:.3}"
    );
}

#[test]
fn the_example_world_is_confounded_the_way_its_comments_claim() {
    // `example_new/simulations/marketing_lift.simulation.yml` is what somebody
    // meets first, and its whole point is that a clean within-panel fit on an
    // honest-looking history still overstates the truth — because the budget is
    // set from trailing sales. If that stopped being true the file's commentary
    // would be describing a world that no longer exists.
    let by_name: std::collections::HashMap<_, _> = declared()
        .into_iter()
        .map(|(n, s)| (n.trim_end_matches(".simulation.yml").to_string(), s))
        .collect();

    let spec = &by_name["marketing_lift"];
    let report = check(spec).expect("the example world drifted from its spec");
    eprintln!("marketing_lift:\n{report}");

    assert!(
        report.n_pairs > 4_000,
        "only {} pairs — too thin to say anything",
        report.n_pairs
    );
    assert!(
        report.bias_ratio() > 1.0,
        "the example world no longer overstates the truth (bias {:.3}) — its \
         commentary about a budget set from revenue is now wrong",
        report.bias_ratio()
    );

    // There is no separate hold arm to keep in step any more, and that is the
    // fix: `hold` and `machine` are two runs of THIS world on one seed, so they
    // cannot drift apart in the first place.
}
