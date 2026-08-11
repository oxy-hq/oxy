//! Platform tests that don't belong to a single product domain — the compile
//! boundary, compiled readers, the CLI `build`/`run` commands, router shape in
//! local mode, project queries, workspace detail fields, and the partner-table
//! regression guard.
//!
//! One binary for the whole group; see `tests/authz/main.rs` for why. Add a case
//! as a `mod` here rather than a new `tests/*.rs`.
//!
//! Mixed three ways, and the split is in `.config/nextest.toml`:
//!
//! * `compiled_reader_semantic`, `toast_webhook_compile_boundary` and
//!   `airway_compile_boundary` are database-backed through [`common::fresh_db`]
//!   — own database each, so they sit in `db-per-test` (`max-threads = 4`).
//! * `projects_query` and `local_mode_router` call `api_router(..)`, which
//!   reaches the *shared* `OXY_DATABASE_URL` and runs `cleanup_stale_runs` — an
//!   unscoped UPDATE over `agentic_runs`. They are excluded from `db-per-test`
//!   and pinned into `serial-db` with the other shared-schema tests.
//! * everything else is in-process and ungrouped.
//!
//! `authz::shared_db_registry` fails the build if that classification drifts.

#[path = "../common/mod.rs"]
mod common;

mod airway_compile_boundary;
mod build;
mod compile;
mod compiled_reader_semantic;
mod local_mode_router;
mod no_dropped_partner_tables;
mod projects_query;
mod run;
mod toast_webhook_compile_boundary;
mod workspace_details_fields;
