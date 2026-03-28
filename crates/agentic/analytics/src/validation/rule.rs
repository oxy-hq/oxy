//! Core validation traits and context types.
//!
//! Each pipeline stage has a dedicated trait and a context struct that carries
//! exactly the inputs that stage's rules need.  Rules receive a reference to
//! the context and return the **first** error they encounter (fail-fast).
//!
//! | Trait | Stage | Context |
//! |---|---|---|
//! | [`SpecifiedRule`] | `specify` | [`SpecifiedCtx`] |
//! | [`SolvableRule`] | `solve`   | [`SolvableCtx`] |
//! | [`SolvedRule`]   | `execute` | [`SolvedCtx`]   |

use crate::semantic::SemanticCatalog;
use crate::{AnalyticsError, AnalyticsResult, QuerySpec};

// ---------------------------------------------------------------------------
// Context structs
// ---------------------------------------------------------------------------

/// Inputs available to rules that run after the **Specify** stage.
pub struct SpecifiedCtx<'a> {
    pub spec: &'a QuerySpec,
    pub catalog: &'a SemanticCatalog,
}

/// Inputs available to rules that run after the **Solve** stage.
pub struct SolvableCtx<'a> {
    pub sql: &'a str,
    pub spec: &'a QuerySpec,
    pub catalog: &'a SemanticCatalog,
}

/// Inputs available to rules that run after the **Execute** stage.
pub struct SolvedCtx<'a> {
    pub result: &'a AnalyticsResult,
    pub spec: &'a QuerySpec,
}

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

/// A named, configurable validation check for the **Specify** stage.
///
/// Implementors live in `specified.rs` and are registered by name in
/// [`crate::validation::registry::RuleRegistry`].
pub trait SpecifiedRule: Send + Sync {
    /// Unique snake_case identifier referenced in the YAML config.
    fn name(&self) -> &'static str;
    /// One-line description shown in documentation and error context.
    fn description(&self) -> &'static str;
    /// Run the check.  Returns `Ok(())` or the first [`AnalyticsError`].
    fn check(&self, ctx: &SpecifiedCtx<'_>) -> Result<(), AnalyticsError>;
}

/// A named, configurable validation check for the **Solve** stage.
pub trait SolvableRule: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn check(&self, ctx: &SolvableCtx<'_>) -> Result<(), AnalyticsError>;
}

/// A named, configurable validation check for the **Execute** stage.
pub trait SolvedRule: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn check(&self, ctx: &SolvedCtx<'_>) -> Result<(), AnalyticsError>;
}
