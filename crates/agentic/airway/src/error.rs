//! Error type for the airway runtime.

use thiserror::Error;

/// Errors produced by `agentic-airway`.
///
/// Wraps `airway::AirwayError` (the engine's error type) plus oxy-side
/// concerns — config parsing, source/destination factory dispatch,
/// state-store conflicts. The worker converts these into `String`
/// outcomes at the runtime boundary.
#[derive(Debug, Error)]
pub enum AirwayError {
    /// Propagated from the airway engine — extract/normalize/load
    /// failures, state-store conflicts, cancellation.
    #[error(transparent)]
    Engine(#[from] airway::AirwayError),

    /// Catch-all for oxy-side dispatch / parse failures.
    #[error("airway: {0}")]
    Other(String),
}
