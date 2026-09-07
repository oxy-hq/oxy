//! What acts on the model's answer, period after period.
//!
//! The profit race is the practical half of the ask, and it is only meaningful
//! if the `machine` arm is *the product*. That makes two things load-bearing,
//! and both are asserted below rather than left to review:
//!
//! * **A refusal is not a zero.** When the fitter declines an edge, the shipped
//!   surface propagates nothing. A policy that read the missing coefficient as
//!   `0.0` would conclude marginal profit is `−1` and cut the budget to the
//!   floor — scoring a product we do not ship, and scoring it as a disaster.
//! * **The policy never sees the truth.** It decides from [`EdgeFit`], which
//!   carries exactly what `baseline` returns. [`Oracle`] is the one exception,
//!   and it exists to *be* the exception: it is the ceiling regret is measured
//!   against, so it is told the answer on purpose.
//!
//! # Why the machine settles
//!
//! The fit is linear, so in the model's own view marginal profit `m·β̂ − 1` does
//! not fall as spend rises — believed literally, it says "spend more" for ever.
//! Two things turn it around. The clip bounds how far one period may move, so
//! the next refit lands on a flatter part of the true curve and β̂ falls. And the
//! move is gated on the estimate being *distinguishable* from break-even: once
//! the confidence interval on `m·β̂` covers 1, the policy stops.
//!
//! That gate is what makes the predicted failure mode reachable. A policy that
//! settles stops producing variation; `se` grows, `t` falls under the fitter's
//! floor, and the edge is refused — the machine going quiet exactly when it has
//! finished working. [`MachineExplore`] is the candidate fix, and the reason it
//! ships alongside rather than after.

use crate::rng::Rng;
use crate::spec::{LeverSpec, PolicyKind, ResponseCurve, SimulationSpec};
use crate::world::legacy_budget;

/// Whether a coefficient can be read as a marginal effect at all.
///
/// **Not** a catalogue of forms, deliberately. The fitter picks a basis by AIC
/// from nine-plus shapes, several with multiple terms, and airlayer's own
/// guidance is that a consumer should read the sampled `profile` rather than
/// interpret coefficients — "exactly what a per-form solver and per-form unit
/// wording could not do". So this crate makes the one distinction it can defend
/// and refuses the rest.
///
/// This exists because dropping it cost a factor of ~43 on the first real run:
/// an edge that declared no form came back log-log, and its elasticity read as
/// a level slope is wrong by `target / driver`, silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FitForm {
    /// `Y = a + bX` — the coefficient already *is* `dY/dX`.
    #[default]
    Linear,
    /// Any curved or logged basis. Its coefficient is in other units, and this
    /// crate will not guess which — it reads the sampled response instead. See
    /// [`EdgeFit::level_slope`].
    NonLinear,
}

/// One edge's fit, as `baseline` reports it.
///
/// Mirrors the decision-relevant fields of airlayer's `FittedDriver` rather than
/// importing it, for the same reason the scorer re-implements OLS: this crate is
/// kept free of the engine it measures. The runner owns the one conversion, so
/// there is a single place for the two to be reconciled if the shape moves.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeFit {
    /// The within-panel coefficient, **in `form`'s units** — `None` exactly when
    /// the fit was refused. Read it through [`EdgeFit::level_slope`], never
    /// directly.
    pub coefficient: Option<f64>,
    pub form: FitForm,
    /// The fitter's own name for the basis it chose (`log-log`, `quadratic`, …).
    /// Recorded so a run says *which* shape a number was measured in.
    pub form_name: String,
    /// The response sampled as `(relative lever change, target delta)` — what
    /// airlayer tells consumers to read *instead of* interpreting coefficients,
    /// because it stays correct when a new basis is added. `r = 0` is no change;
    /// the range is roughly `[-0.9, +2.0]`, narrowed to what the fit actually saw.
    pub profile: Vec<(f64, f64)>,
    /// The lever's current aggregate over the fit's window. Needed because the
    /// profile's x-axis is a *proportion*, and a policy moves money.
    pub driver_value: Option<f64>,
    pub se: f64,
    pub t_stat: f64,
    pub n: usize,
    pub n_panels: usize,
    /// Why no coefficient was produced, verbatim from the fitter.
    pub refusal: Option<String>,
}

impl EdgeFit {
    /// A linear fit, whose coefficient is already a marginal effect.
    pub fn fitted(coefficient: f64, se: f64) -> Self {
        Self {
            coefficient: Some(coefficient),
            form: FitForm::Linear,
            form_name: "linear".to_string(),
            profile: Vec::new(),
            driver_value: None,
            se,
            t_stat: if se > 0.0 { coefficient / se } else { 0.0 },
            n: 0,
            n_panels: 0,
            refusal: None,
        }
    }

    pub fn refused(reason: impl Into<String>) -> Self {
        Self {
            coefficient: None,
            form: FitForm::Linear,
            form_name: "linear".to_string(),
            profile: Vec::new(),
            driver_value: None,
            se: 0.0,
            t_stat: 0.0,
            n: 0,
            n_panels: 0,
            refusal: Some(reason.into()),
        }
    }

    /// `dY/dX` at the current operating point — target units per driver unit.
    ///
    /// The only reading a policy or a scorer may use, because it is the only one
    /// whose meaning does not depend on the basis. A linear fit's coefficient
    /// already *is* this; anything else is read off [`Self::profile`] rather
    /// than converted per-form. That is upstream's guidance and it is what keeps
    /// this working when airlayer adds a tenth basis.
    ///
    /// `None` means the fit cannot be read as a marginal effect at all —
    /// refused, or curved with no profile to sample. **Both must mean no move.**
    /// Defaulting either to the raw coefficient is the silent 43x error this
    /// method exists to prevent.
    pub fn level_slope(&self) -> Option<f64> {
        if self.form == FitForm::Linear {
            return self.coefficient;
        }
        let lever = self.driver_value?;
        if !lever.is_finite() || lever <= 0.0 {
            return None;
        }
        let (dr, ddelta) = self.slope_across_zero()?;
        // ddelta is a change in the target; dr * lever is the change in the
        // driver that produced it.
        Some(ddelta / (dr * lever))
    }

    /// The standard error on [`Self::level_slope`], in the same units.
    ///
    /// Scaled by the same factor the point estimate was, which preserves the
    /// *relative* precision. First-order and it treats the operating point as
    /// known, so it understates the interval slightly — a gate marginally too
    /// eager is a smaller error than one applied in the wrong units.
    pub fn level_se(&self) -> Option<f64> {
        if self.form == FitForm::Linear {
            return Some(self.se);
        }
        let coefficient = self.coefficient?;
        if coefficient == 0.0 {
            return None;
        }
        Some((self.level_slope()? / coefficient * self.se).abs())
    }

    /// The two profile samples bracketing "no change", as `(Δr, Δdelta)`.
    ///
    /// Bracketing rather than the first two samples: the response is curved by
    /// construction, so its slope at `r = -0.9` is not its slope at the level
    /// the world is actually sitting at — which is `r = 0` by definition.
    fn slope_across_zero(&self) -> Option<(f64, f64)> {
        let below = self
            .profile
            .iter()
            .filter(|(r, _)| *r <= 0.0)
            .max_by(|a, b| a.0.total_cmp(&b.0))?;
        let above = self
            .profile
            .iter()
            .filter(|(r, _)| *r > below.0)
            .min_by(|a, b| a.0.total_cmp(&b.0))?;
        let dr = above.0 - below.0;
        if dr <= 0.0 {
            return None;
        }
        Some((dr, above.1 - below.1))
    }
}

/// Everything a policy is allowed to know at the top of a decision period.
#[derive(Debug, Clone)]
pub struct PeriodObservation<'a> {
    /// Spend per entity-day over the period just closed.
    pub current_spend: &'a [f64],
    /// Mean daily sales per entity over the trailing window — the legacy rule's
    /// input, and the only outcome signal a policy reads directly.
    pub trailing_sales: &'a [f64],
    /// The driver edge's fit. `None` when the baseline returned no row for it at
    /// all, which is a stronger silence than a refusal and treated the same way.
    pub fit: Option<EdgeFit>,
    /// Whether `predict` sized the candidate move, or degraded the impact to
    /// `unquantifiable`. An unsized impact means no move at all — the number on
    /// the surface would be a direction with no magnitude.
    pub impact_quantified: bool,
}

pub trait Policy {
    fn name(&self) -> &'static str;

    /// Spend per entity for the coming period.
    fn decide(&mut self, obs: &PeriodObservation<'_>) -> Vec<f64>;
}

/// Build the policy a spec declares.
///
/// `curve` is the world's solved truth, and it is threaded here only so
/// [`Oracle`] can be constructed. Every other arm ignores it.
/// The arm a run is, resolved against the world it runs on.
///
/// `policy` is an argument rather than a field of `spec` on purpose: the profit
/// race is only attributable if every arm sees the same world, same seed, same
/// shocks — which cannot be true when each arm is its own file.
pub fn build(policy: PolicyKind, spec: &SimulationSpec, curve: ResponseCurve) -> Box<dyn Policy> {
    let lever = Lever::new(&spec.lever, curve.anchor_spend);
    match policy {
        PolicyKind::Hold => Box::new(Hold),
        PolicyKind::Legacy => Box::new(Legacy::new(spec)),
        PolicyKind::Machine => Box::new(Machine::new(spec.baseline.margin, lever)),
        PolicyKind::MachineExplore => Box::new(MachineExplore::new(
            Machine::new(spec.baseline.margin, lever),
            lever,
            spec.seed,
            spec.lever.explore_jitter_sd,
        )),
        PolicyKind::Oracle => Box::new(Oracle::new(lever.clamp(curve.optimum_spend))),
    }
}

/// Spend bounds and the per-period clip, resolved to absolute money.
#[derive(Debug, Clone, Copy)]
struct Lever {
    min: f64,
    max: f64,
    clip: f64,
}

impl Lever {
    fn new(spec: &LeverSpec, anchor_spend: f64) -> Self {
        Self {
            min: spec.min_multiple * anchor_spend,
            max: spec.max_multiple * anchor_spend,
            clip: spec.max_move_per_period,
        }
    }

    fn clamp(&self, spend: f64) -> f64 {
        spend.clamp(self.min, self.max)
    }

    /// The largest move in `direction` this period allows.
    fn step(&self, current: f64, direction: f64) -> f64 {
        self.clamp(self.clamp(current) * (1.0 + direction * self.clip))
    }
}

/// Actions frozen. The null every other policy is scored against.
pub struct Hold;

impl Policy for Hold {
    fn name(&self) -> &'static str {
        "hold"
    }

    fn decide(&mut self, obs: &PeriodObservation<'_>) -> Vec<f64> {
        obs.current_spend.to_vec()
    }
}

/// Budget as a share of trailing sales — what a real chain does, and the rule
/// that puts the confounding in the data.
pub struct Legacy {
    share: f64,
    /// The world's declared spread, so the arm that *is* the customer's current
    /// behaviour keeps producing exactly the variation the burn-in did.
    jitter_sd: f64,
    /// Its own stream. The burn-in draws from `legacy_jitter`; if the policy
    /// shared it, how long a run lasted would change the history it started from.
    jitter: Rng,
}

impl Legacy {
    fn new(spec: &SimulationSpec) -> Self {
        Self {
            share: spec.mechanism.calibrate.anchor_spend_share,
            jitter_sd: spec.baseline.budget_jitter_sd,
            jitter: Rng::stream(spec.seed, "policy_legacy_jitter"),
        }
    }
}

impl Policy for Legacy {
    fn name(&self) -> &'static str {
        "legacy"
    }

    fn decide(&mut self, obs: &PeriodObservation<'_>) -> Vec<f64> {
        obs.trailing_sales
            .iter()
            .map(|sales| {
                let jitter = self.jitter.gauss(1.0, self.jitter_sd);
                legacy_budget(self.share, *sales, jitter)
            })
            .collect()
    }
}

/// Follow the fit toward zero marginal profit, clipped per period.
pub struct Machine {
    margin: f64,
    lever: Lever,
}

/// How many standard errors marginal profit must clear break-even before the
/// policy will move. Mirrors the fitter's own `abs t >= 2` discipline: acting on
/// a difference the data cannot resolve is how a policy converts noise into
/// spend.
const MIN_MOVE_T: f64 = 2.0;

impl Machine {
    fn new(margin: f64, lever: Lever) -> Self {
        Self { margin, lever }
    }

    /// `+1` to spend more, `−1` to spend less, `None` to hold.
    ///
    /// Every `None` here is a case where the shipped surface shows the user
    /// nothing actionable, so the policy does nothing. Collapsing any of them
    /// into a number is how this stops being a measurement of the product.
    fn direction(&self, obs: &PeriodObservation<'_>) -> Option<f64> {
        if !obs.impact_quantified {
            return None;
        }
        let fit = obs.fit.as_ref()?;
        // A refusal carries no coefficient — and emphatically does not carry a
        // zero one. `level_slope` also returns `None` for a non-linear fit with
        // no operating point, which is the same kind of silence: the number
        // exists but cannot be read as a marginal effect.
        let beta = fit.level_slope()?;
        let se = fit.level_se()?;
        if !beta.is_finite() || !se.is_finite() || se < 0.0 {
            return None;
        }

        let marginal_profit = self.margin * beta - 1.0;
        let resolvable = MIN_MOVE_T * self.margin * se;
        if marginal_profit.abs() <= resolvable {
            return None;
        }
        Some(marginal_profit.signum())
    }
}

impl Policy for Machine {
    fn name(&self) -> &'static str {
        "machine"
    }

    fn decide(&mut self, obs: &PeriodObservation<'_>) -> Vec<f64> {
        match self.direction(obs) {
            Some(direction) => obs
                .current_spend
                .iter()
                .map(|s| self.lever.step(*s, direction))
                .collect(),
            None => obs.current_spend.to_vec(),
        }
    }
}

/// The machine, plus a small randomized spread across entities.
///
/// The jitter is applied on **every** period, including the ones the machine
/// holds on — holding is precisely when variation would otherwise stop arriving,
/// and a perturbation that switches off with the policy would leave the
/// estimator with nothing at exactly the moment it needs something.
///
/// # Why this keeps its own ledger
///
/// The jitter has to be a *spread around a level*, and the level has to be the
/// machine's. Those two requirements pull against each other, because of how the
/// pieces already compose:
///
/// * [`Machine::decide`] steps **from** `obs.current_spend` — it returns it
///   verbatim when it holds, and `lever.step`s it when it moves. It has no other
///   idea of where it is.
/// * The runner reads `current_spend` back off the rows it wrote, so what
///   arrives is the **realized** spend — this arm's own jittered output from
///   last period.
///
/// Feed that back in and the composition is `s_t = clamp(s_{t−1} · exp(σZ_t))`:
/// a geometric random walk, whose cross-sectional spread grows as
/// `sqrt(sd₀² + t·σ²)` instead of holding at a width, and whose mean climbs
/// `exp(σ²/2)` per period because [`Rng::lognormal`] has unit *median*. That
/// climb is not bad luck — [`crate::spec::CalibrateSpec::solve`] refuses any
/// world whose optimum is not above the anchor, so the drift is always *toward*
/// the answer, and the arm scores points for diffusing rather than for deciding.
///
/// Anchoring to a fixed baseline would stop the diffusion and take the arm's
/// climb with it: the machine is *supposed* to walk the level, one clipped move
/// at a time. So neither end of the feedback loop is the anchor. The anchor is
/// the machine's own intent, which means this arm has to remember it: the
/// machine is stepped from `intended`, never from what the world actually ran,
/// and the jitter is one fresh layer applied to `intended` on the way out.
///
/// ```text
///   intended_t = machine.step(intended_{t−1})     ← no jitter in this line
///   realized_t = clamp(intended_t · exp(σZ_t))    ← exactly one layer, always
/// ```
///
/// **This changes every `machine+explore` number.** A run recorded before this
/// and one recorded after are measuring different policies — the older ones
/// scored a random walk that happened to drift toward the optimum — so they are
/// not comparable and should not be pooled into one grid.
pub struct MachineExplore {
    machine: Machine,
    lever: Lever,
    jitter: Rng,
    sd: f64,
    /// The machine's own trajectory, unpolluted by the jitter the world ran.
    ///
    /// Seeded from the first observation — the warm-up's per-entity spend, so
    /// the heterogeneity a real history left behind survives into the run — and
    /// re-seeded if the panel width ever changes under it, which would mean the
    /// ledger no longer describes the same entities.
    intended: Vec<f64>,
}

impl MachineExplore {
    fn new(machine: Machine, lever: Lever, seed: u64, sd: f64) -> Self {
        Self {
            machine,
            lever,
            jitter: Rng::stream(seed, "policy_explore"),
            sd,
            intended: Vec::new(),
        }
    }
}

impl Policy for MachineExplore {
    fn name(&self) -> &'static str {
        "machine+explore"
    }

    fn decide(&mut self, obs: &PeriodObservation<'_>) -> Vec<f64> {
        if self.intended.len() != obs.current_spend.len() {
            self.intended = obs.current_spend.to_vec();
        }

        // The machine sees where *it* left the lever, not where the jitter did.
        // Everything else in the observation is passed through untouched: this
        // substitutes the arm's ledger for one field, it does not manufacture a
        // fact the policy was not given.
        let intended = self.machine.decide(&PeriodObservation {
            current_spend: &self.intended,
            trailing_sales: obs.trailing_sales,
            fit: obs.fit.clone(),
            impact_quantified: obs.impact_quantified,
        });

        let realized = intended
            .iter()
            .map(|s| self.lever.clamp(s * self.jitter.lognormal(self.sd)))
            .collect();
        self.intended = intended;
        realized
    }
}

/// The world's true optimum, held. The ceiling regret is measured against.
pub struct Oracle {
    optimum_spend: f64,
}

impl Oracle {
    fn new(optimum_spend: f64) -> Self {
        Self { optimum_spend }
    }
}

impl Policy for Oracle {
    fn name(&self) -> &'static str {
        "oracle"
    }

    fn decide(&mut self, obs: &PeriodObservation<'_>) -> Vec<f64> {
        vec![self.optimum_spend; obs.current_spend.len()]
    }
}

#[cfg(test)]
mod tests;
