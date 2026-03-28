//! Vendor semantic engine integration.
//!
//! Defines the [`SemanticEngine`] trait and supporting types for delegating
//! query execution to external semantic engines (Cube, Looker, etc.).
//!
//! # Architecture
//!
//! The engine path is the **highest-priority** routing in the Specifying state:
//!
//! ```text
//! 1. engine configured + translate() succeeds → VendorEngine (skip Solving)
//! 2. vendor path not taken + try_compile succeeds → SemanticLayer (skip Solving)
//! 3. try_compile returns TooComplex → LlmWithSemanticContext (enter Solving)
//! 4. Unresolvable → fatal error
//! ```
//!
//! `TranslationFailed` → graceful fall-through to path 2/3.
//! `EngineUnreachable` → hard failure at startup, never fall-through.

pub mod cube;
pub mod looker;
pub mod translate;

use agentic_core::result::QueryResult;
use async_trait::async_trait;

use crate::catalog::{DimensionSummary, JoinPath, MetricDef};
use crate::types::AnalyticsIntent;

// ── VendorQuery ───────────────────────────────────────────────────────────────

/// An opaque vendor-native query payload produced by [`SemanticEngine::translate`]
/// and consumed by the same engine's [`SemanticEngine::execute`].
///
/// The payload never crosses engine boundaries — each engine produces and
/// consumes its own format.
#[derive(Debug, Clone)]
pub struct VendorQuery {
    /// The JSON body held opaque by the routing layer.
    pub payload: serde_json::Value,
}

// ── EngineError ───────────────────────────────────────────────────────────────

/// Errors returned by [`SemanticEngine`] methods.
///
/// The variant determines how the routing layer handles the failure:
///
/// | Variant | Where caught | Action |
/// |---------|-------------|--------|
/// | `TranslationFailed` | Specifying handler | Fall through to `try_compile` / LLM |
/// | `EngineUnreachable` | Startup (`ping()`) | Hard failure — `ConfigError::EngineConnectionError` |
/// | `ApiError` / `Transport` | Executing handler | `AnalyticsError::VendorError` |
#[derive(Debug)]
pub enum EngineError {
    /// The specific query could not be expressed in the vendor's query format
    /// (e.g. metric not in vendor namespace, unsupported filter operator).
    ///
    /// **Graceful fall-through**: routing drops to `try_compile` / LLM path.
    /// Only returned from [`SemanticEngine::translate`], never from `execute` or `ping`.
    TranslationFailed(String),

    /// The engine could not be reached or authenticated during the startup
    /// health-check.
    ///
    /// **Hard failure**: surfaced as `ConfigError::EngineConnectionError` and
    /// prevents the solver from starting. Never silently degrades to LLM fallback.
    /// Only returned from [`SemanticEngine::ping`], never from `translate` or `execute`.
    EngineUnreachable(String),

    /// The vendor API returned a query-level error after the engine was
    /// successfully reached (e.g. invalid member name in the submitted query).
    ApiError { status: u16, body: String },

    /// Network or serialisation error during query execution.
    Transport(String),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::TranslationFailed(msg) => write!(f, "translation failed: {msg}"),
            EngineError::EngineUnreachable(msg) => write!(f, "engine unreachable: {msg}"),
            EngineError::ApiError { status, body } => {
                write!(f, "vendor API error {status}: {body}")
            }
            EngineError::Transport(msg) => write!(f, "transport error: {msg}"),
        }
    }
}

impl std::error::Error for EngineError {}

// ── TranslationContext ────────────────────────────────────────────────────────

/// All catalog data needed to translate an [`AnalyticsIntent`] into a vendor query.
///
/// Built by the Specifying handler from [`Catalog`][crate::catalog::Catalog] trait methods.
#[derive(Debug, Clone)]
pub struct TranslationContext {
    /// Metric definitions for every metric named in the intent.
    pub metrics: Vec<MetricDef>,
    /// Dimension summaries for every dimension named in the intent.
    pub dimensions: Vec<DimensionSummary>,
    /// Join paths relevant to the intent's tables.
    pub join_paths: Vec<(String, String, JoinPath)>,
}

// ── SemanticEngine trait ──────────────────────────────────────────────────────

/// Interface for external vendor semantic engines (Cube, Looker, etc.).
///
/// # Contract
///
/// - `translate` is **pure** — no I/O, no side effects. It may be called
///   multiple times and must be deterministic given the same inputs.
/// - `ping` is called **once at startup**. If it returns `EngineUnreachable`,
///   the solver fails to build. It is never called again.
/// - `execute` is called for each query on the vendor path. It must never
///   return `TranslationFailed` or `EngineUnreachable`.
#[async_trait]
pub trait SemanticEngine: Send + Sync {
    /// A stable, human-readable vendor label (e.g. `"cube"`, `"looker"`).
    ///
    /// Used in [`SolutionSource::VendorEngine`][crate::types::SolutionSource],
    /// telemetry, and error messages.
    fn vendor_name(&self) -> &str;

    /// Translate catalog context + analytics intent into a vendor-native query.
    ///
    /// **Pure** — no I/O. Returns [`EngineError::TranslationFailed`] when the
    /// specific query cannot be expressed in the vendor's format; the routing
    /// layer will fall through to `try_compile` / LLM in that case.
    fn translate(
        &self,
        ctx: &TranslationContext,
        intent: &AnalyticsIntent,
    ) -> Result<VendorQuery, EngineError>;

    /// Lightweight connectivity check run **once at startup**.
    ///
    /// Returns [`EngineError::EngineUnreachable`] if the engine cannot be
    /// contacted or authentication fails. This is the **only** method that
    /// may return `EngineUnreachable`.
    async fn ping(&self) -> Result<(), EngineError>;

    /// Execute a pre-translated vendor query and return rows as [`QueryResult`].
    ///
    /// Must never return `TranslationFailed` or `EngineUnreachable`.
    async fn execute(&self, query: &VendorQuery) -> Result<QueryResult, EngineError>;
}
