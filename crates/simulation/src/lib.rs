//! Declared worlds with a known truth, run forward against the semantic layer.
//!
//! The scenario forecast fits a coefficient from history and propagates it. This
//! crate builds worlds where that coefficient is something *we* chose, so a run
//! can ask the two questions a customer's warehouse never can: did the estimate
//! reach the truth, and did acting on it pay.
//!
//! # The one invariant
//!
//! **True parameters reach the scorer and nothing else.** The world generates
//! ordinary rows; the fitter sees only those rows, through the same path a
//! customer's data takes. Any shortcut that lets a policy read [`ResponseCurve`]
//! turns a measurement into a tautology.
//!
//! See `internal-docs/2026-08-12-simulation-in-oxygen-plan.md`.

pub mod check;
pub mod policy;
pub mod race;
pub mod readiness;
pub mod rng;
pub mod runner;
pub mod spec;
pub mod world;

pub use check::{WorldCheck, check};
pub use policy::{EdgeFit, FitForm, PeriodObservation, Policy};
pub use race::{
    ArmProfits, ArmSummary, Inference, NoInference, PairedComparison, PairedTest, profit_race,
};
pub use readiness::{Concern, PanelPair, Readiness};
pub use rng::Rng;
pub use runner::{
    FitScore, Outcome, PeriodResult, Probe, RowSink, RunSummary, Runner, SemanticProbe,
};
pub use spec::{
    BaselineSpec, CalibrateSpec, DEFAULT_BUDGET_JITTER_SD, EntitiesSpec, LeverSpec, MechanismSpec,
    PolicyKind, ResponseCurve, SimulationSpec,
};
pub use world::{EntityDay, World, total_profit, trailing_mean_sales};

#[derive(Debug, thiserror::Error)]
pub enum SimulationError {
    /// The declared world is not coherent — an unreachable optimum, a history
    /// too short to fit on. Raised before any rows are generated.
    #[error("invalid simulation spec: {0}")]
    Spec(String),

    /// The rows a world emits no longer carry the mechanism it declares. Always
    /// a bug in the engine, never in a spec — and the one failure that would
    /// otherwise be invisible, since a drifted world still produces a clean run.
    #[error("world drift: {0}")]
    Drift(String),

    /// The world could not be observed at all — its layer would not parse, its
    /// engine would not build, or a warehouse read failed. Distinct from
    /// [`SimulationError::Drift`] on purpose: drift is a claim ABOUT the rows,
    /// and filing "the connection reset" under it tells an operator the world
    /// stopped carrying its mechanism when in fact nobody managed to look.
    /// That mislabelling is the same failure this crate measures one level up,
    /// where a broken read scored as `Refused` was indistinguishable from the
    /// model honestly declining.
    #[error("could not read the world: {0}")]
    Read(String),

    /// A generated row, or a period's fit, could not be persisted — the sink
    /// disk is full, or the run's own record write to Postgres failed.
    /// Distinct from [`SimulationError::Read`] for the same reason `Read` was
    /// split out of [`SimulationError::Drift`]: "could not read the world"
    /// under a variant named `Read` would itself be a false label for a
    /// failure that never touched a read path, and folding it into `Drift`
    /// would again tell an operator the world stopped carrying its mechanism
    /// when in fact nobody managed to write it down.
    #[error("could not write the world: {0}")]
    Write(String),
}
