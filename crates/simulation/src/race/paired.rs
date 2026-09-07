//! The paired-t machinery, kept separate from the shape it reports into.
//!
//! Nothing here knows what a policy is. It takes a vector of differences and
//! either produces a test or names the reason there is none — which is the
//! whole contract, because every degenerate case this crate will actually meet
//! (one replicate, a dead heat, a constant margin) has a t of `0/0` or `x/0`,
//! and a NaN escaping into a "which arm wins" panel is indistinguishable from
//! an answer.

use statrs::distribution::{ContinuousCDF, StudentsT};

use super::{Inference, NoInference, PairedTest};

/// Sample mean. Caller guarantees non-empty.
pub(super) fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

/// Bessel-corrected sample standard deviation, `None` below two observations.
///
/// `n − 1` because the mean it deviates from was estimated from these same
/// values; dividing by `n` would understate the spread and, downstream, quote a
/// confidence in the winning arm that the replicates did not buy.
pub(super) fn sample_sd(values: &[f64]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    let m = mean(values);
    let ss: f64 = values.iter().map(|v| (v - m) * (v - m)).sum();
    Some((ss / (values.len() - 1) as f64).sqrt())
}

/// The two-sided paired t-test on `differences`, or the reason there isn't one.
///
/// **H₀: the mean per-world profit difference is zero** — the two arms are the
/// same policy as far as this world's distribution of draws can tell. The
/// alternative is two-sided: the point of a race is which arm wins, and
/// pre-committing to a direction would let a *loss* read as "not significant".
///
/// `confidence` is assumed already validated by the caller.
pub(super) fn test(differences: &[f64], confidence: f64) -> Inference {
    let n = differences.len();
    match n {
        // Nothing was paired. The two arms were never run on a shared world.
        0 => return Inference::Withheld(NoInference::NoPairs),
        // One world. The margin is a fact about that draw and the spread across
        // draws is undefined, so there is no sampling distribution to place it
        // in. Reporting `p = 1` or `p = 0` here would both be inventions.
        1 => return Inference::Withheld(NoInference::SinglePair),
        _ => {}
    }

    // Checked before the variance, so a dead heat is named as one rather than
    // arriving as the `0/0` case of a constant margin.
    if differences.iter().all(|d| *d == 0.0) {
        return Inference::Withheld(NoInference::IdenticalArms);
    }
    let sd = sample_sd(differences).expect("n >= 2");
    if sd == 0.0 {
        // Every draw moved by the same amount. `t` is infinite and the interval
        // is a point, so the honest report is the margin and the fact that its
        // spread is zero — never `p = 0`, which claims a certainty a handful of
        // worlds cannot underwrite and which no reader would discount.
        return Inference::Withheld(NoInference::ConstantDifference);
    }

    let mean_d = mean(differences);
    let std_error = sd / (n as f64).sqrt();
    let t = mean_d / std_error;
    // One degree of freedom goes to the mean difference itself, so what is left
    // to estimate the spread is `n − 1` — and `n` is the number of *worlds*,
    // not the number of runs. Racing two arms over five replicates is ten runs,
    // five differences and four degrees of freedom.
    let dof = n - 1;

    let dist = StudentsT::new(0.0, 1.0, dof as f64).expect("dof >= 1, unit scale");
    // Survival function rather than `1 − cdf`: the interesting p-values here are
    // small, and the subtraction throws away exactly the digits that decide
    // whether a margin is 1e-3 or 1e-7.
    let p_value = (2.0 * dist.sf(t.abs())).clamp(0.0, 1.0);
    let critical = dist.inverse_cdf(0.5 + confidence / 2.0);

    Inference::Tested(PairedTest {
        std_error,
        t,
        dof,
        p_value,
        confidence,
        interval: (mean_d - critical * std_error, mean_d + critical * std_error),
    })
}
