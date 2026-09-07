//! Can this edge be trusted, before anyone trusts its number?
//!
//! Everything the declared worlds vary is computable from the **panel query the
//! fit already runs** — the batched entity × day pull. That is what makes this
//! cheap enough to run on every baseline rather than as a separate audit, and it
//! is the part of the whole exercise that transfers to real customer data: the
//! simulation calibrates what each threshold must be, and this reports whether a
//! customer clears it.
//!
//! **Not yet wired into any run.** [`Readiness::measure`] has no caller today —
//! nothing in the fitter, a runner, or a race screens the declared worlds by
//! these thresholds. This module is calibrated and unit-tested against the
//! simulation grid, and intended for the fit path, but "**No extra query.**"
//! below is a true statement about its cost when called, not evidence that
//! anything calls it. See `simulations/README.md` for what the calibration
//! numbers currently mean absent a gate.
//!
//! # Why this exists at all
//!
//! `abs t >= 2` says a slope is not zero. It never says the slope is *right*, so
//! on real data `converged` and `confidently wrong` are the same response byte
//! for byte. Worse, the gate gets **more** confident with history while a bias
//! stays put: `t = (β + bias) / se` and `se` shrinks as `1/√n`. A confounded edge
//! with two years of data passes more confidently than the same edge with six
//! months, and is exactly as wrong.
//!
//! So `abs t >= 2` cannot be the only thing between a customer and a bad number.
//! These are the diagnostics that can see what `t` cannot.
//!
//! # A dispersion is not a precision
//!
//! Every gate here has to be checked against the estimator it is standing in
//! for, because the within-panel fit is not the pooled one and its `se` is
//!
//! ```text
//! se(β̂) = σ_resid / sqrt( Σ_p Σ_i (x − x̄_p)² )
//! ```
//!
//! Two things follow, and one of them cost this module a shipped defect.
//!
//! First, the **sum under that square root is the whole precision story on the
//! regressor side** — [`Readiness::identifying_variation`] is that sum, made
//! scale-free by the driver's pooled RMS. Nothing else about the driver's
//! spread enters.
//!
//! Second, **`between_var` does not appear at all.** The demeaning discards it
//! before the estimator sees anything. `between_var / within_var` therefore
//! measures how cross-sectional the data looks, which is worth reporting, and
//! says nothing about how precise the answer will be — and this module used to
//! gate on it. In these worlds that gate was not merely uninformative but
//! *inverted*: spend is a share of an entity's own trailing sales, so raising
//! `entities.scale_sigma` raises both variances with `between` going as scale²,
//! and the flag fired hardest exactly where `se` was tightest.
//!
//! The same trap is why [`Readiness::driver_trailing_corr`] is demeaned by panel
//! before it is correlated. Pooled, it tracks entity size rather than the budget
//! rule, so it reads near 1 under any rule at all once entities differ in size.
//!
//! The general rule, for anything added here: state which term of `se` — or
//! which term of the *bias* — the new measurement is, and if it is neither, it
//! is reported rather than gated on.

use std::collections::BTreeMap;

/// One (panel, period) observation of a driver and its target, already paired at
/// the declared lag. This is the shape the fitter's own pull produces.
#[derive(Debug, Clone, Copy)]
pub struct PanelPair {
    pub panel: u32,
    /// The driver on day `d`.
    pub driver: f64,
    /// The target on day `d + lag`.
    pub target: f64,
    /// The driver's own trailing level, used only for the "is spend set from
    /// sales?" smell test. `None` where the caller cannot supply it.
    pub trailing_target: Option<f64>,
}

/// What the data can and cannot support, per edge.
///
/// Every field is a *measurement*, not a verdict. [`Readiness::concerns`] turns
/// them into findings against thresholds the grid calibrates — deliberately
/// separate, so a threshold can move without the measurement changing meaning.
#[derive(Debug, Clone, PartialEq)]
pub struct Readiness {
    /// Paired observations after the lag.
    pub n_pairs: usize,
    pub n_panels: usize,
    /// `n − (n_panels + 1)`. What the within-panel fit actually has to work
    /// with, and it can be negative — one period per panel spends every degree
    /// of freedom on the fixed effects.
    pub dof: i64,
    /// Mean within-panel coefficient of variation of the driver.
    ///
    /// **The identification question, per panel and scale-free.** A within-panel
    /// fit can only learn from movement *inside* a panel; an edge whose driver
    /// never moves has nothing to identify the slope from, however many rows it
    /// has.
    ///
    /// `None` when no panel had two observations to compare — which is not the
    /// same claim as `Some(0.0)`. "Nothing moved" is a fact about the data;
    /// "nothing was observed" is a fact about the pull, and only the first is a
    /// reason to distrust an edge. When every panel is a singleton `dof` is
    /// already negative, so [`Concern`] `no_degrees_of_freedom` says the real
    /// thing and this one stays quiet.
    pub within_cv: Option<f64>,
    /// `sqrt(Σ_p Σ_i (x − x̄_p)²) / sqrt(mean(x²))` — the identifying variation
    /// the within estimator actually has, relative to the driver's own pooled
    /// RMS level.
    ///
    /// **This is the one that predicts `se`.** The within estimator's standard
    /// error is `σ_resid / sqrt(Σ_p Σ_i (x − x̄_p)²)`, so the sum under this
    /// square root is literally the denominator of its precision. Dividing by
    /// the pooled RMS makes it scale-free, so a threshold does not depend on
    /// whether spend is recorded in dollars or thousands; the `sqrt(n)` growth
    /// is kept, because more of the same movement genuinely is more
    /// identification.
    ///
    /// RMS, not `|pooled mean|`: a driver centred near zero — a signed index, a
    /// net-change column, a demeaned score — can have a pooled mean near zero
    /// while moving enormously within every panel, and dividing by that mean
    /// reported an astronomical (or, past `f64::EPSILON`, an exactly-`0.0`)
    /// figure for a driver with plenty to identify from. RMS cannot do that:
    /// `within_ss = Σ_p Σ_i (x − x̄_p)² ≤ Σ_i x² = n · RMS²` (dropping a mean
    /// only shrinks a sum of squares), so the ratio is bounded above by
    /// `sqrt(n)` and falls to `0.0` only as RMS itself does. For a
    /// strictly-positive spend-shaped driver with modest variation,
    /// `RMS = sqrt(x̄² + pooled variance) ≈ |x̄|`, so this is close to the old
    /// figure on the shape [`thresholds::MIN_IDENTIFYING_VARIATION`] was
    /// calibrated on — the change is in what it does for the shapes that used
    /// to break it.
    ///
    /// What it does NOT see is `σ_resid`. This is a statement about the
    /// regressor side alone — a panel can have ample identifying variation and
    /// still fit badly because the target is noisy. That is the same division of
    /// labour the module doc draws around `t`: these diagnostics see what `t`
    /// cannot, and `t` sees what they cannot.
    ///
    /// `0.0` when the pooled RMS is indistinguishable from zero — which now
    /// honestly means the column is all zeros, since RMS can only vanish that
    /// way.
    pub identifying_variation: f64,
    /// Between-panel variance over within-panel variance.
    ///
    /// High means the contrast in the data is mostly *entity size* rather than
    /// anything moving, and that is worth reporting: a POOLED fit would read
    /// that cross-section as an effect.
    ///
    /// **Reported, never gated on.** It used to raise a concern, and that was
    /// backwards. `between_var` does not appear in the within estimator's `se`
    /// at all — the demeaning discards it before the estimator sees anything —
    /// so this is a ratio of two dispersions being read as a precision. It has
    /// two ways to rise and only one of them (`within_var` falling) is bad news;
    /// the other is pure nuisance. In these worlds it is the *nuisance* that
    /// dominates, because spend is a share of an entity's own trailing sales:
    /// raising `entities.scale_sigma` raises both variances, `between` as
    /// scale² and so faster, and a gate here fired precisely as the estimate got
    /// sharper. [`Readiness::identifying_variation`] is what that gate was
    /// reaching for. See `entity_spread_does_not_make_a_sharper_panel_look_worse`.
    pub between_within_ratio: f64,
    /// **Within-panel** correlation of the driver with the target's trailing
    /// level.
    ///
    /// **Is spend set from sales?** Near-universal, and the single biggest
    /// source of confounding. `None` when no trailing level was supplied, or
    /// when no panel had two of them to demean.
    ///
    /// Within-panel because that is what the fit sees. Pooled, this quantity is
    /// dominated by entity size — a big restaurant has both more spend and more
    /// trailing revenue than a small one, so the pooled figure reads near 1
    /// under any budget rule whatsoever and near 0 under none, tracking
    /// `scale_sigma` rather than the rule. `simulations/README.md` already says
    /// of the pooled figure that it "says nothing"; demeaning by panel first is
    /// what makes it say something.
    pub driver_trailing_corr: Option<f64>,
    /// Share of pairs whose driver or target is non-positive — dropped by any
    /// logged basis, so it silently moves `n`, and `n` is what the fit gates on.
    pub nonpositive_rate: f64,
}

/// A named reason to distrust an edge's number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Concern {
    pub code: &'static str,
    pub detail: String,
}

/// Thresholds. Simulation is what calibrates these; the values here are the
/// starting points the grid will move.
pub mod thresholds {
    /// Below this within-panel CV there is essentially nothing to identify from.
    /// Most customers sit near this end — a budget that barely moves.
    pub const MIN_WITHIN_CV: f64 = 0.05;
    /// Mirrors the fitter's own floor.
    pub const MIN_PAIRS: usize = 30;
    /// Below this identifying variation, the `se` will be wide whatever the
    /// within-panel CV looks like per panel.
    ///
    /// Derived rather than guessed, so the grid has somewhere to start:
    /// `identifying_variation ≈ within_cv × sqrt(n − n_panels)`, so an edge
    /// sitting exactly on the other two floors — [`MIN_WITHIN_CV`] of 0.05 at
    /// [`MIN_PAIRS`] of 30 — lands near `0.05 × √30 ≈ 0.27`. This derivation
    /// uses `within_cv`, which normalises by each PANEL's own mean, while
    /// `identifying_variation` normalises by the driver's POOLED RMS — the two
    /// coincide for a strictly-positive driver with modest variation (RMS ≈
    /// |mean| there, per panel and pooled alike), which is the shape both
    /// floors are calibrated on, so the derivation still holds for it. Rounding
    /// to 0.3 makes this gate about as strict as those two together at their boundary,
    /// and it separates the declared worlds cleanly: the motionless and
    /// one-percent-swing grids come in at 0.00 and 0.20, everything healthy at
    /// 1.4 and above.
    ///
    /// Note what replacing `MAX_BETWEEN_WITHIN` with this changes in kind, not
    /// just in value: the old gate fired on a ratio being too HIGH, this one
    /// fires on a quantity being too LOW.
    pub const MIN_IDENTIFYING_VARIATION: f64 = 0.3;
    /// Above this correlation, treat the driver as set *from* the outcome.
    pub const MAX_TRAILING_CORR: f64 = 0.5;
    /// Above this share of dropped pairs, `n` no longer means what it says.
    pub const MAX_NONPOSITIVE: f64 = 0.05;
}

impl Readiness {
    /// Measure an edge from pairs the fit already pulled. **No extra query.**
    pub fn measure(pairs: &[PanelPair]) -> Self {
        let mut panels: BTreeMap<u32, Vec<&PanelPair>> = BTreeMap::new();
        for p in pairs {
            panels.entry(p.panel).or_default().push(p);
        }

        let n_pairs = pairs.len();
        let n_panels = panels.len();
        let dof = n_pairs as i64 - (n_panels as i64 + 1);

        // Within-panel CV, averaged over panels that have more than one
        // observation. A one-row panel has no within-variation by definition and
        // must not be scored as if it had zero — that would report "no movement"
        // where the honest answer is "nothing was observed".
        let mut cvs = Vec::new();
        let mut within_ss = 0.0;
        let mut panel_means = Vec::new();
        for rows in panels.values() {
            let xs: Vec<f64> = rows.iter().map(|r| r.driver).collect();
            let mean = mean(&xs);
            panel_means.push(mean);
            if xs.len() < 2 {
                continue;
            }
            let var = variance(&xs, mean);
            within_ss += var * (xs.len() - 1) as f64;
            if mean.abs() > f64::EPSILON {
                cvs.push(var.sqrt() / mean.abs());
            }
        }
        // `None`, not `0.0`: with no panel of two rows there is nothing to have
        // moved, and reporting "the driver does not move" would be an
        // observation nobody made. See the field's doc.
        let within_cv = (!cvs.is_empty()).then(|| mean(&cvs));
        let within_var = if n_pairs > n_panels {
            within_ss / (n_pairs - n_panels) as f64
        } else {
            0.0
        };
        // The denominator of the within estimator's `se`, made scale-free by
        // the driver's pooled RMS — see the field's doc for why RMS and not
        // `|pooled mean|`. `within_ss ≤ Σ x²` (dropping a mean only shrinks a
        // sum of squares), so this ratio is bounded above by `sqrt(n)` and
        // cannot blow up the way dividing by a near-zero mean did.
        let pooled_driver_rms = mean(
            &pairs
                .iter()
                .map(|p| p.driver * p.driver)
                .collect::<Vec<_>>(),
        )
        .sqrt();
        let identifying_variation = if pooled_driver_rms > f64::EPSILON {
            within_ss.sqrt() / pooled_driver_rms
        } else {
            0.0
        };
        let between_var = if panel_means.len() > 1 {
            variance(&panel_means, mean(&panel_means))
        } else {
            0.0
        };
        let between_within_ratio = if within_var > f64::EPSILON {
            between_var / within_var
        } else {
            f64::INFINITY
        };

        // Demeaned by panel before correlating, because a pooled correlation
        // here measures entity size rather than the budget rule — see the
        // field's doc. A panel with one usable row contributes nothing: its
        // deviation from its own mean is zero by construction, and a pile of
        // exact zeros would drag the correlation toward whatever the panels
        // with real movement happen to say.
        let mut trailing: Vec<(f64, f64)> = Vec::new();
        for rows in panels.values() {
            let usable: Vec<(f64, f64)> = rows
                .iter()
                .filter_map(|r| r.trailing_target.map(|t| (r.driver, t)))
                .collect();
            if usable.len() < 2 {
                continue;
            }
            let mx = mean(&usable.iter().map(|(d, _)| *d).collect::<Vec<_>>());
            let my = mean(&usable.iter().map(|(_, t)| *t).collect::<Vec<_>>());
            trailing.extend(usable.into_iter().map(|(d, t)| (d - mx, t - my)));
        }
        let driver_trailing_corr = (trailing.len() > 2).then(|| correlation(&trailing));

        let nonpositive = pairs
            .iter()
            .filter(|p| p.driver <= 0.0 || p.target <= 0.0)
            .count();
        let nonpositive_rate = if n_pairs == 0 {
            0.0
        } else {
            nonpositive as f64 / n_pairs as f64
        };

        Self {
            n_pairs,
            n_panels,
            dof,
            within_cv,
            identifying_variation,
            between_within_ratio,
            driver_trailing_corr,
            nonpositive_rate,
        }
    }

    /// Everything about this edge that should stop someone trusting its number.
    ///
    /// Empty is the good case. Note that a concern here is **not** a refusal:
    /// the fit may still clear `abs t >= 2` and produce a confident coefficient.
    /// That is the whole point — these see what `t` cannot.
    pub fn concerns(&self) -> Vec<Concern> {
        let mut out = Vec::new();
        if self.n_pairs < thresholds::MIN_PAIRS {
            out.push(Concern {
                code: "too_few_pairs",
                detail: format!(
                    "{} paired observations, under the fitter's floor of {}",
                    self.n_pairs,
                    thresholds::MIN_PAIRS
                ),
            });
        }
        if self.dof <= 0 {
            out.push(Concern {
                code: "no_degrees_of_freedom",
                detail: format!(
                    "dof {} — the panel fixed effects consume every observation",
                    self.dof
                ),
            });
        }
        // `None` raises nothing: no panel had two rows to compare, which
        // `no_degrees_of_freedom` above has already said in the honest words.
        if let Some(cv) = self.within_cv
            && cv < thresholds::MIN_WITHIN_CV
        {
            out.push(Concern {
                code: "no_within_panel_variation",
                detail: format!(
                    "within-panel CV {cv:.3} — the driver barely moves inside a panel, so there \
                     is little to identify the slope from however many rows there are"
                ),
            });
        }
        if self.identifying_variation < thresholds::MIN_IDENTIFYING_VARIATION {
            out.push(Concern {
                code: "too_little_identifying_variation",
                detail: format!(
                    "identifying variation {:.2} — this is the quantity the within estimator's \
                     `se` divides by, so the slope will be imprecise however many rows there \
                     are. Note this is about the DRIVER's movement only: a noisy target widens \
                     `se` further and is not visible here",
                    self.identifying_variation
                ),
            });
        }
        if let Some(corr) = self.driver_trailing_corr
            && corr.abs() > thresholds::MAX_TRAILING_CORR
        {
            out.push(Concern {
                code: "driver_set_from_outcome",
                detail: format!(
                    "the driver correlates {corr:.2} with the target's trailing level — budget \
                     set as a share of revenue. This biases the estimate, and MORE history \
                     makes the gate more confident without making it less wrong"
                ),
            });
        }
        if self.nonpositive_rate > thresholds::MAX_NONPOSITIVE {
            out.push(Concern {
                code: "pairs_dropped",
                detail: format!(
                    "{:.0}% of pairs are non-positive and drop out of any logged basis, so `n` \
                     is smaller than it looks",
                    self.nonpositive_rate * 100.0
                ),
            });
        }
        out
    }

    /// Whether anything at all was found. Convenience for a caller that only
    /// wants a badge.
    pub fn is_clear(&self) -> bool {
        self.concerns().is_empty()
    }
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

fn variance(values: &[f64], mean: f64) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (values.len() - 1) as f64
}

fn correlation(pairs: &[(f64, f64)]) -> f64 {
    let xs: Vec<f64> = pairs.iter().map(|p| p.0).collect();
    let ys: Vec<f64> = pairs.iter().map(|p| p.1).collect();
    let (mx, my) = (mean(&xs), mean(&ys));
    let cov: f64 = pairs.iter().map(|(x, y)| (x - mx) * (y - my)).sum();
    let sx: f64 = xs.iter().map(|x| (x - mx).powi(2)).sum::<f64>().sqrt();
    let sy: f64 = ys.iter().map(|y| (y - my).powi(2)).sum::<f64>().sqrt();
    if sx <= f64::EPSILON || sy <= f64::EPSILON {
        return 0.0;
    }
    cov / (sx * sy)
}

#[cfg(test)]
mod tests;
