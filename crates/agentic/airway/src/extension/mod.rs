//! Airway extension entities + migrator.
//!
//! Three aggregate roots managed by a single `AirwayMigrator`
//! (tracking table `seaql_migrations_airway`):
//!
//! - [`pipeline_state`] — `airway_pipeline_state` per pipeline_name
//! - [`load_audit`] — `airway_load_audit` per load_id
//! - [`run_extension`] — `airway_run_extensions` per `agentic_runs.id`
//!
//! See [`internal-docs/airway-crate-layout.md`] for the schema
//! definitions and the aggregate-boundary rationale.

pub mod load_audit;
pub mod migration;
pub mod pipeline_state;
pub mod run_extension;

pub use migration::AirwayMigrator;
