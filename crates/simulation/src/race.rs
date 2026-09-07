//! Which arm won the profit race, and whether the margin is a finding.
//!
//! [`RunSummary::cumulative_profit`](crate::RunSummary) is one number from one
//! draw of a world. A world declaring `replicates:` produces several, and until
//! this module existed the race was settled by reading them off a chart — which
//! is why it existed to be settled: a policy that is genuinely worse can win
//! four draws out of five when the draws swing harder than the policies do.
//!
//! # Why paired, and not Welch
//!
//! `replicate_seed(base, replicate)` in the app's run fan-out takes **no policy
//! argument**, and [`World`](crate::World) draws its exogenous streams —
//! `entity_scale`, `demand_shock`, `target_noise` — from the spec seed alone.
//! So replicate *k* of every arm is the *same world*: the same entity sizes,
//! the same shock path, the same noise, differing only in what the policy did
//! about them. That is common random numbers, and it makes the arms paired.
//!
//! Pairing is not a refinement here, it is the measurement. The world's own
//! swing is the dominant variance term and it is *shared*, so differencing
//! within a replicate cancels it outright and what is left is the policy
//! effect. A two-sample test would put that shared swing back into the standard
//! error and report a real effect as noise — `pairing_is_what_makes_the_effect
//! _visible` in the tests is that exact scenario, at p = 4e-7 paired against
//! p = 0.96 unpaired, on the same sixteen numbers.
//!
//! # What a p-value here does and does not license
//!
//! It licenses one claim: **over the distribution of worlds this spec
//! generates, these two arms differ in mean cumulative profit.** It says
//! nothing about whether the spec resembles a customer — a declared world is a
//! world *we chose*, and a tight interval around a large margin is a statement
//! about the estimator's behaviour under that choice, not a forecast of
//! revenue. It also says nothing about *why*: an arm can win on profit while
//! its fits are `ConfidentlyWrong`, and [`Outcome`](crate::Outcome) is the
//! surface that catches that.
//!
//! **Multiplicity is not handled.** [`profit_race`] runs each challenger
//! against the baseline independently and every p-value it returns is
//! *per-comparison*. Racing the four non-baseline arms at α = 0.05 gives a
//! family-wise error near 1 − 0.95⁴ ≈ 18%, so "one arm cleared 0.05" is a much
//! weaker statement than it looks when four were tried. A surface that ranks
//! arms should either say per-comparison in the copy or apply a correction —
//! and the correction belongs wherever the family is decided, which is not
//! here: this module cannot know how many comparisons a caller ran.

use std::collections::BTreeMap;

use crate::spec::PolicyKind;

mod paired;

/// The interval level a comparison reports when the caller does not choose one.
pub const DEFAULT_CONFIDENCE: f64 = 0.95;

/// One arm's finished replicates, keyed by replicate index.
///
/// Keyed rather than positional on purpose. A replicate can fail — a warehouse
/// read that never returned, a run an instance death took — and the arms are
/// then ragged. Zipping two vectors would pair the treatment's world 3 against
/// the baseline's world 4 and every later world one off, which does not lose
/// the comparison so much as fabricate one: the differences would then be
/// mostly world-to-world variance, which is the largest term in the whole
/// exercise and the one pairing exists to remove.
#[derive(Debug, Clone, PartialEq)]
pub struct ArmProfits {
    pub arm: PolicyKind,
    profits: BTreeMap<u32, f64>,
}

/// One arm, aggregated over whatever subset it was scored on.
///
/// `mean` and `sd` are [`Option`] rather than `f64` so that "no replicates" and
/// "one replicate" cannot leave the module as a NaN. A NaN reaching a panel
/// renders as a blank or a `—` and reads as *zero profit*, which is a specific
/// wrong answer rather than a missing one.
#[derive(Debug, Clone, PartialEq)]
pub struct ArmSummary {
    pub arm: PolicyKind,
    /// Replicates this summary covers.
    pub n: usize,
    /// `None` when `n == 0`.
    pub mean: Option<f64>,
    /// Bessel-corrected. `None` when `n < 2` — one draw has no spread.
    pub sd: Option<f64>,
}

/// The paired t-test, when there was one to run.
#[derive(Debug, Clone, PartialEq)]
pub struct PairedTest {
    /// `sd(differences) / √n`.
    pub std_error: f64,
    /// `mean(differences) / std_error`.
    pub t: f64,
    /// `n_pairs − 1`, where `n_pairs` counts worlds, not runs.
    pub dof: usize,
    /// Two-sided, against H₀ of a zero mean difference.
    pub p_value: f64,
    /// The level [`interval`](PairedTest::interval) was built at.
    pub confidence: f64,
    /// Confidence interval on the **mean difference**, not on either arm.
    pub interval: (f64, f64),
}

/// Why a comparison carries no test. Every variant is a case that occurs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoInference {
    /// No replicate index the two arms both scored. Either arm may be empty,
    /// or they may simply not overlap.
    NoPairs,
    /// One shared world. The margin is reported; inference from a single draw
    /// would be a number with no sampling distribution behind it.
    SinglePair,
    /// Every difference was exactly zero — a dead heat, and `t = 0/0`.
    IdenticalArms,
    /// Every difference was the same non-zero number. `t` is infinite, and the
    /// implied `p = 0` would overstate what a few worlds can support.
    ConstantDifference,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Inference {
    Tested(PairedTest),
    Withheld(NoInference),
}

/// One arm against one baseline, on the worlds they both ran.
#[derive(Debug, Clone, PartialEq)]
pub struct PairedComparison {
    /// Summarised over the paired subset only, so the two arms and the
    /// difference are all describing the same worlds.
    pub treatment: ArmSummary,
    pub baseline: ArmSummary,
    /// Worlds both arms scored, after dropping non-finite ones. The sample
    /// size of the test.
    pub n_pairs: usize,
    /// Replicates present in one arm and not the other — a failed or
    /// still-running run. Surfaced rather than swallowed: a race quietly
    /// decided on two of five worlds is a different claim from one decided on
    /// five.
    pub dropped_unpaired: usize,
    /// Pairs discarded because a profit, or their difference, was not finite.
    /// Always an upstream bug; counted here so it is visible instead of
    /// poisoning the mean.
    pub dropped_nonfinite: usize,
    /// `mean(treatment − baseline)`. Positive means the treatment earned more.
    /// `None` only when `n_pairs == 0`.
    pub mean_difference: Option<f64>,
    pub inference: Inference,
}

impl ArmProfits {
    pub fn new(arm: PolicyKind) -> Self {
        Self {
            arm,
            profits: BTreeMap::new(),
        }
    }

    /// Record one replicate's cumulative profit, returning any value it
    /// displaced — a re-run of the same replicate overwrites rather than
    /// accumulating, since two rows for one world are one world.
    pub fn observe(&mut self, replicate: u32, cumulative_profit: f64) -> Option<f64> {
        self.profits.insert(replicate, cumulative_profit)
    }

    pub fn collect(arm: PolicyKind, rows: impl IntoIterator<Item = (u32, f64)>) -> Self {
        let mut out = Self::new(arm);
        for (replicate, profit) in rows {
            out.observe(replicate, profit);
        }
        out
    }

    pub fn len(&self) -> usize {
        self.profits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.profits.is_empty()
    }

    /// This arm on its own, over every replicate it holds.
    ///
    /// Note this is **not** what a [`PairedComparison`] reports: that one
    /// summarises the paired subset, because an arm's mean over five worlds and
    /// another's over three are not comparable numbers.
    pub fn summary(&self) -> ArmSummary {
        let values: Vec<f64> = self.profits.values().copied().collect();
        summarize(self.arm, &values)
    }
}

fn summarize(arm: PolicyKind, values: &[f64]) -> ArmSummary {
    ArmSummary {
        arm,
        n: values.len(),
        mean: (!values.is_empty()).then(|| paired::mean(values)),
        sd: paired::sample_sd(values),
    }
}

/// One arm against one baseline at [`DEFAULT_CONFIDENCE`].
pub fn compare(treatment: &ArmProfits, baseline: &ArmProfits) -> PairedComparison {
    compare_at_confidence(treatment, baseline, DEFAULT_CONFIDENCE)
}

/// As [`compare`], with the interval at a chosen level.
///
/// A `confidence` outside `(0, 1)` is a caller bug — most often a percentage
/// where a proportion was wanted. It falls back to [`DEFAULT_CONFIDENCE`] and
/// warns, rather than panicking a run or handing `inverse_cdf` a value it will
/// assert on: the comparison itself is still correct, and only the interval's
/// width was ever in question.
pub fn compare_at_confidence(
    treatment: &ArmProfits,
    baseline: &ArmProfits,
    confidence: f64,
) -> PairedComparison {
    let confidence = if confidence > 0.0 && confidence < 1.0 {
        confidence
    } else {
        tracing::warn!(
            confidence,
            "confidence level is not a proportion in (0, 1); using {DEFAULT_CONFIDENCE}"
        );
        DEFAULT_CONFIDENCE
    };

    let paired_rows = pair_up(treatment, baseline);
    let matched = paired_rows.matched;
    let dropped_unpaired = (treatment.len() - matched) + (baseline.len() - matched);

    let (t_values, b_values): (Vec<f64>, Vec<f64>) = paired_rows.rows.iter().copied().unzip();
    let differences: Vec<f64> = paired_rows.rows.iter().map(|(t, b)| t - b).collect();

    PairedComparison {
        treatment: summarize(treatment.arm, &t_values),
        baseline: summarize(baseline.arm, &b_values),
        n_pairs: differences.len(),
        dropped_unpaired,
        dropped_nonfinite: paired_rows.dropped_nonfinite,
        mean_difference: (!differences.is_empty()).then(|| paired::mean(&differences)),
        inference: paired::test(&differences, confidence),
    }
}

/// Every challenger against one baseline, in the order given.
///
/// **The p-values are per-comparison** — see the module docs. This exists
/// because a race has a reference arm (`legacy` is what a customer does today,
/// `hold` is the null), not because comparing every pair would be wrong; it
/// would just be a larger family with the same missing correction.
pub fn profit_race<'a>(
    baseline: &ArmProfits,
    challengers: impl IntoIterator<Item = &'a ArmProfits>,
) -> Vec<PairedComparison> {
    challengers
        .into_iter()
        .map(|arm| compare(arm, baseline))
        .collect()
}

/// The replicates both arms scored with a usable number.
struct Paired {
    /// `(treatment, baseline)` per shared, finite replicate.
    rows: Vec<(f64, f64)>,
    /// Replicate indices present in both arms, before the finiteness filter.
    matched: usize,
    dropped_nonfinite: usize,
}

fn pair_up(treatment: &ArmProfits, baseline: &ArmProfits) -> Paired {
    let mut out = Paired {
        rows: Vec::new(),
        matched: 0,
        dropped_nonfinite: 0,
    };
    for (replicate, t) in &treatment.profits {
        let Some(b) = baseline.profits.get(replicate) else {
            continue;
        };
        out.matched += 1;
        // The difference is checked too, not just the operands: two finite
        // profits at the edges of the range subtract to an infinity, and that
        // pair is no more usable than a NaN one.
        if !t.is_finite() || !b.is_finite() || !(t - b).is_finite() {
            out.dropped_nonfinite += 1;
            continue;
        }
        out.rows.push((*t, *b));
    }
    out
}

#[cfg(test)]
mod tests;
