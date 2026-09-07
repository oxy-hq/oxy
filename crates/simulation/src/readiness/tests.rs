//! The diagnostics, against worlds whose answer is known by construction.

use super::*;

fn pair(panel: u32, driver: f64, target: f64) -> PanelPair {
    PanelPair {
        panel,
        driver,
        target,
        trailing_target: None,
    }
}

/// `panels` panels, `per` observations each, entities differing in size by
/// `spread` and the driver moving within a panel by `swing` — **a fraction of
/// that panel's own base**, not an absolute amount.
///
/// Multiplicative on purpose, because the world is: `World` scales an entity's
/// level by `scale[e]` and sets its budget as a share of its own trailing
/// sales, so within-panel spend movement is proportional to entity size. An
/// additive `swing` makes spread and movement independent knobs, which is a
/// regime no declared world produces — and it is precisely the regime in which
/// `between_within_ratio`'s inversion is unreachable, since the inversion needs
/// both variances to rise together.
fn grid(panels: u32, per: usize, spread: f64, swing: f64) -> Vec<PanelPair> {
    let mut out = Vec::new();
    for p in 0..panels {
        let base = 100.0 + spread * p as f64;
        for i in 0..per {
            let wobble = if i % 2 == 0 { swing } else { -swing };
            out.push(pair(p, base * (1.0 + wobble), 1000.0 + base));
        }
    }
    out
}

#[test]
fn a_driver_that_never_moves_inside_a_panel_is_flagged() {
    // The identification question, and the one most customers fail. Plenty of
    // rows, plenty of panels, and nothing to learn from: a within-panel fit can
    // only see movement inside a panel.
    let r = Readiness::measure(&grid(20, 20, 50.0, 0.0));
    assert_eq!(r.n_pairs, 400);
    assert!(
        r.within_cv.is_some_and(|cv| cv < 1e-9),
        "the panels were observed and did not move, which is Some(0.0) — not \
         the `None` that means nobody looked: {:?}",
        r.within_cv
    );
    let codes: Vec<_> = r.concerns().iter().map(|c| c.code).collect();
    assert!(
        codes.contains(&"no_within_panel_variation"),
        "a motionless driver was not flagged: {codes:?}"
    );
}

#[test]
fn a_driver_that_moves_is_not_flagged_for_variation() {
    let r = Readiness::measure(&grid(20, 20, 5.0, 0.20));
    assert!(r.within_cv.is_some_and(|cv| cv > thresholds::MIN_WITHIN_CV));
    let codes: Vec<_> = r.concerns().iter().map(|c| c.code).collect();
    assert!(!codes.contains(&"no_within_panel_variation"), "{codes:?}");
}

/// A driver that barely moves is flagged for the reason that is actually true
/// of it, and the cross-sectional ratio is reported without gating.
///
/// This fixture used to be the `contrast_is_entity_size` case. It does have a
/// huge between/within ratio — but the reason its `se` is wide is the 1% swing
/// in the numerator, not the entity spread in the denominator, and the two are
/// only ever confounded when a fixture moves both at once. `wide` in
/// `entity_spread_does_not_make_a_sharper_panel_look_worse` is the same ratio
/// with real movement, and it is clear.
#[test]
fn a_driver_that_barely_moves_is_flagged_for_its_identifying_variation() {
    let r = Readiness::measure(&grid(20, 20, 500.0, 0.01));
    let codes: Vec<_> = r.concerns().iter().map(|c| c.code).collect();
    assert!(
        codes.contains(&"too_little_identifying_variation"),
        "a barely-moving driver was not flagged: {codes:?} \
         (identifying variation {})",
        r.identifying_variation
    );
    assert!(
        r.between_within_ratio > thresholds::MIN_IDENTIFYING_VARIATION,
        "the ratio is still measured and still large ({}) — it is reported, \
         not gated on",
        r.between_within_ratio
    );
    assert!(
        !codes.iter().any(|c| c.contains("entity_size")),
        "nothing gates on the cross-sectional ratio any more: {codes:?}"
    );
}

/// The ranking defect. Entity spread must not make a SHARPER panel look worse.
///
/// The within estimator's `se` is `σ_resid / sqrt(Σ_p Σ_i (x − x̄_p)²)` — the
/// demeaning discards the between-panel spread before the estimator sees
/// anything, so `between_var` does not appear in it at all. A ratio of two
/// dispersions is therefore not a precision, and gating on it reads the world
/// backwards: in this world spend is a share of the entity's OWN level, so
/// raising entity spread raises both variances, `between` as scale² and
/// therefore faster, and the flag fires exactly as the estimate gets tighter.
///
/// Both panels below move by the same 10% of their own base. `wide` differs
/// only in having entities of widely different sizes — its real `se` is ~56×
/// tighter than `flat`'s, which pooled-RMS-normalised `identifying_variation`
/// reflects by staying level rather than falling (see
/// `identifying_variation_does_not_fall_with_entity_spread_while_the_ratio_explodes`);
/// it is `between_within_ratio`, not `identifying_variation`, that swings wildly
/// between these two.
#[test]
fn entity_spread_does_not_make_a_sharper_panel_look_worse() {
    let flat = Readiness::measure(&grid(20, 20, 0.0, 0.10));
    let wide = Readiness::measure(&grid(20, 20, 500.0, 0.10));

    let codes =
        |r: &Readiness| -> Vec<&'static str> { r.concerns().iter().map(|c| c.code).collect() };
    assert_eq!(
        codes(&flat),
        Vec::<&str>::new(),
        "the flat panel is fine, and is the control"
    );
    assert_eq!(
        codes(&wide),
        Vec::<&str>::new(),
        "the wide-spread panel identifies the slope STRICTLY better — same \
         fractional movement, larger entities, so more identifying variation \
         and a tighter `se`. Flagging it is the ranking inverted."
    );
}

#[test]
fn one_observation_per_panel_spends_every_degree_of_freedom() {
    // dof = n − (n_panels + 1), so this is negative. A fit here is fitting the
    // fixed effects and nothing else.
    let r = Readiness::measure(&grid(40, 1, 10.0, 0.0));
    assert_eq!(r.n_pairs, 40);
    assert_eq!(r.n_panels, 40);
    assert!(r.dof < 0, "dof was {}", r.dof);
    let codes: Vec<_> = r.concerns().iter().map(|c| c.code).collect();
    assert!(codes.contains(&"no_degrees_of_freedom"), "{codes:?}");
}

#[test]
fn a_budget_set_from_revenue_is_flagged() {
    // The modal customer, and the one the plan says we are most likely to sell
    // to. Spend is 2% of trailing sales, so the driver is a linear function of
    // the outcome's history — exactly the confounding the grid exists to map.
    let pairs: Vec<PanelPair> = (0..200)
        .map(|i| {
            let trailing = 1000.0 + (i % 50) as f64 * 40.0;
            PanelPair {
                panel: (i % 10) as u32,
                driver: 0.02 * trailing,
                target: trailing * 1.05,
                trailing_target: Some(trailing),
            }
        })
        .collect();

    let r = Readiness::measure(&pairs);
    let corr = r.driver_trailing_corr.expect("no correlation computed");
    assert!(corr > 0.99, "correlation was {corr}");
    let codes: Vec<_> = r.concerns().iter().map(|c| c.code).collect();
    assert!(codes.contains(&"driver_set_from_outcome"), "{codes:?}");
}

#[test]
fn an_independent_budget_is_not_flagged_for_confounding() {
    // The control. Without it, the flag above could just mean "this statistic
    // always fires".
    let pairs: Vec<PanelPair> = (0..200)
        .map(|i| PanelPair {
            panel: (i % 10) as u32,
            // Deterministic but unrelated to the trailing level.
            driver: 20.0 + ((i * 37) % 11) as f64,
            target: 1000.0,
            trailing_target: Some(1000.0 + (i % 50) as f64 * 40.0),
        })
        .collect();
    let r = Readiness::measure(&pairs);
    let codes: Vec<_> = r.concerns().iter().map(|c| c.code).collect();
    assert!(!codes.contains(&"driver_set_from_outcome"), "{codes:?}");
}

/// The confounding smell test must smell the budget RULE, not entity size.
///
/// Pooled, `corr(driver, trailing_target)` is dominated by how big each entity
/// is: a large restaurant has both more spend and more trailing revenue than a
/// small one, so the pooled figure reads high under any budget rule whatsoever.
/// Here the budget is set INDEPENDENTLY of trailing revenue within each panel —
/// there is no confounding to find — and entity sizes span 512×. Pooled that
/// reads 0.95 and fires; demeaned by panel, which is what the fit sees, it
/// reads 0.
#[test]
fn entity_size_alone_does_not_read_as_a_budget_set_from_revenue() {
    let mut pairs = Vec::new();
    for p in 0..10u32 {
        let level = 100.0 * 2f64.powi(p as i32);
        for i in 0..20 {
            // Two independent square waves at different periods: within a
            // panel, spend and trailing revenue move for unrelated reasons.
            let spend_phase = if i % 2 == 0 { 0.20 } else { -0.20 };
            let trailing_phase = if (i / 2) % 2 == 0 { 0.20 } else { -0.20 };
            pairs.push(PanelPair {
                panel: p,
                driver: level * (1.0 + spend_phase),
                target: 10.0 * level,
                trailing_target: Some(level * (1.0 + trailing_phase)),
            });
        }
    }

    let r = Readiness::measure(&pairs);
    let corr = r
        .driver_trailing_corr
        .expect("a correlation was computable");
    assert!(
        corr.abs() < 0.05,
        "within-panel, spend and trailing revenue are unrelated here; got {corr}"
    );
    let codes: Vec<_> = r.concerns().iter().map(|c| c.code).collect();
    assert!(
        !codes.contains(&"driver_set_from_outcome"),
        "entity size was read as confounding: {codes:?}"
    );
}

/// The whole ranking, in one direction. Entity spread must never make
/// `identifying_variation` fall in a world where spend is a share of an
/// entity's own level — and normalising by pooled RMS instead of `|pooled
/// mean|` earns "scale-free" literally here rather than approximately: the
/// swing in [`grid`] is a fixed *fraction* of each panel's own base, so
/// numerator (`sqrt(within_ss)`) and denominator (pooled RMS) scale by the
/// same factor as spread rises and it cancels — the figure does not merely
/// avoid falling, it does not move at all (up to floating-point noise). The
/// old `|pooled mean|` normaliser lacked that cancellation — Jensen's gap
/// between `mean(x)²` and `mean(x²)` made it climb with spread instead —
/// which is what this test used to assert before the RMS fix.
#[test]
fn identifying_variation_does_not_fall_with_entity_spread_while_the_ratio_explodes() {
    let spreads = [0.0, 100.0, 500.0, 1000.0];
    let measured: Vec<Readiness> = spreads
        .iter()
        .map(|&s| Readiness::measure(&grid(20, 20, s, 0.10)))
        .collect();

    for pair in measured.windows(2) {
        let relative_drop = (pair[0].identifying_variation - pair[1].identifying_variation)
            / pair[0].identifying_variation;
        assert!(
            relative_drop < 1e-6,
            "identifying variation must not fall as entities spread: {} then {}",
            pair[0].identifying_variation,
            pair[1].identifying_variation
        );
    }
    // And the quantity that used to gate goes the other way entirely — 0 to
    // ~26 over the same range, which is why gating on it inverted the ranking.
    assert!(
        measured[3].between_within_ratio > 10.0 * measured[1].between_within_ratio.max(1.0)
            || measured[3].between_within_ratio > 20.0,
        "the ratio should be large at wide spread; got {}",
        measured[3].between_within_ratio
    );
    assert!(
        measured.iter().all(|r| r.is_clear()),
        "every one of these panels is fine"
    );
}

/// "Nothing was observed" is not "nothing moved" — the distinction the field's
/// doc promises, now actually kept.
#[test]
fn all_singleton_panels_report_no_within_cv_rather_than_a_motionless_one() {
    let pairs: Vec<PanelPair> = (0..40u32)
        .map(|p| pair(p, 100.0 + p as f64, 1000.0))
        .collect();
    let r = Readiness::measure(&pairs);

    assert_eq!(
        r.within_cv, None,
        "no panel had two rows to compare, so there is no CV to report"
    );
    let codes: Vec<_> = r.concerns().iter().map(|c| c.code).collect();
    assert!(
        !codes.contains(&"no_within_panel_variation"),
        "reported movement nobody observed: {codes:?}"
    );
    assert!(
        codes.contains(&"no_degrees_of_freedom"),
        "the honest finding is the one about dof: {codes:?}"
    );
    // `too_little_identifying_variation` DOES fire, and correctly: the demeaned
    // regressor is identically zero here, which is a fact about the data rather
    // than a claim about movement nobody watched for.
    assert!(
        codes.contains(&"too_little_identifying_variation"),
        "{codes:?}"
    );
}

#[test]
fn dropped_pairs_are_reported_because_they_move_n() {
    // `n` is what the fitter gates on, so pairs a logged basis silently drops
    // can cause a refusal that nothing on the surface explains.
    let mut pairs = grid(10, 10, 5.0, 0.20);
    for p in pairs.iter_mut().take(20) {
        p.driver = 0.0;
    }
    let r = Readiness::measure(&pairs);
    assert!((r.nonpositive_rate - 0.2).abs() < 1e-9);
    let codes: Vec<_> = r.concerns().iter().map(|c| c.code).collect();
    assert!(codes.contains(&"pairs_dropped"), "{codes:?}");
}

#[test]
fn a_healthy_edge_raises_nothing() {
    // The good case has to be reachable, or the check is just an alarm that is
    // always on and everyone learns to ignore it.
    let pairs: Vec<PanelPair> = (0..300)
        .map(|i| PanelPair {
            panel: (i % 15) as u32,
            driver: 100.0 + ((i * 29) % 40) as f64,
            target: 2000.0 + ((i * 13) % 100) as f64,
            trailing_target: Some(2000.0 + ((i * 7) % 90) as f64),
        })
        .collect();
    let r = Readiness::measure(&pairs);
    assert!(
        r.is_clear(),
        "a healthy edge raised {:?}",
        r.concerns().iter().map(|c| c.code).collect::<Vec<_>>()
    );
}

#[test]
fn a_single_row_panel_is_not_scored_as_motionless() {
    // "Nothing was observed" and "nothing moved" are different claims, and only
    // one of them is a reason to distrust an edge.
    let mut pairs = grid(5, 10, 5.0, 0.20);
    pairs.push(pair(99, 100.0, 1000.0));
    let r = Readiness::measure(&pairs);
    assert!(
        r.within_cv.is_some_and(|cv| cv > thresholds::MIN_WITHIN_CV),
        "the one-row panel dragged the within-panel CV to {:?}",
        r.within_cv
    );
}

/// The bug this regresses: normalising by `|pooled mean|` reads a driver
/// centred near zero as having no identifying variation, when the pooled mean
/// being near zero says nothing about the within-panel movement. Ten panels,
/// each swinging symmetrically about zero (`-10, 10, -8, 8`), so the pooled
/// mean is exactly `0.0` while `within_ss` is large — the old code divided by
/// (approximately) zero and reported `0.0`, firing `too_little_identifying_variation`
/// on a driver that moves enormously inside every panel.
#[test]
fn a_mean_zero_driver_with_large_within_panel_swings_is_not_flagged_for_identifying_variation() {
    let mut pairs = Vec::new();
    for p in 0..10u32 {
        for &d in &[-10.0, 10.0, -8.0, 8.0] {
            pairs.push(pair(p, d, 1000.0));
        }
    }
    let r = Readiness::measure(&pairs);
    assert_eq!(r.n_pairs, 40);

    let codes: Vec<_> = r.concerns().iter().map(|c| c.code).collect();
    assert!(
        !codes.contains(&"too_little_identifying_variation"),
        "a mean-zero driver that swings hugely within every panel was flagged \
         as having too little identifying variation: {codes:?} \
         (identifying variation {})",
        r.identifying_variation
    );
    assert!(
        r.identifying_variation.is_finite() && r.identifying_variation > 1.0,
        "identifying variation should be a large finite number, not the {} a \
         division by a near-zero pooled mean would have produced",
        r.identifying_variation
    );
}

/// The calibration claim in `thresholds::MIN_IDENTIFYING_VARIATION`'s doc: for
/// a strictly-positive spend-shaped driver, pooled RMS ≈ |pooled mean|, so
/// switching the normaliser from one to the other barely moves the figure.
#[test]
fn identifying_variation_is_essentially_unchanged_for_a_positive_spend_shaped_driver() {
    let pairs = grid(20, 20, 5.0, 0.20);
    let r = Readiness::measure(&pairs);

    // Reproduce the pre-fix (`|pooled mean|`) formula independently from the
    // same pairs, so this test actually holds the calibration claim rather
    // than just asserting today's output.
    let mut panels: std::collections::BTreeMap<u32, Vec<f64>> = std::collections::BTreeMap::new();
    for p in &pairs {
        panels.entry(p.panel).or_default().push(p.driver);
    }
    let mut within_ss = 0.0;
    for xs in panels.values() {
        let m = xs.iter().sum::<f64>() / xs.len() as f64;
        within_ss += xs.iter().map(|x| (x - m).powi(2)).sum::<f64>();
    }
    let pooled_mean = pairs.iter().map(|p| p.driver).sum::<f64>() / pairs.len() as f64;
    let old_style = within_ss.sqrt() / pooled_mean.abs();

    let relative_diff = (r.identifying_variation - old_style).abs() / old_style;
    assert!(
        relative_diff < 0.05,
        "RMS normalisation should barely move a strictly-positive spend-shaped \
         driver's figure: old-style (|pooled mean|) {old_style}, new (pooled \
         RMS) {}, relative diff {relative_diff}",
        r.identifying_variation
    );
}

#[test]
fn concerns_carry_a_stable_code_and_a_readable_reason() {
    // The code is what a surface keys off; the detail is what a person reads.
    // A finding with only one of the two is useless to somebody.
    let r = Readiness::measure(&grid(20, 20, 50.0, 0.0));
    for c in r.concerns() {
        assert!(
            !c.code.is_empty()
                && c.code
                    .chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch == '_')
        );
        assert!(c.detail.len() > 20, "unhelpful detail: {}", c.detail);
    }
}
