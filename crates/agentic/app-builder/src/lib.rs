//! Data app builder domain for the agentic workflow framework.
//!
//! Implements the [`AppBuilderDomain`] for the FSM pipeline in `agentic-core`.
//!
//! # Pipeline
//!
//! ```text
//! Intent (AppIntent)
//!   │
//!   ▼
//! Clarifying  ──► Specifying (AppSpec)
//!                   │
//!                   ▼
//!                 Solving (AppSolution — SQL per task)
//!                   │
//!                   ▼
//!                 Executing (AppResult — row samples)
//!                   │
//!                   ▼
//!                 Interpreting ──► Done (AppAnswer — YAML)
//! ```

pub mod config;
mod events;
mod schemas;
mod solver;
mod tools;
mod types;

// ── Events ────────────────────────────────────────────────────────────────────

pub use events::AppBuilderEvent;

// ── Schemas ───────────────────────────────────────────────────────────────────

pub use schemas::{solve_response_schema, specify_response_schema, triage_response_schema};

// ── Solver ────────────────────────────────────────────────────────────────────

pub use solver::{AppBuilderSolver, build_app_builder_handlers};

// ── Domain types ──────────────────────────────────────────────────────────────

pub use types::{
    AppAnswer, AppBuilderDomain, AppBuilderError, AppIntent, AppResult, AppSolution, AppSpec,
    AppValidator, ChartPreference, ChartType, ControlPlan, ControlType, LayoutNode, ResolvedTask,
    ResultShape, TaskPlan, TaskResult,
};

// ── Config re-exports ─────────────────────────────────────────────────────────

pub use config::{
    AppBuilderConfig, BuildContext, ConfigError, StateConfig, ThinkingConfigYaml,
    build_app_solver_with_context,
};
