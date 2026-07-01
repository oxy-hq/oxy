//! External-source reconciliation: compare an Oxy measure against a live
//! external source (Toast first) and fold drift into the health rollup.
pub(crate) mod compare;
pub(crate) mod config;
pub(crate) mod runner;
pub(crate) mod source;
pub(crate) mod toast;
pub(crate) mod window;

// Re-exports consumed across this module's siblings (`super::X`) and by the
// evaluator / sweep (`reconcile::X`). Items only ever referenced via their
// submodule path are intentionally not re-exported here.
pub(crate) use compare::{DriftVerdict, Tolerance, compare, error_verdict, unreachable_verdict};
pub(crate) use config::{Grain, Window};
pub(crate) use runner::{LiveReconcileRunner, ReconcileRunner};
