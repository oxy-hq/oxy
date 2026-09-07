//! `.simulation.yml` — the declared world, and the constants solved from it.
//!
//! The spec states *intent* ("the optimum should sit 3× above today's spend") and
//! this module solves the parameters that produce it. Nothing here is guessed: a
//! world whose optimum lands somewhere the levers cannot reach silently tests
//! nothing, and an author cannot eyeball where a power curve turns.

use serde::{Deserialize, Serialize};

use crate::SimulationError;

pub mod curve;

pub use curve::ResponseCurve;

/// A declared world.
///
/// # What is *not* here
///
/// **The policy.** A world is what happens; a policy is what someone does about
/// it, and the same world has to be runnable under `hold`, `machine` and
/// `oracle` or the profit race compares two different worlds. It is chosen when
/// a run is queued and recorded on the run — see `POST /simulations/{name}/runs`.
///
/// `deny_unknown_fields`, here and on every nested block, because the silent
/// failure this crate exists to catch is exactly the one an ignored field
/// produces: a mistyped `noise_ratio` would default, the world would still be
/// well-formed, and the run would report a confident estimate of a world nobody
/// declared.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationSpec {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub seed: u64,
    /// How many seeds this world is worth running at.
    ///
    /// A property of the *world*, not of a run: how many draws it takes before
    /// a cell of the outcome map can be called is set by how noisy the world is,
    /// which is declared right here. One seed on a marginal world classifies the
    /// draw, not the world — the plan's own 6-panel finding (an estimator that
    /// came back at −0.84) is a cell where one replicate would have reported
    /// `confidently_wrong` as if it meant something.
    ///
    /// Replicate 0 always runs the declared `seed`, so a single-replicate world
    /// reproduces exactly what it did before this field existed.
    #[serde(default = "one")]
    pub replicates: u32,
    /// Decision periods the loop runs for.
    pub periods: u32,
    /// Days per decision period. The policy acts once per period; rows are daily.
    pub period_days: u32,
    /// Days of history generated under the opening spend before the loop starts.
    ///
    /// A customer has a past when we arrive. It also has to clear the fitter's
    /// floors — `n >= 30` pairs after the lag — or period 1 fits nothing and the
    /// first thing the run shows is a refusal that means "too early", not
    /// "unidentified".
    pub history_days: u32,
    pub start_date: chrono::NaiveDate,
    pub entities: EntitiesSpec,
    pub baseline: BaselineSpec,
    pub mechanism: MechanismSpec,
    #[serde(default)]
    pub lever: LeverSpec,
}

fn one() -> u32 {
    1
}

/// What a policy is allowed to do to the driver.
///
/// Every field is a *policy* constraint, not a world one — the world will
/// happily generate rows at any spend. These exist because an unconstrained
/// policy measures nothing: it either walks straight to a bound on the first
/// period, or oscillates at an amplitude that has no counterpart in how a real
/// chain moves a budget.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LeverSpec {
    /// Floor on spend, as a multiple of the anchor.
    ///
    /// Deliberately **not** zero. The move is multiplicative, so a hard zero
    /// floor is absorbing: once a policy reaches it, no proportional step can
    /// ever climb back out, and the run reports a collapse that is an artefact
    /// of the step rule rather than anything about the estimate.
    pub min_multiple: f64,
    /// Ceiling on spend, as a multiple of the anchor.
    pub max_multiple: f64,
    /// Largest fractional change one decision period may make to spend.
    ///
    /// This is the only thing bounding a `machine` move. The fit is linear, so
    /// in the model's own view marginal profit does not fall as spend rises —
    /// it believes "spend more" for ever. What actually turns the policy around
    /// is the *next* period's refit landing on a flatter part of the true curve.
    pub max_move_per_period: f64,
    /// Log-space spread of the `machine+explore` jitter across entities.
    pub explore_jitter_sd: f64,
}

impl Default for LeverSpec {
    fn default() -> Self {
        Self {
            min_multiple: 0.1,
            max_multiple: 5.0,
            max_move_per_period: 0.25,
            explore_jitter_sd: 0.15,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EntitiesSpec {
    /// Panels. The fit is within-panel, and `dof = n - (n_panels + k)`, so this
    /// is not free.
    pub count: u32,
    /// Log-space spread of entity size. This is what within-panel demeaning
    /// exists to remove, so a world with no spread cannot show it working.
    pub scale_sigma: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineSpec {
    /// Sales for a typical entity on a typical day, before any marketing effect.
    pub sales_per_entity_day: f64,
    /// Contribution margin. Sets where the profit optimum lands, and converts
    /// sales into the objective.
    pub margin: f64,
    /// AR(1) persistence of the latent demand shock. This is the confounder a
    /// `legacy` policy correlates spend with.
    pub demand_shock_rho: f64,
    pub demand_shock_sd: f64,
    /// Amplitude of the weekly cycle, as a fraction of baseline.
    pub weekly_seasonality: f64,
    /// Idiosyncratic multiplicative spread on the budget rule — how much spend
    /// moves for reasons that are *not* the demand shock.
    ///
    /// This is the **identification** axis, and the plan calls it the one most
    /// customers fail: it is the only variation in the regressor that is not the
    /// confounder itself, so at zero the honest answer is always "unidentified"
    /// and the confounding axis cannot be turned independently of it. It lives
    /// here rather than as a constant because a grid that cannot sweep it cannot
    /// draw the left-hand column of the outcome map.
    ///
    /// Distinct from `lever.explore_jitter_sd`, which belongs to a *policy*:
    /// this one is the world's own opening history and what the `legacy` arm
    /// keeps producing.
    #[serde(default = "default_budget_jitter_sd")]
    pub budget_jitter_sd: f64,
}

/// Enough movement to identify a slope on a world that is otherwise well
/// behaved. The value the declared grid was measured at before it was a field.
pub const DEFAULT_BUDGET_JITTER_SD: f64 = 0.12;

fn default_budget_jitter_sd() -> f64 {
    DEFAULT_BUDGET_JITTER_SD
}

/// Columns every generated world declares regardless of `driver`/`target`:
/// the grain (`entity_id`, `date`) and the fixed cost measure (`prime_cost`).
/// A world naming its driver or target one of these would collide with a
/// column `world_dir::view_yml` already declares.
const RESERVED_COLUMN_NAMES: [&str; 3] = ["entity_id", "date", "prime_cost"];

/// `^[A-Za-z_][A-Za-z0-9_]*$` — the class a `driver`/`target` must sit in.
///
/// The value is interpolated raw, never quoted or escaped, into three places
/// downstream (`world_dir::csv_header` and, in `world_dir::view_yml`, a
/// measure `name:`, its `expr:` and a `drivers.measure` path). A comma splits
/// the CSV header; a colon, quote, newline or leading `#` breaks the YAML; a
/// space or leading digit is not a column any SQL engine resolves bare; a dot
/// reads as a `view.member` path. ASCII only, deliberately: what survives all
/// three layers unescaped is exactly this class, and a rule the author can
/// read off the error is worth more than a wider one.
fn is_bare_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// One driver → target mechanism. Phase 1 carries exactly one.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MechanismSpec {
    /// Bare column name — becomes both the generated CSV's column header
    /// (`world_dir::csv_header`) and the matching measure's `name:`/`expr:`
    /// in the generated `.view.yml` (`world_dir::view_yml`), which always
    /// declares one view. Written verbatim into both, never quoted or
    /// escaped, so it must be a bare identifier
    /// (`^[A-Za-z_][A-Za-z0-9_]*$`): a `view.member` path here is not a
    /// reference to some other workspace's measure, and a comma, colon,
    /// quote or space would corrupt the CSV header or the YAML downstream.
    /// Also may not collide with a reserved column (`entity_id`, `date`,
    /// `prime_cost`) or with `target`. All caught in
    /// [`SimulationSpec::validate`].
    pub driver: String,
    /// Same constraints as `driver`, same reasons.
    pub target: String,
    /// Days between the spend and the sales it produces. **The truth**, and what
    /// the world generates against.
    pub lag_days: u32,
    /// The lag the generated `.view.yml` claims, when it is not the true one.
    ///
    /// `lag:` is declared by a human and never fitted, so on real data it is a
    /// *guess* — which is why "lag error" is an axis of the outcome map. One
    /// field could not carry both roles: reading the same number twice makes the
    /// customer right by construction, and the axis unreachable.
    ///
    /// `None` means the customer guessed right, which is the interesting case
    /// exactly once.
    #[serde(default)]
    pub declared_lag_days: Option<u32>,
    /// Noise on the target, as a fraction of the baseline level.
    pub noise_ratio: f64,
    pub calibrate: CalibrateSpec,
}

impl MechanismSpec {
    /// The lag the fitter is told about — what pairs day `d`'s driver with day
    /// `d + lag`'s target, and therefore what sets `n`.
    pub fn declared_lag(&self) -> u32 {
        self.declared_lag_days.unwrap_or(self.lag_days)
    }
}

/// What the response curve has to satisfy. `theta` and the scale are solved from
/// these rather than declared, because the relationship between them is not
/// something an author can hold in their head.
///
/// # The anchor is not the operating point
///
/// These pin the *shape* of the curve at one reference spend. Where the world
/// actually settles is a different question, and under a budget set as a share of
/// revenue it is a fixed point: spend raises sales, which raises the budget, which
/// raises sales. [`crate::check`] reports the settled mean spend and the true
/// local slope there, and that slope — not `local_slope_at_anchor` — is what a fit
/// should be scored against.
///
/// Watch the implied magnitudes. A power curve has `R = R'·s/θ`, so a large
/// marginal return at a large spend share means marketing explains an implausible
/// fraction of revenue: `local_slope_at_anchor` 6.0 at an 8.5% share puts the
/// marginal contribution at 51% of sales, and the total response above 100% of
/// base. Those are `example_new`'s constants, and they only look reasonable there
/// because that fixture is built backwards — nothing ever had to close forward.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalibrateSpec {
    /// Reference spend, as a share of baseline daily sales. Anchors the curve; see
    /// the note above on why the world will not sit here.
    pub anchor_spend_share: f64,
    /// Marginal sales per unit of spend, evaluated at the anchor spend.
    pub local_slope_at_anchor: f64,
    /// Where the profit optimum should sit, as a multiple of the anchor spend.
    pub optimum_at: f64,
}

/// Which arm of the experiment a run is.
///
/// **Not** part of [`SimulationSpec`]. `hold` and `machine` over one world are a
/// profit race; over two files they are two worlds that happen to look alike,
/// and the comparison means nothing. Chosen per run, recorded on the run row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PolicyKind {
    Hold,
    Legacy,
    /// The product, and the default arm: a run nobody parameterised is a run of
    /// the thing we ship.
    #[default]
    Machine,
    /// The plan and the charts both write this one `machine+explore`; the alias
    /// keeps a hand-typed run request readable without forcing the enum to carry
    /// a character serde would otherwise have to be told about at every use.
    #[serde(alias = "machine+explore")]
    MachineExplore,
    Oracle,
}

impl PolicyKind {
    /// Every arm, in the order a profit race reads: the null, what a customer
    /// does today, the product, the candidate fix, the ceiling.
    pub const ALL: [PolicyKind; 5] = [
        PolicyKind::Hold,
        PolicyKind::Legacy,
        PolicyKind::Machine,
        PolicyKind::MachineExplore,
        PolicyKind::Oracle,
    ];

    /// The wire spelling — what a run row stores and what the API accepts.
    ///
    /// Matches serde's `snake_case`, and exists because `format!("{:?}")` does
    /// not: it renders `MachineExplore` as `machineexplore`, which round-trips
    /// through neither this type nor a `.simulation.yml`.
    pub fn as_str(self) -> &'static str {
        match self {
            PolicyKind::Hold => "hold",
            PolicyKind::Legacy => "legacy",
            PolicyKind::Machine => "machine",
            PolicyKind::MachineExplore => "machine_explore",
            PolicyKind::Oracle => "oracle",
        }
    }
}

impl std::str::FromStr for PolicyKind {
    type Err = SimulationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Same alias serde carries, applied before the lookup rather than inside
        // it: an `||` in the predicate matches on the *first* arm tried, which
        // silently turns `machine+explore` into `hold`.
        let normalized = match s.trim().to_lowercase().as_str() {
            "machine+explore" => "machine_explore".to_string(),
            other => other.to_string(),
        };
        PolicyKind::ALL
            .into_iter()
            .find(|p| p.as_str() == normalized)
            .ok_or_else(|| {
                SimulationError::Spec(format!(
                    "unknown policy '{s}' — expected one of {}",
                    PolicyKind::ALL.map(|p| p.as_str()).join(", ")
                ))
            })
    }
}

impl SimulationSpec {
    pub fn from_yaml(source: &str) -> Result<Self, SimulationError> {
        let spec: SimulationSpec =
            serde_yaml::from_str(source).map_err(|e| SimulationError::Spec(e.to_string()))?;
        spec.validate()?;
        Ok(spec)
    }

    /// Same contract as [`Self::from_yaml`], for a caller that already has JSON
    /// rather than a YAML source — a form posting a candidate world, in
    /// particular, which wants exactly the checks below run against a spec
    /// nobody has written to a file yet.
    pub fn from_value(value: serde_json::Value) -> Result<Self, SimulationError> {
        let spec: SimulationSpec =
            serde_json::from_value(value).map_err(|e| SimulationError::Spec(e.to_string()))?;
        spec.validate()?;
        Ok(spec)
    }

    /// The solved response curve for this world.
    pub fn curve(&self) -> Result<ResponseCurve, SimulationError> {
        self.mechanism
            .calibrate
            .solve(self.baseline.margin, self.baseline.sales_per_entity_day)
    }

    /// Every rule a declared world must satisfy — an unreachable optimum, an
    /// absorbing lever floor, too little history to clear the fitter's floor,
    /// and so on. `pub` so a caller can check a candidate spec (a UI form, in
    /// particular) before it is ever written to a `.simulation.yml`, without
    /// duplicating these rules in a second language.
    pub fn validate(&self) -> Result<(), SimulationError> {
        if self.entities.count == 0 {
            return Err(SimulationError::Spec("entities.count must be > 0".into()));
        }
        if self.periods == 0 || self.period_days == 0 {
            return Err(SimulationError::Spec(
                "periods and period_days must both be > 0".into(),
            ));
        }
        if self.replicates == 0 {
            return Err(SimulationError::Spec(
                "replicates must be >= 1 — a world nobody runs declares nothing".into(),
            ));
        }
        // Open at both ends, as the message says. `(0.0..1.0).contains` admits
        // zero, and a zero margin then reaches `CalibrateSpec::solve`, which
        // divides by it and reports "raise local_slope_at_anchor above inf" —
        // advice about a different field. `contains` still rejects NaN.
        if !(0.0..1.0).contains(&self.baseline.margin) || self.baseline.margin <= 0.0 {
            return Err(SimulationError::Spec(format!(
                "baseline.margin must be in (0, 1) — got {}",
                self.baseline.margin
            )));
        }
        if !self.baseline.budget_jitter_sd.is_finite() || self.baseline.budget_jitter_sd < 0.0 {
            return Err(SimulationError::Spec(
                "baseline.budget_jitter_sd must be finite and >= 0. Zero is legal and is the \
                 point of the flat-lever corner: spend then moves only with the confounder, and \
                 the honest answer becomes 'unidentified'."
                    .into(),
            ));
        }
        if self.baseline.demand_shock_rho.abs() >= 1.0 {
            return Err(SimulationError::Spec(
                "baseline.demand_shock_rho must be in (-1, 1), or the shock is not stationary and \
                 the world drifts without bound"
                    .into(),
            ));
        }

        if self.lever.min_multiple <= 0.0 || self.lever.min_multiple >= self.lever.max_multiple {
            return Err(SimulationError::Spec(
                "lever.min_multiple must be positive and below max_multiple — a zero floor is \
                 absorbing under a multiplicative step"
                    .into(),
            ));
        }
        if !(0.0..1.0).contains(&self.lever.max_move_per_period)
            || self.lever.max_move_per_period == 0.0
        {
            return Err(SimulationError::Spec(
                "lever.max_move_per_period must be in (0, 1)".into(),
            ));
        }

        // `driver`/`target` become a measure `name:` — and, via
        // `world_dir::csv_header`, an actual CSV column — in the one view
        // this world generates, then get re-qualified as `{view}.{name}` to
        // build the driver edge and the fit's roots. Every step is plain
        // string interpolation — nothing quotes, escapes or rejects — so a
        // value outside the identifier class only fails three layers down:
        // a dot as an unreadable `view.a.b` inside airlayer's member-path
        // parser, a comma as a CSV header with one column too many, a colon
        // or quote as a YAML parse error in a file the author never wrote.
        // None of those point back at the world file.
        for (field, name) in [
            ("driver", &self.mechanism.driver),
            ("target", &self.mechanism.target),
        ] {
            if !is_bare_identifier(name) {
                return Err(SimulationError::Spec(format!(
                    "mechanism.{field} must be a bare column name: a letter or underscore, then \
                     letters, digits or underscores (`^[A-Za-z_][A-Za-z0-9_]*$`). It names a \
                     measure in the one view this world generates and is written verbatim into \
                     that view's YAML and the dataset's CSV header, not a reference to a \
                     workspace view. Got '{name}'."
                )));
            }
            if RESERVED_COLUMN_NAMES.contains(&name.as_str()) {
                return Err(SimulationError::Spec(format!(
                    "mechanism.{field} may not be '{name}' — that column is already declared \
                     by every generated world ({}).",
                    RESERVED_COLUMN_NAMES.join(", ")
                )));
            }
        }
        if self.mechanism.driver == self.mechanism.target {
            return Err(SimulationError::Spec(format!(
                "mechanism.driver and mechanism.target must be different columns — both are \
                 '{}'.",
                self.mechanism.driver
            )));
        }

        if self.mechanism.declared_lag() == 0 {
            return Err(SimulationError::Spec(
                "mechanism.declared_lag_days must be > 0 — a lag of zero pairs a day with itself"
                    .into(),
            ));
        }

        // Deliberately NOT refused here: `period_days < mechanism.lag_days`.
        // It breaks `check::sales_at_flat_spend`'s two-step measurement window
        // and nothing else, so `check` is where it is refused. `validate` runs
        // from both `from_yaml` and `from_value`, so a rule added here gates
        // every world load and every form POST — and the *runner* handles the
        // short-period case correctly: `generate_day` pushes one day's spend
        // and pops one matured entry per day, so the lag is carried by the
        // queue's depth and never by `period_days`. A daily-decision world
        // against a weekly lag is exactly the regime where a naive fitter is
        // most wrong, which is to say the regime this crate exists to measure.
        // Refusing to declare it would remove the measurement, not the bug.

        // The fit pairs day d's driver with day d+lag's target within a panel, so
        // history shorter than the lag yields no pairs at all — the run would open
        // on a refusal that reads as "unidentified" when it only means "too early".
        //
        // Against the **declared** lag, not the true one: `n` is whatever the
        // fitter can pair, and the fitter only knows what the `.view.yml` claims.
        // A lag-error world is precisely one where the two differ.
        let declared_lag = self.mechanism.declared_lag();
        let pairs_per_entity = self.history_days.saturating_sub(declared_lag);
        let pairs = pairs_per_entity as u64 * self.entities.count as u64;
        if pairs < MIN_FIT_OBSERVATIONS {
            return Err(SimulationError::Spec(format!(
                "history_days {} with declared lag {declared_lag} over {} entities yields {pairs} \
                 paired observations, under the fitter's floor of {MIN_FIT_OBSERVATIONS}. Period \
                 1 would refuse for lack of history rather than for lack of identification.",
                self.history_days, self.entities.count
            )));
        }

        // Solving is validation: an unreachable optimum is a spec error, and it is
        // far cheaper to find here than after a run produces a policy that walks
        // to a bound and stops.
        self.curve()?;
        Ok(())
    }
}

/// Mirrors `MIN_FIT_OBSERVATIONS` in the airlayer fitter. Duplicated rather than
/// imported because this crate is deliberately free of the engine it validates —
/// see the scoring-independence decision in the plan. `spec_rejects_history_below_the_fit_floor`
/// is what catches the two drifting apart.
pub const MIN_FIT_OBSERVATIONS: u64 = 30;

#[cfg(test)]
mod tests;
