//! The loop, against stub sinks and probes.
//!
//! No warehouse, no workspace — the point of the two traits is that the
//! interesting properties (a refusal costs an opportunity not money; a period
//! lands before the run ends; truth reaches the score and not the policy) are
//! all assertable without one.

use super::*;
use crate::policy;
use crate::spec::{
    BaselineSpec, CalibrateSpec, DEFAULT_BUDGET_JITTER_SD, EntitiesSpec, LeverSpec, MechanismSpec,
    PolicyKind, SimulationSpec,
};
use chrono::NaiveDate;

const EDGE: &str = "marketing_spend -> net_sales";

fn spec(periods: u32) -> SimulationSpec {
    SimulationSpec {
        name: "runner".into(),
        description: None,
        seed: 7,
        replicates: 1,
        periods,
        period_days: 7,
        history_days: 180,
        start_date: NaiveDate::from_ymd_opt(2025, 1, 6).unwrap(),
        entities: EntitiesSpec {
            count: 6,
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

/// Collects rows the way the real sink writes them to the dataset dir.
#[derive(Default)]
struct Collector {
    rows: Vec<EntityDay>,
    appends: usize,
}

impl RowSink for Collector {
    fn append(&mut self, rows: &[EntityDay]) -> Result<(), SimulationError> {
        self.rows.extend_from_slice(rows);
        self.appends += 1;
        Ok(())
    }
}

/// Returns the same answer every period.
struct FixedProbe(Probe);

impl SemanticProbe for FixedProbe {
    fn probe(&mut self) -> Result<Probe, SimulationError> {
        Ok(self.0.clone())
    }
}

fn fitted_probe(coefficient: f64, se: f64) -> FixedProbe {
    FixedProbe(Probe {
        fits: vec![(EDGE.to_string(), EdgeFit::fitted(coefficient, se))],
        impact_quantified: true,
    })
}

fn refusing_probe() -> FixedProbe {
    FixedProbe(Probe {
        fits: vec![(EDGE.to_string(), EdgeFit::refused("abs t < 2"))],
        impact_quantified: true,
    })
}

/// Run `arm` against `probe`, returning every period that landed.
///
/// The arm is an argument, not a field of the spec: one world, several
/// policies, which is what makes a profit race between them attributable.
fn drive(
    arm: PolicyKind,
    spec: SimulationSpec,
    probe: &mut FixedProbe,
) -> (Vec<PeriodResult>, RunSummary, Collector) {
    let curve = spec.curve().unwrap();
    let mut policy = policy::build(arm, &spec, curve);
    let mut world = World::new(spec).unwrap();
    let mut sink = Collector::default();

    let mut periods = Vec::new();
    let summary = {
        let mut runner = Runner::new(&mut world, &mut *policy, &mut sink, probe);
        runner
            .run(|p| {
                periods.push(p.clone());
                Ok(())
            })
            .unwrap()
    };
    (periods, summary, sink)
}

#[test]
fn every_period_lands_as_it_completes() {
    // Not at the end. A 40-period run is minutes of warehouse queries, and a
    // result that only exists once the loop returns is one an instance death
    // destroys — which is the entire reason this is a queued task.
    let (periods, summary, _) = drive(PolicyKind::Machine, spec(5), &mut fitted_probe(5.0, 0.01));
    assert_eq!(summary.periods, 5);
    assert_eq!(periods.len(), 5);
    assert_eq!(
        periods.iter().map(|p| p.period).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
}

#[test]
fn the_history_is_written_before_the_first_probe() {
    // Period 1 must fit on a real past. Without the burn-in the run opens on a
    // refusal that reads "unidentified" when it only means "too early", and
    // every convergence claim starts from a lie about what was available.
    let arm = PolicyKind::Machine;
    let s = spec(3);
    let (history_days, entities) = (s.history_days, s.entities.count);
    let (_, _, sink) = drive(arm, s, &mut fitted_probe(5.0, 0.01));
    assert!(
        sink.rows.len() > (history_days * entities) as usize,
        "the sink never received the burn-in history"
    );
    assert_eq!(sink.appends, 4, "one append for history, one per period");
}

#[test]
fn a_refused_edge_costs_an_opportunity_and_not_money() {
    // The taxonomy's first row, asserted end to end: the machine holds, so the
    // run still earns — it simply earns what `hold` earns. A policy that read
    // the refusal as a zero would cut spend and this would go the other way.
    let (periods, summary, _) = drive(PolicyKind::Machine, spec(6), &mut refusing_probe());

    let opening = periods[0].mean_spend;
    assert!(
        periods
            .iter()
            .all(|p| (p.mean_spend - opening).abs() < 1e-9),
        "spend moved on a refusal: {:?}",
        periods.iter().map(|p| p.mean_spend).collect::<Vec<_>>()
    );
    assert!(
        summary.cumulative_profit > 0.0,
        "a refused run lost money, which is not what a refusal costs"
    );
    assert!(
        periods
            .iter()
            .all(|p| p.fits[0].outcome == Outcome::Refused)
    );
}

#[test]
fn a_profitable_slope_moves_spend_up_period_after_period() {
    let (periods, _, _) = drive(PolicyKind::Machine, spec(6), &mut fitted_probe(5.0, 0.01));
    for pair in periods.windows(2) {
        assert!(
            pair[1].mean_spend > pair[0].mean_spend,
            "spend did not climb: {} → {}",
            pair[0].mean_spend,
            pair[1].mean_spend
        );
    }
}

#[test]
fn cumulative_profit_is_the_running_sum_of_the_periods() {
    let (periods, summary, _) = drive(PolicyKind::Hold, spec(5), &mut fitted_probe(5.0, 0.01));
    let mut running = 0.0;
    for p in &periods {
        running += p.realized_profit;
        assert!(
            (p.cumulative_profit - running).abs() < 1e-6,
            "period {} cumulative {} vs running {running}",
            p.period,
            p.cumulative_profit
        );
    }
    assert!((summary.cumulative_profit - running).abs() < 1e-6);
}

#[test]
fn the_score_uses_the_settled_spend_not_the_anchor() {
    // The finding this guards is the easy one to get wrong: a budget set as a
    // share of revenue is a fixed point, so the world does not sit at the
    // anchor the curve was calibrated from. Scoring against the anchor would
    // book that modelling difference as estimator bias.
    let arm = PolicyKind::Hold;
    let s = spec(3);
    let curve = s.curve().unwrap();
    let (periods, _, _) = drive(arm, s, &mut fitted_probe(5.0, 0.01));

    let scored_at = periods[0].fits[0].true_local_slope;
    let at_anchor = curve.local_slope(curve.anchor_spend);
    assert!(
        (scored_at - at_anchor).abs() > 1e-6,
        "the score used the anchor slope {at_anchor}, so the settled spend is being ignored"
    );
}

#[test]
fn actions_carry_every_entity_not_just_the_mean() {
    // A mean cannot answer how much within-panel variation an `explore` arm
    // left behind, which is the only question that arm exists to answer.
    let (periods, _, _) = drive(
        PolicyKind::MachineExplore,
        spec(3),
        &mut fitted_probe(5.0, 0.01),
    );
    let actions = &periods[0].actions;
    assert_eq!(actions.len(), 6);
    let spread = actions.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
        - actions.iter().cloned().fold(f64::INFINITY, f64::min);
    assert!(
        spread > 0.0,
        "explore produced identical actions: {actions:?}"
    );
}

#[test]
fn a_sink_failure_stops_the_run_rather_than_scoring_a_partial_world() {
    // A period whose rows never landed is a period the next fit cannot see.
    // Carrying on would produce a convergence curve for a world that skipped a
    // step — well-formed, confident, and about nothing.
    struct Failing(usize);
    impl RowSink for Failing {
        fn append(&mut self, _: &[EntityDay]) -> Result<(), SimulationError> {
            self.0 += 1;
            if self.0 > 2 {
                return Err(SimulationError::Drift("disk full".into()));
            }
            Ok(())
        }
    }

    let arm = PolicyKind::Hold;
    let s = spec(5);
    let curve = s.curve().unwrap();
    let mut policy = policy::build(arm, &s, curve);
    let mut world = World::new(s).unwrap();
    let mut sink = Failing(0);
    let mut probe = fitted_probe(5.0, 0.01);
    let mut landed = 0;
    let err = Runner::new(&mut world, &mut *policy, &mut sink, &mut probe)
        .run(|_| {
            landed += 1;
            Ok(())
        })
        .unwrap_err();

    assert!(err.to_string().contains("disk full"));
    assert_eq!(landed, 1, "kept running after the sink failed");
}

#[test]
fn outcome_classification_separates_the_three_cases() {
    let truth = 4.0;
    assert_eq!(
        Outcome::classify(&EdgeFit::refused("abs t < 2"), truth, 0.2),
        Outcome::Refused
    );
    assert_eq!(
        Outcome::classify(&EdgeFit::fitted(4.2, 0.01), truth, 0.2),
        Outcome::Converged
    );
    // High t, plausible number, wrong — the only outcome that hurts a customer.
    let confident = EdgeFit::fitted(6.5, 0.01);
    assert!(confident.t_stat.abs() > 2.0);
    assert_eq!(
        Outcome::classify(&confident, truth, 0.2),
        Outcome::ConfidentlyWrong
    );
}

#[test]
fn a_non_finite_beta_is_refused_not_confidently_wrong() {
    // `true_local_slope` is finite-checked; β̂ was not. Both `NaN <= tolerance`
    // and `inf <= tolerance` are false, so a non-finite β̂ fell through to
    // `ConfidentlyWrong` — the one outcome documented as hurting a customer —
    // for a fit that carries no number at all. `Machine::direction` already
    // reads the identical fit as silence, so the same fit scored as the
    // estimator's worst failure and the policy's safest one at once.
    for beta in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            Outcome::classify(&EdgeFit::fitted(beta, 0.01), 4.0, 0.2),
            Outcome::Refused,
            "β̂ {beta} must score as a refusal"
        );
    }
}

#[test]
fn a_sign_flip_is_confidently_wrong_not_converged() {
    // Same magnitude, opposite direction. A tolerance applied to |β̂| − |β| by
    // accident would call this converged, and a policy acting on it moves the
    // lever the wrong way with confidence.
    assert_eq!(
        Outcome::classify(&EdgeFit::fitted(-4.0, 0.01), 4.0, 0.2),
        Outcome::ConfidentlyWrong
    );
}

#[test]
fn the_policy_receives_the_probe_answer_verbatim_and_nothing_else() {
    // The invariant the crate rests on, at the one seam where it could break.
    // The runner is the scorer, so it holds the world's true curve — and the
    // observation it hands the policy must carry the *model's* answer,
    // untouched. Substituting `true_local_slope` for the coefficient here would
    // turn every convergence claim into a tautology and nothing downstream
    // would look wrong.
    //
    // Note this cannot be asserted by comparing two worlds: a different true
    // curve generates different rows, so a policy legitimately acts differently
    // without ever being told anything.
    struct Recorder {
        seen: Vec<(Option<EdgeFit>, Vec<f64>)>,
    }
    impl policy::Policy for Recorder {
        fn name(&self) -> &'static str {
            "recorder"
        }
        fn decide(&mut self, obs: &PeriodObservation<'_>) -> Vec<f64> {
            self.seen
                .push((obs.fit.clone(), obs.trailing_sales.to_vec()));
            obs.current_spend.to_vec()
        }
    }

    let s = spec(4);
    let entity_count = s.entities.count as usize;
    let mut world = World::new(s).unwrap();
    let mut recorder = Recorder { seen: Vec::new() };
    let mut sink = Collector::default();
    let mut probe = fitted_probe(5.0, 0.01);
    Runner::new(&mut world, &mut recorder, &mut sink, &mut probe)
        .run(|_| Ok(()))
        .unwrap();

    assert_eq!(recorder.seen.len(), 4);
    for (fit, trailing) in &recorder.seen {
        assert_eq!(
            fit.as_ref().and_then(|f| f.coefficient),
            Some(5.0),
            "the coefficient the policy saw is not the one the probe returned"
        );
        assert_eq!(fit.as_ref().map(|f| f.se), Some(0.01));
        assert_eq!(trailing.len(), entity_count);
        assert!(
            trailing.iter().all(|s| *s > 0.0),
            "trailing sales were not computed from the emitted rows: {trailing:?}"
        );
    }
}

/// A sink and a probe sharing one row buffer, so the probe can record the
/// window it was actually handed. The real [`FitProbe`] fits over the whole
/// dataset dir with no date bound, so "the rows the sink holds when `probe()`
/// is called" is literally its sample.
#[derive(Clone, Default)]
struct SharedRows(std::rc::Rc<std::cell::RefCell<Vec<EntityDay>>>);

impl RowSink for SharedRows {
    fn append(&mut self, rows: &[EntityDay]) -> Result<(), SimulationError> {
        self.0.borrow_mut().extend_from_slice(rows);
        Ok(())
    }
}

/// Answers the same thing every period, and records the mean driver over the
/// rows it was fitting when it answered.
struct WindowProbe {
    rows: SharedRows,
    seen_mean_spend: Vec<f64>,
    answer: Probe,
}

impl SemanticProbe for WindowProbe {
    fn probe(&mut self) -> Result<Probe, SimulationError> {
        let rows = self.rows.0.borrow();
        let n = rows.len();
        assert!(n > 0, "the probe was called before any rows were written");
        let mean = rows.iter().map(|r| r.marketing_spend).sum::<f64>() / n as f64;
        self.seen_mean_spend.push(mean);
        Ok(self.answer.clone())
    }
}

#[test]
fn the_score_is_evaluated_at_the_spend_the_fit_actually_saw() {
    // β̂ is an average marginal effect over the driver's spread in the fit's own
    // sample, so the truth it is scored against must be the local slope at that
    // sample's mean spend. The fit runs *before* the period acts, so a scoring
    // point that includes the period's own rows evaluates the truth over a
    // window one period wider than the estimator ever saw.
    let s = spec(5);
    let curve = s.curve().unwrap();
    let mut policy = policy::build(PolicyKind::Machine, &s, curve);
    let mut world = World::new(s).unwrap();

    let shared = SharedRows::default();
    let mut sink = shared.clone();
    let mut probe = WindowProbe {
        rows: shared,
        seen_mean_spend: Vec::new(),
        // A confident, profitable slope, so the machine climbs and the two
        // candidate windows separate.
        answer: Probe {
            fits: vec![(EDGE.to_string(), EdgeFit::fitted(5.0, 0.01))],
            impact_quantified: true,
        },
    };

    let mut periods = Vec::new();
    Runner::new(&mut world, &mut *policy, &mut sink, &mut probe)
        .run(|p| {
            periods.push(p.clone());
            Ok(())
        })
        .unwrap();

    for (p, fit_window_mean) in periods.iter().zip(&probe.seen_mean_spend) {
        let expected = curve.local_slope(*fit_window_mean);
        let scored = p.fits[0].true_local_slope;
        assert!(
            (scored / expected - 1.0).abs() < 1e-9,
            "period {}: scored truth {scored:.6}, but the slope at the fit's own window mean \
             spend {fit_window_mean:.4} is {expected:.6} — off by {:.3}%",
            p.period,
            (scored / expected - 1.0) * 100.0
        );
    }
}
