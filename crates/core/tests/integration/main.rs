//! Integration tests for `oxy (core)`, in ONE binary.
//!
//! Cargo compiles every `tests/*.rs` into its own executable, each statically
//! linking the whole dependency graph — so a test *file* costs a link, not just
//! a compile. Grouping keeps that cost proportional to crates rather than to
//! files. Add a case as a `mod` below, not as a new `tests/*.rs`.
//!
//! See `internal-docs/testing.md` for the cost model.

mod config_parsing;
mod omni_query_workflow;
mod semantic_variables;
mod serve;
mod strict_validation;
mod sync;
mod validation;
