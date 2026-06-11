//! `oxy-compile` — Phase 1.6a foundation of the compile boundary.
//!
//! Walks a workspace, parses every recognised YAML/SQL file, and
//! writes the result into the compile-boundary Postgres schema
//! introduced by migration `m20260606_000001_create_compile_boundary`.
//! The result is a single `revisions` row plus per-entity rows
//! (`agent_definitions`, `semantic_views`, `app_definitions`, etc.)
//! all tagged with the new `revision_id`.
//!
//! See `internal-docs/2026-06-06-compile-boundary-design.md` for the
//! design rationale and the multi-phase rollout plan. Phase 1.6a is
//! deliberately observation-mode:
//!
//!   - Compiles run.
//!   - Rows get written.
//!   - `workspaces.current_revision_id` is NOT updated.
//!   - Runtime keeps reading YAML from disk.
//!
//! That gives us production telemetry on what compiles cost and what
//! the rows look like before any read path depends on them.
//!
//! Public surface:
//!
//!   - [`compile_workspace`] — the end-to-end entry point used by
//!     `oxy compile`, by the future TaskSpec wrapper, and by tests.
//!   - [`CompileRequest`] / [`CompileOutcome`] — the request +
//!     response types.
//!   - [`errors::CompileError`] — run-level errors (DB unreachable,
//!     workspace missing). Per-file failures are recorded inside the
//!     `CompileOutcome.failures` vector, not raised.
//!
//! Boundaries:
//!
//!   - `walker.rs` is the only module that touches the filesystem.
//!   - `writer.rs` is the only module that touches the database.
//!   - `compile.rs` orchestrates and holds the parsing logic; no I/O
//!     of either kind except via the other two modules.
//!
//! This crate is `oxy-shared`-like: small, focused, no upward
//! dependencies on platform crates. The CLI command lives in
//! `oxy-app` and the future TaskSpec wrapper lives in
//! `agentic-pipeline`.

pub mod blob_store;
pub mod compile;
pub mod errors;
pub mod outcome;
pub mod walker;
pub mod workspace_path;
pub mod writer;

pub use compile::{compile_workspace, CompileRequest, RevisionKind, CURRENT_SCHEMA_VERSION};
pub use workspace_path::resolve_workspace_path;
pub use errors::CompileError;
pub use outcome::{CompileOutcome, FailureKind, FileFailure, RevisionStatus};

/// Identifier the compile worker stamps on every `revisions` row so
/// operator dashboards can spot version-skew across a rolling deploy.
/// Sourced from `CARGO_PKG_VERSION` so a release-bumped oxy binary
/// automatically writes a different value without code changes.
pub fn compiler_version() -> String {
    format!("oxy-compile/{}", env!("CARGO_PKG_VERSION"))
}
